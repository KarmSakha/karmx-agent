use crate::config::{Config, ConfigError};
use crate::conversation::message::Message;
use crate::providers::base::Provider;
use anyhow::{anyhow, Result};
use goose_providers::conversation::token_usage::ProviderUsage;
use goose_providers::errors::ProviderError;
use goose_providers::model::ModelConfig;
use goose_providers::thinking::ThinkingEffort;
use rmcp::model::Tool;
use serde_json::Value;
use std::collections::HashMap;

pub fn model_config_from_user_config(
    provider_name: &str,
    model_name: impl AsRef<str>,
) -> Result<ModelConfig> {
    let model = base_model_config_from_user_config(provider_name, model_name.as_ref())?;
    materialize_model_config(provider_name, model)
}

pub fn model_config_from_user_config_with_session_settings(
    provider_name: &str,
    model_name: impl AsRef<str>,
    previous: Option<&ModelConfig>,
    request_params: Option<HashMap<String, Value>>,
    _context_limit: Option<usize>,
) -> Result<ModelConfig> {
    let config = Config::global();
    let model = base_model_config_from_user_config(provider_name, model_name.as_ref())?;
    let model = materialize_model_config_inner(model, provider_name, false)?
        .with_inherited_session_settings_from(previous, request_params)
        .with_default_thinking_effort(config.get_goose_thinking_effort());

    Ok(apply_canonical_limits(provider_name, model))
}

pub fn materialize_model_config(provider_name: &str, model: ModelConfig) -> Result<ModelConfig> {
    let model = materialize_model_config_inner(model, provider_name, true)?;
    Ok(apply_canonical_limits(provider_name, model))
}

fn apply_canonical_limits(provider_name: &str, model: ModelConfig) -> ModelConfig {
    if provider_name == goose_providers::azure_foundry::AZURE_FOUNDRY_PROVIDER_NAME {
        model
    } else {
        model.with_canonical_limits(provider_name)
    }
}

fn materialize_model_config_inner(
    mut model: ModelConfig,
    provider_name: &str,
    include_default_thinking_effort: bool,
) -> Result<ModelConfig> {
    let config = Config::global();

    if model.temperature.is_none() {
        model = model.with_temperature(get_goose_temperature(config)?);
    }

    if model.toolshim && model.toolshim_model.is_none() {
        model = model.with_toolshim_model(get_goose_toolshim_model(config)?);
    }

    model = model.with_default_max_tokens(config.get_goose_max_tokens()?);

    if include_default_thinking_effort {
        model = model.with_default_thinking_effort(config.get_goose_thinking_effort());
    }

    if model.cache_ttl().is_none() {
        if let Some(ttl) = get_goose_cache_ttl(config)? {
            model = model.with_cache_ttl(&ttl);
        }
    }

    if provider_name == goose_providers::openai::OPEN_AI_PROVIDER_NAME {
        model = apply_openai_request_params(model);
    }

    Ok(model)
}

fn one_shot_model_config(model_config: ModelConfig) -> ModelConfig {
    model_config
        .with_thinking_effort(ThinkingEffort::Off)
        .with_prompt_cache_disabled()
}

/// Run a completion for a one-shot auxiliary task on the main session model.
/// Thinking is disabled and prompt-cache writes are skipped because this prompt
/// will not recur.
pub async fn complete_one_shot(
    provider: &dyn Provider,
    model_config: &ModelConfig,
    session_id: &str,
    system: &str,
    messages: &[Message],
    tools: &[Tool],
) -> Result<(Message, ProviderUsage), ProviderError> {
    let one_shot_model_config = one_shot_model_config(model_config.clone());

    crate::session_context::with_session_id(
        Some(session_id.to_string()),
        provider.complete(&one_shot_model_config, system, messages, tools),
    )
    .await
}

fn apply_openai_request_params(mut model: ModelConfig) -> ModelConfig {
    let config = Config::global();
    if let Some(store) = config.get_openai_store() {
        model = model.with_merged_request_params(HashMap::from([(
            "store".to_string(),
            serde_json::json!(store),
        )]));
    }
    model
}

fn base_model_config_from_user_config(
    provider_name: &str,
    model_name: &str,
) -> Result<ModelConfig> {
    let config = Config::global();
    let mut model = ModelConfig {
        model_name: model_name.to_string(),
        context_limit: None,
        temperature: get_goose_temperature(config)?,
        max_tokens: None,
        toolshim: get_goose_toolshim(config)?.unwrap_or(false),
        toolshim_model: get_goose_toolshim_model(config)?,
        request_params: None,
        reasoning: None,
        supports_vision: None,
        request_headers: None,
    };
    if provider_name != goose_providers::azure_foundry::AZURE_FOUNDRY_PROVIDER_NAME {
        model.normalize_effort_suffix();
    }
    Ok(model)
}

/// Re-derive the prompt-cache TTL from the current configuration, discarding
/// any value stored on the model config. The TTL is configuration state, not
/// session state: a resumed session must reflect the user's current opt-in,
/// not a value persisted by an earlier (possibly clamped) run.
pub fn with_rederived_cache_ttl(model: ModelConfig) -> Result<ModelConfig> {
    let mut model = model.without_cache_ttl();
    if let Some(ttl) = get_goose_cache_ttl(Config::global())? {
        model = model.with_cache_ttl(&ttl);
    }
    Ok(model)
}

fn get_goose_cache_ttl(config: &Config) -> Result<Option<String>> {
    match config.get_param::<String>("GOOSE_CACHE_TTL") {
        Ok(ttl) => {
            let ttl = ttl.trim().to_lowercase();
            match ttl.as_str() {
                "5m" | "1h" => Ok(Some(ttl)),
                other => Err(anyhow!(
                    "GOOSE_CACHE_TTL must be '5m' or '1h', got '{other}'"
                )),
            }
        }
        Err(ConfigError::NotFound(_)) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

fn get_goose_temperature(config: &Config) -> Result<Option<f32>> {
    match config.get_param::<f32>("GOOSE_TEMPERATURE") {
        Ok(temp) if temp < 0.0 => Err(anyhow!(
            "Value for 'GOOSE_TEMPERATURE' is out of valid range: {temp}"
        )),
        Ok(temp) => Ok(Some(temp)),
        Err(ConfigError::NotFound(_)) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

fn get_goose_toolshim(config: &Config) -> Result<Option<bool>> {
    match config.get_param::<serde_yaml::Value>("GOOSE_TOOLSHIM") {
        Ok(value) => parse_yaml_bool_config("GOOSE_TOOLSHIM", value).map(Some),
        Err(ConfigError::NotFound(_)) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Resolve the global toolshim setting, defaulting to false when unset.
pub fn global_toolshim() -> bool {
    get_goose_toolshim(Config::global())
        .ok()
        .flatten()
        .unwrap_or(false)
}

fn get_goose_toolshim_model(config: &Config) -> Result<Option<String>> {
    match config.get_param::<String>("GOOSE_TOOLSHIM_OLLAMA_MODEL") {
        Ok(value) if value.trim().is_empty() => Err(anyhow!(
            "Invalid value for 'GOOSE_TOOLSHIM_OLLAMA_MODEL': '{value}' - cannot be empty if set"
        )),
        Ok(value) => Ok(Some(value)),
        Err(ConfigError::NotFound(_)) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

fn parse_bool_config(key: &str, value: &str) -> Result<bool> {
    match value.to_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Ok(true),
        "0" | "false" | "no" | "off" => Ok(false),
        _ => Err(anyhow!(
            "Invalid value for '{key}': '{value}' - must be one of: 1, true, yes, on, 0, false, no, off"
        )),
    }
}

fn parse_yaml_bool_config(key: &str, value: serde_yaml::Value) -> Result<bool> {
    match value {
        serde_yaml::Value::Bool(value) => Ok(value),
        serde_yaml::Value::Number(value) => parse_bool_config(key, &value.to_string()),
        serde_yaml::Value::String(value) => parse_bool_config(key, &value),
        other => {
            Err(anyhow!(
            "Invalid value for '{key}': '{}' - must be one of: 1, true, yes, on, 0, false, no, off",
            serde_yaml::to_string(&other).unwrap_or_else(|_| "<unprintable>".to_string()).trim()
        ))
        }
    }
}

#[cfg(test)]
mod one_shot_tests {
    use super::*;

    #[test]
    fn thinking_and_prompt_cache_are_disabled() {
        let config = one_shot_model_config(
            ModelConfig::new("claude-haiku-4-5").with_thinking_effort(ThinkingEffort::High),
        );

        assert_eq!(config.thinking_effort(), Some(ThinkingEffort::Off));
        assert!(config.prompt_cache_disabled());
    }
}

#[cfg(test)]
mod cache_ttl_tests {
    use super::*;

    #[test]
    fn env_var_populates_cache_ttl() {
        let _guard = env_lock::lock_env([("GOOSE_CACHE_TTL", Some("1h"))]);
        let model = materialize_model_config_inner(
            ModelConfig::new("claude-sonnet-4-5"),
            "anthropic",
            false,
        )
        .unwrap();
        assert_eq!(model.cache_ttl().as_deref(), Some("1h"));
    }

    #[test]
    fn absent_env_var_leaves_cache_ttl_unset() {
        let _guard = env_lock::lock_env([("GOOSE_CACHE_TTL", None::<&str>)]);
        let model = materialize_model_config_inner(
            ModelConfig::new("claude-sonnet-4-5"),
            "anthropic",
            false,
        )
        .unwrap();
        assert!(model.cache_ttl().is_none());
    }

    #[test]
    fn invalid_env_var_is_rejected() {
        let _guard = env_lock::lock_env([("GOOSE_CACHE_TTL", Some("2h"))]);
        let result = materialize_model_config_inner(
            ModelConfig::new("claude-sonnet-4-5"),
            "anthropic",
            false,
        );
        assert!(result.is_err());
    }

    #[test]
    fn rederive_replaces_stored_ttl_with_configured_value() {
        let _guard = env_lock::lock_env([("GOOSE_CACHE_TTL", Some("1h"))]);
        let model =
            with_rederived_cache_ttl(ModelConfig::new("claude-sonnet-4-5").with_cache_ttl("5m"))
                .unwrap();
        assert_eq!(model.cache_ttl().as_deref(), Some("1h"));
    }

    #[test]
    fn rederive_drops_stored_ttl_when_config_absent() {
        let _guard = env_lock::lock_env([("GOOSE_CACHE_TTL", None::<&str>)]);
        let model =
            with_rederived_cache_ttl(ModelConfig::new("claude-sonnet-4-5").with_cache_ttl("1h"))
                .unwrap();
        assert!(model.cache_ttl().is_none());
    }

    #[test]
    fn explicit_model_ttl_wins_over_env_var() {
        let _guard = env_lock::lock_env([("GOOSE_CACHE_TTL", Some("1h"))]);
        let model = materialize_model_config_inner(
            ModelConfig::new("claude-sonnet-4-5").with_cache_ttl("5m"),
            "anthropic",
            false,
        )
        .unwrap();
        assert_eq!(model.cache_ttl().as_deref(), Some("5m"));
    }
}

#[cfg(test)]
mod azure_foundry_tests {
    use super::*;

    #[test]
    fn deployment_name_survives_thinking_effort_changes() {
        let config = base_model_config_from_user_config("azure_foundry", "gpt-5-high")
            .unwrap()
            .with_thinking_effort(ThinkingEffort::Off);

        assert_eq!(config.model_name, "gpt-5-high");
        assert_eq!(config.context_limit, None);
        assert_eq!(config.thinking_effort(), Some(ThinkingEffort::Off));
    }

    #[test]
    fn none_suffixed_deployment_name_is_preserved() {
        let config = base_model_config_from_user_config("azure_foundry", "gpt-5-none").unwrap();

        assert_eq!(config.model_name, "gpt-5-none");
        assert_eq!(config.thinking_effort(), None);
    }
}

/// Whether the agent should be told it may batch independent tool calls.
///
/// Resolution order:
///   1. `GOOSE_PARALLEL_TOOL_CALLS` — explicit user override, wins outright
///   2. the model's known capability
///   3. `false`
///
/// The default is `false` deliberately. With bring-your-own models there is no
/// way to know whether an endpoint handles batched calls, and instructing the
/// agent to batch against a model that does not produces dropped or malformed
/// tool calls. Absence of evidence for a capability is not evidence of it.
///
/// This models the same capability Devin carries on its model catalog
/// (`supports_parallel_tool_calls`). It lives here rather than on `ModelConfig`
/// because that struct has 58 construction sites and no catalog-level slot for
/// it; a resolver gives the identical gate without a 58-site diff for one bool.
pub fn resolved_parallel_tool_calls(provider_name: &str, model_name: &str) -> bool {
    if let Ok(explicit) = Config::global().get_param::<bool>("GOOSE_PARALLEL_TOOL_CALLS") {
        return explicit;
    }
    known_parallel_tool_calls(provider_name, model_name)
}

/// Models goose already treats as supporting batched calls.
///
/// Kept in sync with the per-provider hardcoding it replaces: `chatgpt_codex`
/// sends `parallel_tool_calls: true`, and Bedrock must send `false` for Gemma
/// per AWS documentation. Matching on substrings rather than exact ids because
/// these arrive as dated and suffixed variants.
fn known_parallel_tool_calls(provider_name: &str, model_name: &str) -> bool {
    let provider = provider_name.to_ascii_lowercase();
    let model = model_name.to_ascii_lowercase();

    // Gemma is documented as not supporting parallel tool calls.
    if model.contains("gemma") {
        return false;
    }
    if provider == "bedrock" {
        // Bedrock is conservative across the board today.
        return false;
    }
    if provider.contains("codex") || model.contains("codex") {
        return true;
    }
    // Unknown: do not claim a capability we cannot verify.
    false
}

#[cfg(test)]
mod parallel_tool_call_tests {
    use super::known_parallel_tool_calls;

    #[test]
    fn gemma_is_never_batched_even_though_it_is_bedrock() {
        assert!(!known_parallel_tool_calls("bedrock", "gemma-3-12b"));
    }

    #[test]
    fn bedrock_is_conservative() {
        assert!(!known_parallel_tool_calls("bedrock", "anthropic.claude-3"));
    }

    #[test]
    fn codex_is_known_to_batch() {
        assert!(known_parallel_tool_calls("chatgpt_codex", "gpt-5.3-codex"));
        assert!(known_parallel_tool_calls("openai", "gpt-5.3-codex"));
    }

    #[test]
    fn unknown_models_do_not_claim_the_capability() {
        // the whole point: a BYO endpoint we cannot verify stays sequential
        assert!(!known_parallel_tool_calls("karmx_custom", "local-model"));
        assert!(!known_parallel_tool_calls("ollama", "qwen2.5-coder"));
        assert!(!known_parallel_tool_calls("openai", "gpt-4o"));
    }

    #[test]
    fn matching_is_case_insensitive() {
        assert!(known_parallel_tool_calls("ChatGPT_Codex", "GPT-5.3-Codex"));
        assert!(!known_parallel_tool_calls("Bedrock", "GEMMA-3"));
    }
}
