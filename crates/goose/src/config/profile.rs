//! Prompt and tool profiles.
//!
//! `KARMX_PROMPT_PROFILE=compact` (env var or `config.yaml` key) switches karmX to a
//! profile made for small or heavily quantized local models: the compact system
//! prompt (`system.compact.md`), a short default extension allowlist, and a capped
//! skills listing. Each piece can be overridden on its own:
//!
//! - `KARMX_EXTENSIONS_ALLOWLIST`: comma-separated extension keys (the `extensions:`
//!   map keys of config.yaml; matching ignores case, `_`, `-` and spaces) that may run
//!   in a session. It is an intersection with what is enabled in config, never an
//!   enabler. Unset means "everything enabled in config" (default profile) or the
//!   built-in compact list (compact profile); a blank value counts as unset.
//! - `KARMX_SKILLS_PROMPT_LIMIT`: maximum number of skills listed in the system
//!   prompt. `0` means unlimited (default profile); the compact profile uses 24, which
//!   only matters when the `skills` extension is on the allowlist (it is not on the
//!   built-in compact list, so plain compact sessions list no skills at all).
//!
//! Measured on a real config with 228 installed skills and 20 enabled extensions, the
//! default profile sends about 22K tokens of system prompt and tool schemas before the
//! first user word; the compact profile sends about 3K.

use crate::config::{Config, ConfigError};
use std::sync::Once;

pub const PROMPT_PROFILE_KEY: &str = "KARMX_PROMPT_PROFILE";
pub const EXTENSIONS_ALLOWLIST_KEY: &str = "KARMX_EXTENSIONS_ALLOWLIST";
pub const SKILLS_PROMPT_LIMIT_KEY: &str = "KARMX_SKILLS_PROMPT_LIMIT";

/// Extensions the compact profile runs when no allowlist is configured.
pub const COMPACT_EXTENSIONS: &[&str] = &[
    "developer",
    "context_engine",
    "todo",
    "browser_preview",
    "analyze",
    "summon",
];

/// Skills listed in the system prompt under the compact profile.
pub const COMPACT_SKILLS_LIMIT: usize = 24;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PromptProfile {
    #[default]
    Default,
    Compact,
}

impl PromptProfile {
    /// Parses a profile name. Empty and `default` map to the default profile,
    /// `compact` and `small` to the compact one; anything else is `None`.
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "" | "default" | "full" => Some(Self::Default),
            "compact" | "small" => Some(Self::Compact),
            _ => None,
        }
    }

    /// The active profile: explicit `KARMX_PROMPT_PROFILE` first, then the
    /// active provider's `prompt_profile`, then the default profile.
    pub fn current() -> Self {
        Self::from_config(Config::global())
    }

    pub fn from_config(config: &Config) -> Self {
        static WARNED: Once = Once::new();
        match config.get_param::<String>(PROMPT_PROFILE_KEY) {
            Ok(value) => Self::parse(&value).unwrap_or_else(|| {
                WARNED.call_once(|| {
                    tracing::warn!(
                        "{PROMPT_PROFILE_KEY}={value:?} is not a known profile; using the default"
                    );
                });
                Self::Default
            }),
            Err(_) => Self::from_active_provider(config).unwrap_or_default(),
        }
    }

    /// The profile the active provider declares in its declarative config,
    /// e.g. `"prompt_profile": "compact"` on a custom local provider.
    fn from_active_provider(config: &Config) -> Option<Self> {
        let provider = config.get_param::<String>("GOOSE_PROVIDER").ok()?;
        let loaded = crate::config::declarative_providers::load_provider(&provider).ok()?;
        Self::parse(loaded.config.prompt_profile.as_deref()?)
    }

    pub fn is_compact(self) -> bool {
        self == Self::Compact
    }

    /// Template name to render for `name` under this profile, given whether a user
    /// override exists for a name and whether a builtin template exists for a name.
    /// A user override of the base name always wins; otherwise the compact profile
    /// prefers `<stem>.compact.<ext>` when a user or builtin version of it exists.
    pub fn resolve_template_name(
        self,
        name: &str,
        user_override_exists: impl Fn(&str) -> bool,
        builtin_exists: impl Fn(&str) -> bool,
    ) -> String {
        if !self.is_compact() || user_override_exists(name) {
            return name.to_string();
        }
        let Some(candidate) = compact_template_name(name) else {
            return name.to_string();
        };
        if user_override_exists(&candidate) || builtin_exists(&candidate) {
            candidate
        } else {
            name.to_string()
        }
    }
}

/// `system.md` -> `system.compact.md`; names without an extension get `.compact` appended.
pub fn compact_template_name(name: &str) -> Option<String> {
    if name.contains(".compact.") || name.ends_with(".compact") {
        return None;
    }
    match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => Some(format!("{stem}.compact.{ext}")),
        _ => Some(format!("{name}.compact")),
    }
}

/// Normalizes an extension key or display name for allowlist matching: ASCII lower
/// case with `_`, `-` and whitespace removed, so `context_engine`, `Context Engine`
/// and `context-engine` all compare equal.
pub fn loose_key(value: &str) -> String {
    value
        .chars()
        .filter(|c| !(c.is_whitespace() || *c == '_' || *c == '-'))
        .flat_map(|c| c.to_lowercase())
        .collect()
}

/// Splits a comma-separated allowlist into loosely normalized extension keys.
pub fn parse_allowlist(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(loose_key)
        .filter(|item| !item.is_empty())
        .collect()
}

/// The extension allowlist in effect: the configured one, else the compact default
/// under the compact profile, else `None` (no filtering).
pub fn extensions_allowlist() -> Option<Vec<String>> {
    extensions_allowlist_with_config(Config::global())
}

pub fn extensions_allowlist_with_config(config: &Config) -> Option<Vec<String>> {
    let configured = match config.get_param::<String>(EXTENSIONS_ALLOWLIST_KEY) {
        Ok(raw) => Some(raw),
        Err(ConfigError::NotFound(_)) => None,
        Err(err) => {
            tracing::warn!("{EXTENSIONS_ALLOWLIST_KEY} could not be read ({err}); ignoring it");
            None
        }
    };
    extensions_allowlist_for(PromptProfile::from_config(config), configured)
}

pub fn extensions_allowlist_for(
    profile: PromptProfile,
    configured: Option<String>,
) -> Option<Vec<String>> {
    let configured = configured.map(|raw| parse_allowlist(&raw));
    match configured {
        Some(list) if !list.is_empty() => Some(list),
        Some(_) => {
            tracing::warn!(
                "{EXTENSIONS_ALLOWLIST_KEY} is set but names no extension; treating it as unset"
            );
            default_allowlist(profile)
        }
        None => default_allowlist(profile),
    }
}

fn default_allowlist(profile: PromptProfile) -> Option<Vec<String>> {
    if profile.is_compact() {
        Some(
            COMPACT_EXTENSIONS
                .iter()
                .map(|name| loose_key(name))
                .collect(),
        )
    } else {
        None
    }
}

/// Whether an extension with this config key and display name may run under `allowlist`.
/// `None` permits everything.
pub fn allowlist_permits(allowlist: &Option<Vec<String>>, key: &str, name: &str) -> bool {
    let Some(allowed) = allowlist else {
        return true;
    };
    let key = loose_key(key);
    let name = loose_key(name);
    allowed.iter().any(|item| *item == key || *item == name)
}

/// Maximum number of skills to list in the system prompt; `0` means unlimited.
pub fn skills_prompt_limit() -> usize {
    let config = Config::global();
    let configured = match config.get_param::<usize>(SKILLS_PROMPT_LIMIT_KEY) {
        Ok(limit) => Some(limit),
        Err(ConfigError::NotFound(_)) => None,
        Err(err) => {
            tracing::warn!(
                "{SKILLS_PROMPT_LIMIT_KEY} is not a non-negative integer ({err}); ignoring it"
            );
            None
        }
    };
    skills_prompt_limit_for(PromptProfile::from_config(config), configured)
}

pub fn skills_prompt_limit_for(profile: PromptProfile, configured: Option<usize>) -> usize {
    match configured {
        Some(limit) => limit,
        None if profile.is_compact() => COMPACT_SKILLS_LIMIT,
        None => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list_for_compact() -> Option<Vec<String>> {
        extensions_allowlist_for(PromptProfile::Compact, None)
    }

    #[test]
    fn parses_profile_names() {
        assert_eq!(PromptProfile::parse(""), Some(PromptProfile::Default));
        assert_eq!(
            PromptProfile::parse("Default"),
            Some(PromptProfile::Default)
        );
        assert_eq!(
            PromptProfile::parse(" compact "),
            Some(PromptProfile::Compact)
        );
        assert_eq!(PromptProfile::parse("small"), Some(PromptProfile::Compact));
        assert_eq!(PromptProfile::parse("huge"), None);
    }

    #[test]
    fn compact_names() {
        assert_eq!(
            compact_template_name("system.md").as_deref(),
            Some("system.compact.md")
        );
        assert_eq!(
            compact_template_name("noext").as_deref(),
            Some("noext.compact")
        );
        assert_eq!(compact_template_name("system.compact.md"), None);
    }

    #[test]
    fn default_profile_never_swaps_templates() {
        let name = PromptProfile::Default.resolve_template_name("system.md", |_| false, |_| true);
        assert_eq!(name, "system.md");
    }

    #[test]
    fn compact_profile_prefers_compact_builtin() {
        let name = PromptProfile::Compact.resolve_template_name(
            "system.md",
            |_| false,
            |n| n == "system.compact.md",
        );
        assert_eq!(name, "system.compact.md");
    }

    #[test]
    fn compact_profile_falls_back_when_no_compact_template() {
        let name =
            PromptProfile::Compact.resolve_template_name("plan.md", |_| false, |n| n == "plan.md");
        assert_eq!(name, "plan.md");
    }

    #[test]
    fn user_override_of_base_name_wins_over_profile() {
        let name = PromptProfile::Compact.resolve_template_name(
            "system.md",
            |n| n == "system.md",
            |_| true,
        );
        assert_eq!(name, "system.md");
    }

    #[test]
    fn user_override_of_compact_name_is_used() {
        let name = PromptProfile::Compact.resolve_template_name(
            "system.md",
            |n| n == "system.compact.md",
            |_| false,
        );
        assert_eq!(name, "system.compact.md");
    }

    #[test]
    fn allowlist_parsing_normalizes_names() {
        assert_eq!(
            parse_allowlist(" Developer, context engine ,,todo, browser-preview "),
            vec!["developer", "contextengine", "todo", "browserpreview"]
        );
    }

    #[test]
    fn allowlist_matches_key_or_name_loosely() {
        let list = Some(parse_allowlist("developer, Context Engine"));
        assert!(allowlist_permits(&list, "developer", "Developer"));
        assert!(allowlist_permits(&list, "context_engine", "Context Engine"));
        assert!(allowlist_permits(&list, "context-engine", ""));
        assert!(!allowlist_permits(
            &list,
            "computer_control",
            "Computer Control"
        ));
        assert!(allowlist_permits(&None, "anything", "Anything"));
    }

    #[test]
    fn blank_allowlist_counts_as_unset() {
        assert_eq!(
            extensions_allowlist_for(PromptProfile::Default, Some("  ".into())),
            None
        );
        assert_eq!(
            extensions_allowlist_for(PromptProfile::Default, Some(",,".into())),
            None
        );
        let compact = extensions_allowlist_for(PromptProfile::Compact, Some("".into())).unwrap();
        assert!(compact.iter().any(|item| item == "developer"));
    }

    #[test]
    fn compact_profile_supplies_default_allowlist_and_skill_limit() {
        let list = extensions_allowlist_for(PromptProfile::Compact, None).unwrap();
        assert!(list.iter().any(|item| item == "developer"));
        assert!(!list.iter().any(|item| item == "skills"));
        assert_eq!(extensions_allowlist_for(PromptProfile::Default, None), None);
        assert_eq!(
            extensions_allowlist_for(PromptProfile::Default, Some("todo, developer".into())),
            Some(vec!["todo".to_string(), "developer".to_string()])
        );
        assert!(allowlist_permits(
            &list_for_compact(),
            "context_engine",
            "Context Engine"
        ));
        assert_eq!(skills_prompt_limit_for(PromptProfile::Default, None), 0);
        assert_eq!(
            skills_prompt_limit_for(PromptProfile::Compact, None),
            COMPACT_SKILLS_LIMIT
        );
        assert_eq!(skills_prompt_limit_for(PromptProfile::Compact, Some(3)), 3);
    }

    #[test]
    fn compact_allowlist_includes_summon() {
        let list = extensions_allowlist_for(PromptProfile::Compact, None).unwrap();
        assert!(list.iter().any(|item| item == "summon"));
    }

    #[test]
    fn provider_prompt_profile_is_used_when_no_explicit_profile() {
        let temp_dir = tempfile::tempdir().unwrap();
        let temp_root = temp_dir.path().display().to_string();
        let custom = temp_dir.path().join("config").join("custom_providers");
        std::fs::create_dir_all(&custom).unwrap();
        std::fs::write(
            custom.join("compact_prov.json"),
            r#"{
                "name": "compact_prov",
                "engine": "openai",
                "display_name": "Compact",
                "base_url": "http://localhost:1",
                "models": [],
                "prompt_profile": "compact"
            }"#,
        )
        .unwrap();
        let _guard = env_lock::lock_env([
            ("GOOSE_PATH_ROOT", Some(temp_root.as_str())),
            ("GOOSE_PROVIDER", Some("compact_prov")),
            ("KARMX_PROMPT_PROFILE", None::<&str>),
        ]);

        assert_eq!(PromptProfile::current(), PromptProfile::Compact);
    }

    #[test]
    fn explicit_profile_beats_provider_prompt_profile() {
        let temp_dir = tempfile::tempdir().unwrap();
        let temp_root = temp_dir.path().display().to_string();
        let custom = temp_dir.path().join("config").join("custom_providers");
        std::fs::create_dir_all(&custom).unwrap();
        std::fs::write(
            custom.join("compact_prov.json"),
            r#"{
                "name": "compact_prov",
                "engine": "openai",
                "display_name": "Compact",
                "base_url": "http://localhost:1",
                "models": [],
                "prompt_profile": "compact"
            }"#,
        )
        .unwrap();
        let _guard = env_lock::lock_env([
            ("GOOSE_PATH_ROOT", Some(temp_root.as_str())),
            ("GOOSE_PROVIDER", Some("compact_prov")),
            ("KARMX_PROMPT_PROFILE", Some("default")),
        ]);

        assert_eq!(PromptProfile::current(), PromptProfile::Default);
    }
}
