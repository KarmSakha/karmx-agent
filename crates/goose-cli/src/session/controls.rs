use super::{build_session, output, CliSession, SessionBuilderConfig};
use anyhow::Result;
use goose::config::profile::PromptProfile;
use goose::hooks::HookEvent;
use goose::utils::safe_truncate;
use std::io::IsTerminal;

impl CliSession {
    pub(super) async fn handle_resume(&mut self, id: Option<String>) -> Result<()> {
        let manager = self.agent.config.session_manager.clone();
        let id = match id {
            Some(id) => id,
            None => {
                let mut sessions = manager.list_sessions().await?;
                sessions.retain(|session| {
                    session.id != self.session_id && session.archived_at.is_none()
                });
                sessions.sort_by_key(|session| {
                    std::cmp::Reverse(session.last_message_at.unwrap_or(session.updated_at))
                });
                if sessions.is_empty() {
                    println!("No other saved sessions. Start a new session with /new.");
                    return Ok(());
                }
                if !std::io::stdin().is_terminal() {
                    println!("Use /resume <session-id> when input is not a terminal.");
                    return Ok(());
                }
                let mut picker = cliclack::select("Resume session · type to search")
                    .filter_mode()
                    .max_rows(10);
                for session in &sessions {
                    let name = if session.name.is_empty() {
                        "Untitled session"
                    } else {
                        &session.name
                    };
                    picker = picker.item(
                        session.id.clone(),
                        format!("{} · {}", safe_truncate(name, 60), session.id),
                        format!(
                            "{} · {} messages · {}",
                            session
                                .last_message_at
                                .unwrap_or(session.updated_at)
                                .format("%Y-%m-%d %H:%M UTC"),
                            session.message_count,
                            session.working_dir.display()
                        ),
                    );
                }
                match picker.interact() {
                    Ok(id) => id,
                    Err(_) => return Ok(()),
                }
            }
        };
        if id == self.session_id {
            println!("Already in session {id}.");
            return Ok(());
        }
        let saved = manager.get_session(&id, false).await?;
        let mut resumed = build_session(SessionBuilderConfig {
            session_id: Some(id),
            resume: true,
            interactive: true,
            debug: self.debug,
            max_turns: self.max_turns,
            container: self.agent.container().await,
            recipe: saved.recipe.clone(),
            ..Default::default()
        })
        .await;
        resumed.ensure_extensions_loaded(true).await?;
        resumed
            .agent
            .update_goose_mode(saved.goose_mode, &resumed.session_id)
            .await?;
        self.agent
            .emit_hook(HookEvent::SessionEnd, &self.session_id)
            .await;
        self.agent.discard_pending_steers(&self.session_id).await;
        *self = resumed;
        let banners = self
            .agent
            .emit_hook_with_banners(HookEvent::SessionStart, &self.session_id)
            .await;
        output::display_banner(&banners);
        println!("Resumed {} · {}", saved.name, self.session_id);
        self.render_message_history();
        Ok(())
    }

    pub(super) async fn handle_tools(&self) -> Result<()> {
        let mut tools = self.agent.list_tools(&self.session_id, None).await;
        tools.sort_by(|a, b| a.name.cmp(&b.name));
        if tools.is_empty() {
            println!("No tools are loaded. Check enabled extensions and the context profile.");
            return Ok(());
        }
        if !std::io::stdin().is_terminal() {
            for tool in tools {
                println!("{}", tool.name);
            }
            return Ok(());
        }
        let mut picker = cliclack::select("Available tools · inspect without running")
            .filter_mode()
            .max_rows(12);
        for (index, tool) in tools.iter().enumerate() {
            let description = tool.description.as_deref().unwrap_or("");
            picker = picker.item(
                index,
                tool.name.to_string(),
                safe_truncate(description.lines().next().unwrap_or(""), 100),
            );
        }
        if let Ok(index) = picker.interact() {
            let tool = &tools[index];
            println!(
                "\n{}\n{}\n\nInputs:\n{}",
                tool.name,
                tool.description.as_deref().unwrap_or(""),
                serde_json::to_string_pretty(&tool.input_schema)?
            );
        }
        Ok(())
    }

    pub(super) async fn handle_context_details(&self) -> Result<()> {
        let session = self.get_session().await?;
        let provider = self.agent.provider().await?;
        let model = self
            .agent
            .model_config_for_session(&self.session_id)
            .await?;
        let tools = self.agent.list_tools(&self.session_id, None).await;
        let profile = if PromptProfile::current().is_compact() {
            "compact"
        } else {
            "default"
        };
        println!("Session: {}\nModel: {}/{}\nMode: {:?}\nDirectory: {}\nConfigured context profile: {}\nLoaded tools: {} · /tools to inspect", self.session_id, provider.get_name(), model.model_name, self.agent.goose_mode().await, session.working_dir.display(), profile, tools.len());
        self.display_context_usage().await?;
        for (label, value) in [
            ("Latest input", session.usage.input_tokens),
            ("Latest output", session.usage.output_tokens),
            ("Cache read", session.usage.cache_read_input_tokens),
            ("Cache write", session.usage.cache_write_input_tokens),
            ("Session total", session.accumulated_usage.total_tokens),
        ] {
            println!(
                "{label}: {}",
                value
                    .map(|n| n.to_string())
                    .unwrap_or_else(|| "unreported".to_string())
            );
        }
        println!("Context is provider-reported usage, not a measurement of startup prompt size. Restart after changing the context profile.");
        Ok(())
    }
}
