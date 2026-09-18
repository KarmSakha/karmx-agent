//! Local Fusion — the lead/sidekick split.
//!
//! "Fusion" because two models are fused into a single presented identity;
//! "local" because both run on the user's machine. The lead owns the
//! conversation and the authority actions; the sidekick is a *persistent*
//! concurrent peer that does the hands-on work.
//!
//! A subagent is spawn → work → return → die. The sidekick survives across
//! turns, has its own context window and its own model, and can be parked while
//! the lead keeps working. The `sidekick` tool lives in the summon platform
//! extension; the lead-facing guidance is the `enable_subagents` block of
//! `prompts/system.md`, rendered only when this module says the tool is offered.

use crate::config::GooseMode;
use rmcp::model::Tool;

pub const SIDEKICK_TOOL: &str = "sidekick";

/// A sidekick setting, resolved from env first, then `~/.config/karmx/config.yaml`.
fn setting(key: &str) -> Option<String> {
    crate::config::Config::global()
        .get_param::<String>(key)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Provider for the sidekick (`KARMX_SIDEKICK_PROVIDER`, env or config.yaml);
/// unset = the lead's provider.
pub fn sidekick_provider() -> Option<String> {
    setting("KARMX_SIDEKICK_PROVIDER")
}

/// Model for the sidekick (`KARMX_SIDEKICK_MODEL`, env or config.yaml); unset =
/// the lead's model. Different models are the point of fusion: an expensive
/// lead for judgement, a cheaper sidekick for mechanical work.
pub fn sidekick_model_uid() -> Option<String> {
    setting("KARMX_SIDEKICK_MODEL")
}

/// Whether a dedicated sidekick provider or model is configured. The compact
/// profile only offers the `sidekick` tool when this is true — without one it
/// would just be a second copy of the lead.
pub fn sidekick_configured() -> bool {
    sidekick_provider().is_some() || sidekick_model_uid().is_some()
}

/// Whether the lead's system prompt should carry the sidekick guidance: exactly
/// when the `sidekick` tool is in this request and tools can actually run.
pub fn sidekick_offered(tools: &[Tool], goose_mode: GooseMode) -> bool {
    goose_mode != GooseMode::Chat && tools.iter().any(|tool| tool.name == SIDEKICK_TOOL)
}

/// Frame a lead's brief for dispatch to the sidekick.
///
/// The sidekick is an independent peer: it does not share the lead's
/// conversation, so it needs the report contract stated up front rather than
/// discovered. Kept separate from the message so the lead's own words stay
/// intact and this framing is testable.
pub fn sidekick_brief(message: &str) -> String {
    format!(
        "{}\n\n---\nReport back concisely:\n\
         - what you did or found\n\
         - the evidence: diff summary, test output, file paths\n\
         - anything you could not do, and what you need from me\n\
         Do not restate context I already gave you. Do not ask me to confirm a\
         decision that the brief already settled.",
        message.trim()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmcp::object;

    fn tool(name: &'static str) -> Tool {
        Tool::new(name, "", object!({"type": "object"}))
    }

    #[test]
    fn sidekick_guidance_follows_the_tool_list() {
        let with = [tool("shell"), tool(SIDEKICK_TOOL)];
        let without = [tool("shell"), tool("delegate")];

        assert!(sidekick_offered(&with, GooseMode::Auto));
        assert!(!sidekick_offered(&without, GooseMode::Auto));
        assert!(!sidekick_offered(&with, GooseMode::Chat));
    }

    #[test]
    fn brief_preserves_the_message_and_adds_a_report_contract() {
        let b = sidekick_brief("  implement the parser  ");
        assert!(b.starts_with("implement the parser"), "message must lead");
        assert!(b.contains("evidence"));
        assert!(b.contains("could not do"));
    }

    #[test]
    fn brief_does_not_duplicate_on_trim() {
        let b = sidekick_brief("x");
        assert_eq!(b.matches("x\n\n---").count(), 1);
        assert!(!b.starts_with(' '), "leading whitespace should be trimmed");
    }

    #[test]
    fn two_registered_models_keep_sidekick_distinct_from_lead() {
        let _guard = env_lock::lock_env([
            ("GOOSE_PROVIDER", Some("anthropic")),
            ("GOOSE_MODEL", Some("claude-sonnet-4")),
            ("KARMX_SIDEKICK_PROVIDER", Some("openai")),
            ("KARMX_SIDEKICK_MODEL", Some("gpt-4o-mini")),
        ]);

        assert_eq!(sidekick_provider().as_deref(), Some("openai"));
        assert_eq!(sidekick_model_uid().as_deref(), Some("gpt-4o-mini"));
    }
}
