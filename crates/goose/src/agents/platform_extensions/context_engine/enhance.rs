//! Prompt enhancer for `enhance_prompt`.
//!
//! Behaviour:
//!   * trims the prompt and rejects empty input with `empty_prompt`
//!   * appends the user's prompt to the enhancement instruction
//!   * parses the result out of `<enhanced-prompt>` XML tags
//!   * **fails open** — on any error the caller keeps the original prompt
//!   * distinguishes api / network / unknown / parsing failures in the status
//!
//! The instruction text is `prompts/enhance.txt`.
//!
//! Runs on the session's own model, so it uses whatever provider and model the
//! user selected.

use std::sync::Arc;

use goose_providers::model::ModelConfig;

use crate::conversation::message::Message;
use crate::providers::base::Provider;

/// The verbatim enhancement instruction. The user's prompt is appended after it.
pub const ENHANCE_INSTRUCTION: &str = include_str!("prompts/enhance.txt");

/// Outcome of an enhancement attempt.
///
/// Mirrors the original's status union so callers can report the same reasons.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    EmptyPrompt,
    Success,
    ApiError,
    NetworkError,
    UnknownError,
    ParsingError,
}

impl Status {
    pub fn as_str(&self) -> &'static str {
        match self {
            Status::EmptyPrompt => "empty_prompt",
            Status::Success => "success",
            Status::ApiError => "api_error",
            Status::NetworkError => "network_error",
            Status::UnknownError => "unknown_error",
            Status::ParsingError => "parsing_error",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Enhancement {
    pub status: Status,
    /// The prompt to actually use. On any failure this is the original.
    pub prompt: String,
    /// True only when an enhanced prompt was produced and parsed.
    pub was_enhanced: bool,
    pub error_message: Option<String>,
}

impl Enhancement {
    fn passthrough(original: &str, status: Status, err: Option<String>) -> Self {
        Self {
            status,
            prompt: original.to_string(),
            was_enhanced: false,
            error_message: err,
        }
    }
}

/// Extract the enhanced prompt from a model response.
///
/// Accepts `<enhanced-prompt>` plus a couple of longer tag spellings so older
/// responses still parse.
#[allow(clippy::string_slice)] // Indices come from find() on ASCII tags; byte slicing is safe.
pub fn parse_enhanced_prompt(response: &str) -> Option<String> {
    for tag in [
        "enhanced-prompt",
        "augment-enhanced-prompt",
        "vendor-enhanced-prompt",
    ] {
        let open = format!("<{tag}>");
        let close = format!("</{tag}>");
        if let Some(start) = response.find(&open) {
            let after = start + open.len();
            if let Some(rel_end) = response[after..].find(&close) {
                let inner = response[after..after + rel_end].trim();
                if !inner.is_empty() {
                    return Some(inner.to_string());
                }
            }
        }
    }
    None
}

/// Classify a provider error into the same buckets the original used.
fn classify(err: &str) -> Status {
    let s = err.to_lowercase();
    if s.contains("network")
        || s.contains("timeout")
        || s.contains("timed out")
        || s.contains("connection")
        || s.contains("connect")
    {
        Status::NetworkError
    } else if s.contains("status") || s.contains("http") {
        Status::ApiError
    } else {
        Status::UnknownError
    }
}

/// Build the full instruction sent to the model.
pub fn build_request(user_prompt: &str) -> String {
    format!("{ENHANCE_INSTRUCTION}{user_prompt}")
}

/// Enhance `prompt` using the session's provider and model.
///
/// Never fails hard: every error path returns the original prompt so the caller
/// can proceed. That fail-open behaviour is deliberate and matches the original
/// component, which always logged "Continuing with original prompt...".
pub async fn enhance(
    provider: &Arc<dyn Provider>,
    model_config: &ModelConfig,
    prompt: &str,
) -> Enhancement {
    let trimmed = prompt.trim();
    if trimmed.is_empty() {
        return Enhancement::passthrough(
            prompt,
            Status::EmptyPrompt,
            Some("Cannot enhance empty prompt".into()),
        );
    }

    let request = build_request(trimmed);
    run(provider, model_config, prompt, request).await
}

/// Like [`enhance`], but the rewrite model also sees session context (recent
/// conversation lines, working directory) so it can resolve references like
/// "this file" or "that error" instead of guessing.
pub async fn enhance_with_context(
    provider: &Arc<dyn Provider>,
    model_config: &ModelConfig,
    prompt: &str,
    context: &str,
) -> Enhancement {
    let trimmed = prompt.trim();
    if trimmed.is_empty() {
        return Enhancement::passthrough(
            prompt,
            Status::EmptyPrompt,
            Some("Cannot enhance empty prompt".into()),
        );
    }
    if context.trim().is_empty() {
        return enhance(provider, model_config, prompt).await;
    }
    let request = format!(
        "Session context (use it to resolve references, do not answer it):\n{context}\n\n{}",
        build_request(trimmed)
    );
    run(provider, model_config, prompt, request).await
}

async fn run(
    provider: &Arc<dyn Provider>,
    model_config: &ModelConfig,
    prompt: &str,
    request: String,
) -> Enhancement {
    let msg = Message::user().with_text(request);

    let response = match provider.complete(model_config, "", &[msg], &[]).await {
        Ok((message, _usage)) => message.as_concat_text(),
        Err(e) => {
            let text = e.to_string();
            let status = classify(&text);
            tracing::warn!(
                status = status.as_str(),
                "prompt enhancement failed: {text}"
            );
            return Enhancement::passthrough(prompt, status, Some(text));
        }
    };

    match parse_enhanced_prompt(&response) {
        Some(enhanced) => Enhancement {
            status: Status::Success,
            prompt: enhanced,
            was_enhanced: true,
            error_message: None,
        },
        None => {
            tracing::warn!("prompt enhancer response had no <enhanced-prompt> block");
            Enhancement::passthrough(
                prompt,
                Status::ParsingError,
                Some("Failed to parse enhanced prompt from response".into()),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_short_tag() {
        let r = "noise <enhanced-prompt>Do the thing properly.</enhanced-prompt> more";
        assert_eq!(parse_enhanced_prompt(r).unwrap(), "Do the thing properly.");
    }

    #[test]
    fn parses_long_tag_variant() {
        let r = "<augment-enhanced-prompt>\n  Better prompt\n</augment-enhanced-prompt>";
        assert_eq!(parse_enhanced_prompt(r).unwrap(), "Better prompt");
    }

    #[test]
    fn rejects_empty_tag_body() {
        assert!(parse_enhanced_prompt("<enhanced-prompt>   </enhanced-prompt>").is_none());
    }

    #[test]
    fn returns_none_when_absent() {
        assert!(parse_enhanced_prompt("just a normal answer").is_none());
    }

    #[test]
    fn instruction_requests_the_tag_format() {
        // guard the contract the parser depends on
        assert!(ENHANCE_INSTRUCTION.contains("<enhanced-prompt>"));
        assert!(ENHANCE_INSTRUCTION.contains("### BEGIN RESPONSE ###"));
    }

    #[test]
    fn builds_request_with_prompt_appended() {
        let r = build_request("fix the bug");
        assert!(r.starts_with("Here is an instruction"));
        assert!(r.ends_with("fix the bug"));
    }

    #[test]
    fn classify_maps_transport_failures() {
        assert_eq!(classify("connection refused"), Status::NetworkError);
        assert_eq!(classify("request timed out"), Status::NetworkError);
        assert_eq!(classify("http status 500"), Status::ApiError);
        assert_eq!(classify("something odd"), Status::UnknownError);
    }
}
