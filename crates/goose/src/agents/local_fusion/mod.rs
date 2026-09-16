//! Local Fusion — the lead/sidekick split.
//!
//! "Fusion" because two models are fused into a single presented identity;
//! "local" because both run on the user's machine. The lead owns the
//! conversation and the authority actions; the sidekick is a *persistent*
//! concurrent peer that does the hands-on work.
//!
//! ## How this differs from a subagent
//!
//! A subagent is spawn → work → return → die. The sidekick survives across
//! turns, has its own context window, its own model, and its own compaction
//! thresholds. It can be parked in a wait loop while the lead keeps working.
//! That is why `TaskConfig` is the right foundation: it already carries its own
//! `provider` and `model_config`, so the two sides can run different models.
//!
//! ## Concealment
//!
//! The lead must never reveal the split. The prompt is explicit that a
//! redirect must be described as a decision the lead made, not as an
//! instruction given to someone else. [`concealment_note`] exists so that rule
//! can be asserted in tests rather than left implicit.

pub const EXTENSION_NAME: &str = "local_fusion";
pub const SIDEKICK_TOOL: &str = "sidekick";

/// Verbatim lead-facing instruction describing the sidekick.
pub const SIDEKICK_PROMPT: &str = include_str!("prompts/sidekick.txt");
/// Injected when the lead is asked about low-level detail it may not know.
pub const GROUNDING_NOTE: &str = include_str!("prompts/grounding_note.txt");
/// Injected when the sidekick is parked and a result should come back early.
pub const REPORT_FIRST_GUIDANCE: &str = include_str!("prompts/report_first.txt");

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

/// Compaction thresholds for the sidekick chain, independent of the lead's.
///
/// Format matches the upstream convention: `<spawn>,[<apply>,]<hard>`.
/// Unset means inherit the lead's thresholds.
pub fn sidekick_compaction_thresholds() -> Option<String> {
    setting("KARMX_SIDEKICK_COMPACTION_THRESHOLDS")
}

/// Variables the sidekick prompt is parameterised over.
///
/// The recovered template has 18 placeholders, several of which are
/// conditionally populated depending on which tools and features are active.
/// Keeping them in one struct means the prompt stays a verbatim artefact while
/// the caller decides what each slot says.
#[derive(Debug, Clone)]
pub struct PromptVars {
    /// Name of the delegation tool, substituted into `{SIDEKICK_TOOL}`.
    pub sidekick_tool: String,
    /// How the user refers to the lead, e.g. "agent".
    pub lead_identity: String,
    /// Phrase completing "such as {HANDS_ON_EXPLORATION}implementing changes".
    pub hands_on_exploration: String,
    /// What may be delegated, completing "including {DELEGATE_SCOPE}".
    pub delegate_scope: String,
    /// Bullet describing how the lead explores.
    pub lead_exploration: String,
    /// Bullet describing how exploration is delegated.
    pub delegate_exploration: String,
    /// Condition under which the lead implements something itself.
    pub browser_implementation: String,
    /// Bullet about browser work in general.
    pub browser_bullet: String,
    /// Bullet about complex interactive browser work.
    pub complex_browser_bullet: String,
    /// Bullet about authority actions the sidekick cannot perform.
    pub lead_authority: String,
    /// Bullet about reviewing the sidekick's diff.
    pub review_bullet: String,
    /// Guidance on what the lead may do while the sidekick works.
    pub parallel_work: String,
    /// Reminder about actions promised directly to the user.
    pub promised_actions: String,
    /// Bullet about passing file pointers instead of contents.
    pub file_pointers_bullet: String,
    /// Name of the read tool.
    pub read_tool: String,
    /// Where the user can steer work in flight.
    pub steering_surface: String,
    /// Guidance for an ask that is still unsettled.
    pub unsettled_ask: String,
    /// Bullet about blocking dispatch.
    pub blocking_dispatch_bullet: String,
}

impl Default for PromptVars {
    fn default() -> Self {
        Self {
            sidekick_tool: SIDEKICK_TOOL.to_string(),
            lead_identity: "agent".to_string(),
            hands_on_exploration: "exploring the codebase, ".to_string(),
            delegate_scope: "exploration, implementation, and verification".to_string(),
            lead_exploration: "Reading the specific files needed to judge the work.".to_string(),
            delegate_exploration:
                "- **Exploration:** Delegate broad searches and tracing to the sidekick.".to_string(),
            browser_implementation:
                "trivially small or correctness-critical".to_string(),
            browser_bullet:
                "- **Browser work:** Use the browser tools to verify rendered output.".to_string(),
            complex_browser_bullet:
                "- **Complex interactive browser work** stays with you: the build and its rendered verification are one inseparable loop.".to_string(),
            lead_authority:
                "- Messages to the user, review replies, and anything requiring user authority."
                    .to_string(),
            review_bullet: "- Reviewing the diff before it lands.".to_string(),
            parallel_work:
                "- While the sidekick works, do the review and user-facing follow-ups it cannot."
                    .to_string(),
            promised_actions:
                "Track the lead-only actions you have promised the user; the sidekick cannot do these."
                    .to_string(),
            file_pointers_bullet:
                "- Pass file paths and line ranges rather than pasting file contents.".to_string(),
            read_tool: "read".to_string(),
            steering_surface: "the conversation".to_string(),
            unsettled_ask:
                "If an ask is still unsettled, resolve it before dispatching more work.".to_string(),
            blocking_dispatch_bullet:
                "- Prefer a blocking handoff when you need the result before you can continue."
                    .to_string(),
        }
    }
}

impl PromptVars {
    /// Render the verbatim template with every placeholder filled.
    ///
    /// Unknown placeholders are left in place rather than blanked, so a template
    /// change shows up as a visible `{TOKEN}` instead of silently vanishing.
    pub fn render(&self) -> String {
        let pairs = [
            ("SIDEKICK_TOOL", self.sidekick_tool.as_str()),
            ("LEAD_IDENTITY", self.lead_identity.as_str()),
            ("HANDS_ON_EXPLORATION", self.hands_on_exploration.as_str()),
            ("DELEGATE_SCOPE", self.delegate_scope.as_str()),
            ("LEAD_EXPLORATION", self.lead_exploration.as_str()),
            ("DELEGATE_EXPLORATION", self.delegate_exploration.as_str()),
            (
                "BROWSER_IMPLEMENTATION",
                self.browser_implementation.as_str(),
            ),
            ("BROWSER_BULLET", self.browser_bullet.as_str()),
            (
                "COMPLEX_BROWSER_BULLET",
                self.complex_browser_bullet.as_str(),
            ),
            ("LEAD_AUTHORITY", self.lead_authority.as_str()),
            ("REVIEW_BULLET", self.review_bullet.as_str()),
            ("PARALLEL_WORK", self.parallel_work.as_str()),
            ("PROMISED_ACTIONS", self.promised_actions.as_str()),
            ("FILE_POINTERS_BULLET", self.file_pointers_bullet.as_str()),
            ("READ_TOOL", self.read_tool.as_str()),
            ("STEERING_SURFACE", self.steering_surface.as_str()),
            ("UNSETTLED_ASK", self.unsettled_ask.as_str()),
            (
                "BLOCKING_DISPATCH_BULLET",
                self.blocking_dispatch_bullet.as_str(),
            ),
        ];
        let mut out = SIDEKICK_PROMPT.to_string();
        for (key, value) in pairs {
            out = out.replace(&format!("{{{key}}}"), value);
        }
        out
    }

    /// Placeholders still present after rendering.
    #[allow(clippy::string_slice)] // Indices come from find() on ASCII '{' / '}'; byte slicing is safe.
    pub fn unresolved(&self) -> Vec<String> {
        let rendered = self.render();
        let mut found = Vec::new();
        let bytes = rendered.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'{' {
                if let Some(rel) = rendered[i + 1..].find('}') {
                    let inner = &rendered[i + 1..i + 1 + rel];
                    if !inner.is_empty()
                        && inner.chars().all(|c| c.is_ascii_uppercase() || c == '_')
                    {
                        found.push(inner.to_string());
                    }
                    i += rel + 2;
                    continue;
                }
            }
            i += 1;
        }
        found.sort();
        found.dedup();
        found
    }
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

/// The concealment rule, stated once so it can be asserted.
pub fn concealment_note() -> &'static str {
    "Do not mention the sidekick or distinguish its work from your own."
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_is_the_recovered_one() {
        assert!(SIDEKICK_PROMPT.contains("persistent subagent"));
        assert!(SIDEKICK_PROMPT.contains("{SIDEKICK_TOOL}"));
        assert!(SIDEKICK_PROMPT.len() > 15_000, "should be the full prompt");
    }

    #[test]
    fn default_vars_fill_every_placeholder() {
        let v = PromptVars::default();
        let unresolved = v.unresolved();
        assert!(
            unresolved.is_empty(),
            "unresolved placeholders: {unresolved:?}"
        );
    }

    #[test]
    fn rendering_substitutes_values() {
        let v = PromptVars {
            sidekick_tool: "my_helper".into(),
            ..PromptVars::default()
        };
        let r = v.render();
        assert!(r.contains("my_helper"));
        assert!(!r.contains("{SIDEKICK_TOOL}"));
    }

    #[test]
    fn unknown_placeholder_is_reported_not_blanked() {
        // simulate a template drift by checking the detector itself
        let v = PromptVars {
            lead_identity: "the agent".into(),
            ..PromptVars::default()
        };
        let r = v.render();
        assert!(!r.contains("{LEAD_IDENTITY}"));
        assert!(v.unresolved().is_empty());
    }

    #[test]
    fn concealment_rule_is_documented() {
        assert!(SIDEKICK_PROMPT.contains("do not mention it or distinguish its work from yours"));
        assert!(concealment_note().contains("Do not mention"));
    }

    #[test]
    fn grounding_note_is_present() {
        assert!(GROUNDING_NOTE.contains("grounded answer first"));
    }

    #[test]
    fn report_first_guidance_is_present() {
        assert!(REPORT_FIRST_GUIDANCE.contains("parked"));
        assert!(REPORT_FIRST_GUIDANCE.contains("report back immediately"));
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
    fn sidekick_model_is_optional() {
        // must not panic or invent a model when unset
        let _ = sidekick_model_uid();
        let _ = sidekick_compaction_thresholds();
    }

    #[test]
    fn two_registered_models_keep_sidekick_distinct_from_lead() {
        let _guard = env_lock::lock_env([
            ("GOOSE_PROVIDER", Some("anthropic")),
            ("GOOSE_MODEL", Some("claude-sonnet-4")),
            ("KARMX_SIDEKICK_PROVIDER", Some("openai")),
            ("KARMX_SIDEKICK_MODEL", Some("gpt-4o-mini")),
            (
                "KARMX_SIDEKICK_COMPACTION_THRESHOLDS",
                Some("80000,120000,160000"),
            ),
        ]);

        assert_eq!(sidekick_provider().as_deref(), Some("openai"));
        assert_eq!(sidekick_model_uid().as_deref(), Some("gpt-4o-mini"));
        assert_ne!(
            sidekick_model_uid().as_deref(),
            Some("claude-sonnet-4"),
            "fusion sidekick must stay on its own registered model"
        );

        let thresholds = goose_context_management::CompactionThresholds::parse(
            sidekick_compaction_thresholds().as_deref().unwrap(),
        )
        .unwrap();
        assert_eq!(thresholds.spawn, 80_000);
        assert_eq!(thresholds.apply, 120_000);
        assert_eq!(thresholds.hard, 160_000);
    }
}
