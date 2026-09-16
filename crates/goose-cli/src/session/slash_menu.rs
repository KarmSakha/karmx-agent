use super::input::{handle_slash_command, InputResult, ModelCommandOptions};
use super::CompletionCache;
use goose::slash_commands::slash_command::list_acp_commands;
use goose::slash_commands::types::SlashCommandSource;
use std::sync::{Arc, RwLock};
use strum::VariantNames;

/// What happens after a command is picked from the menu.
enum Flow {
    /// Dispatch the bare command, e.g. `/new`.
    Run,
    /// Ask for free-text arguments first (may be optional).
    FreeArgs {
        hint: String,
        required: bool,
    },
    PickTheme,
    PickMode,
    PickModel,
    PickPrompt,
    PickSkills,
    /// Open the sidekick provider/model configure dialog.
    SidekickDialog,
    /// Fusion pairing: pick the sidekick model, then the lead model.
    PickFusion,
}

struct MenuEntry {
    name: String,
    description: String,
    flow: Flow,
}

/// Whether a submitted line should open the slash menu instead of being
/// dispatched or sent to the agent as a message.
///
/// Only single-token inputs starting with `/` qualify, and only when they
/// contain no further `/` (so `/tmp/file` stays a plain message).
pub fn should_open(input: &str) -> bool {
    let trimmed = input.trim();
    trimmed.starts_with('/')
        && !trimmed.contains(char::is_whitespace)
        && !trimmed.strip_prefix('/').unwrap_or(trimmed).contains('/')
}

/// Open the command picker. `input` is the submitted line (`/`, `/mode`, or an
/// unrecognized `/word`). Returns `None` when the user cancels.
pub fn run(input: &str, cache: &Arc<RwLock<CompletionCache>>) -> Option<InputResult> {
    let entries = menu_entries();
    let typed = input.trim().trim_start_matches('/');

    let name = if entries.iter().any(|e| e.name == typed) {
        typed.to_string()
    } else {
        pick_command(&entries, typed)?
    };

    let entry = entries.iter().find(|e| e.name == name)?;
    run_entry(entry, cache)
}

/// Interactive `/model` flow: pick a model from the overlay (all providers,
/// thinking effort, sidekick). Returns `None` when there is nothing to pick
/// from, the terminal is not interactive, or the user cancels.
pub fn pick_model(cache: &Arc<RwLock<CompletionCache>>) -> Option<InputResult> {
    super::model_picker::run(cache)
}

/// Interactive `/model --provider` flow: pick a provider, then its model.
pub fn pick_provider(cache: &Arc<RwLock<CompletionCache>>) -> Option<InputResult> {
    let provider_names = cache.read().unwrap().provider_names.clone();
    if provider_names.is_empty() {
        return None;
    }

    let mut select = cliclack::select("Provider").filter_mode();
    for name in &provider_names {
        select = select.item(name.clone(), name.clone(), "");
    }
    let provider = select.interact().ok()?;

    let models = {
        cache
            .read()
            .unwrap()
            .provider_models
            .get(&provider)
            .cloned()
            .unwrap_or_default()
    };
    if models.is_empty() {
        return Some(InputResult::Model(ModelCommandOptions {
            provider: Some(provider),
            model: None,
            thinking: None,
        }));
    }

    let mut select = cliclack::select(format!("Model (provider: {provider})")).filter_mode();
    for model in &models {
        select = select.item(model.clone(), model.clone(), "");
    }
    Some(InputResult::Model(ModelCommandOptions {
        provider: Some(provider),
        model: select.interact().ok(),
        thinking: None,
    }))
}

fn menu_entries() -> Vec<MenuEntry> {
    let mut entries = vec![
        entry("exit", "Exit the session", Flow::Run),
        entry("help", "Show available commands", Flow::Run),
        entry(
            "new",
            "Start a fresh session (keeps provider, model, extensions)",
            Flow::Run,
        ),
        entry("clear", "Clear the chat history", Flow::Run),
        entry(
            "compact",
            "Compact the conversation to save context",
            Flow::Run,
        ),
        entry("model", "Open the model picker", Flow::PickModel),
        entry("mode", "Set the karmx mode", Flow::PickMode),
        entry(
            "t",
            "Toggle or set the theme (light, dark, ansi)",
            Flow::PickTheme,
        ),
        entry("r", "Toggle full tool output", Flow::Run),
        entry(
            "edit",
            "Compose a message in your editor",
            Flow::FreeArgs {
                hint: "optional prefill text".to_string(),
                required: false,
            },
        ),
        entry(
            "plan",
            "Enter plan mode",
            Flow::FreeArgs {
                hint: "optional message".to_string(),
                required: false,
            },
        ),
        entry("endplan", "Exit plan mode", Flow::Run),
        entry(
            "skills",
            "List skills, or load specific ones",
            Flow::PickSkills,
        ),
        entry(
            "prompts",
            "List available prompts",
            Flow::FreeArgs {
                hint: "--extension <name> (optional)".to_string(),
                required: false,
            },
        ),
        entry("prompt", "Execute a prompt by name", Flow::PickPrompt),
        entry(
            "sidekick",
            "Pick the sidekick (fusion) provider + model",
            Flow::SidekickDialog,
        ),
        entry(
            "fusion",
            "Pick a sidekick model, then the lead model",
            Flow::PickFusion,
        ),
        entry(
            "builtin",
            "Enable builtin extensions by name",
            Flow::FreeArgs {
                hint: "names, comma-separated".to_string(),
                required: true,
            },
        ),
        entry(
            "extension",
            "Add a stdio extension",
            Flow::FreeArgs {
                hint: "ENV=val command args...".to_string(),
                required: true,
            },
        ),
    ];

    let cwd = std::env::current_dir().unwrap_or_default();
    for command in list_acp_commands(Some(&cwd)) {
        if entries.iter().any(|e| e.name == command.name) {
            continue;
        }
        let flow = match command.source {
            SlashCommandSource::Builtin => match command.name.as_str() {
                "goal" | "grind" => Flow::FreeArgs {
                    hint: "description (or 'off' to clear)".to_string(),
                    required: false,
                },
                _ => Flow::Run,
            },
            _ => Flow::FreeArgs {
                hint: command
                    .input_hint
                    .clone()
                    .unwrap_or_else(|| "arguments (optional)".to_string()),
                required: false,
            },
        };
        entries.push(MenuEntry {
            name: command.name,
            description: command.description,
            flow,
        });
    }

    entries
}

fn entry(name: &str, description: &str, flow: Flow) -> MenuEntry {
    MenuEntry {
        name: name.to_string(),
        description: description.to_string(),
        flow,
    }
}

fn pick_command(entries: &[MenuEntry], typed: &str) -> Option<String> {
    let mut select = cliclack::select("karmx commands")
        .filter_mode()
        .max_rows(12);

    // Prefix matches float to the top so `/mo` + Enter lands on /model.
    let mut sorted: Vec<&MenuEntry> = entries.iter().collect();
    sorted.sort_by_key(|e| usize::from(!e.name.starts_with(typed)));

    for e in sorted {
        select = select.item(
            e.name.clone(),
            format!("/{}", e.name),
            e.description.as_str(),
        );
    }

    select.interact().ok()
}

fn run_entry(entry: &MenuEntry, cache: &Arc<RwLock<CompletionCache>>) -> Option<InputResult> {
    match &entry.flow {
        Flow::Run => Some(dispatch(&format!("/{}", entry.name))),
        Flow::FreeArgs { hint, required } => {
            let args = ask_args(&entry.name, hint, *required)?;
            Some(dispatch(&compose(&entry.name, &args)))
        }
        Flow::PickTheme => pick_from("/t", &["light", "dark", "ansi"], "Theme"),
        Flow::PickMode => pick_mode(),
        Flow::PickModel => pick_model(cache),
        Flow::PickPrompt => pick_prompt(cache),
        Flow::PickSkills => pick_skills(),
        Flow::SidekickDialog => Some(dispatch("/sidekick")),
        // The model picker overlay already includes sidekick (Tab) editing —
        // /fusion opens it directly.
        Flow::PickFusion => pick_model(cache),
    }
}

fn pick_mode() -> Option<InputResult> {
    let modes = goose::config::GooseMode::VARIANTS;
    pick_from("/mode", modes, "karmx mode")
}

fn pick_from(command: &str, options: &[&str], title: &str) -> Option<InputResult> {
    let mut select = cliclack::select(title).filter_mode();
    for option in options {
        select = select.item(*option, *option, "");
    }
    let chosen = select.interact().ok()?;
    Some(dispatch(&format!("{command} {chosen}")))
}

fn pick_prompt(cache: &Arc<RwLock<CompletionCache>>) -> Option<InputResult> {
    let names: Vec<String> = {
        let cache = cache.read().unwrap();
        let mut names: Vec<String> = cache.prompts.values().flatten().cloned().collect();
        names.sort();
        names.dedup();
        names
    };

    let name = if names.is_empty() {
        ask_args("prompt", "prompt name", true)?
    } else {
        let mut select = cliclack::select("Prompt").filter_mode();
        for name in &names {
            select = select.item(name.clone(), name.clone(), "");
        }
        select.interact().ok()?
    };

    let args = ask_args("prompt", "key=value arguments (optional)", false).unwrap_or_default();
    Some(dispatch(&compose(&format!("prompt {name}"), &args)))
}

fn pick_skills() -> Option<InputResult> {
    let cwd = std::env::current_dir().unwrap_or_default();
    let mut names: Vec<String> = goose::skills::list_installed_skills(Some(&cwd))
        .iter()
        .map(|s| s.name.clone())
        .collect();
    names.sort();

    if names.is_empty() {
        return Some(dispatch("/skills"));
    }

    let mut select = cliclack::multiselect("Load skills (none = list all)")
        .filter_mode()
        .required(false);
    for name in &names {
        select = select.item(name.clone(), name.clone(), "");
    }
    match select.interact() {
        Ok(chosen) if chosen.is_empty() => Some(dispatch("/skills")),
        Ok(chosen) => Some(dispatch(&format!("/skills {}", chosen.join(" ")))),
        Err(_) => None,
    }
}

fn ask_args(command: &str, hint: &str, required: bool) -> Option<String> {
    cliclack::input(format!("/{command} arguments"))
        .placeholder(hint)
        .required(required)
        .interact()
        .ok()
        .map(|s: String| s.trim().to_string())
}

fn compose(name: &str, args: &str) -> String {
    if args.is_empty() {
        format!("/{name}")
    } else {
        format!("/{name} {args}")
    }
}

/// Re-dispatch a composed command line through the normal slash handler;
/// commands handled agent-side (goal, grind, skills, recipes) fall back to a
/// regular message so the agent resolves them.
fn dispatch(line: &str) -> InputResult {
    handle_slash_command(line).unwrap_or_else(|| InputResult::Message(line.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bare_slash_opens_menu() {
        assert!(should_open("/"));
    }

    #[test]
    fn single_token_prefix_opens_menu() {
        assert!(should_open("/modl"));
        assert!(should_open("/mode"));
    }

    #[test]
    fn paths_and_args_do_not_open_menu() {
        assert!(!should_open("/tmp/file"));
        assert!(!should_open("/model coder"));
        assert!(!should_open("plain message"));
    }

    #[test]
    fn compose_adds_args() {
        assert_eq!(compose("mode", "auto"), "/mode auto");
        assert_eq!(compose("new", ""), "/new");
    }
}
