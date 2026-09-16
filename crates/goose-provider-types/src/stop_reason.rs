//! Normalised stop reasons.
//!
//! Providers report why a turn ended using their own vocabulary — `"stop"`,
//! `"end_turn"`, `"STOP"`, `"max_tokens"`, `"length"`, `"incomplete"`. goose
//! stores those strings verbatim on `ProviderUsage::finish_reasons`, so nothing
//! downstream can tell "the model finished" from "the model was cut off" without
//! matching against every provider's spelling.
//!
//! This normalises them, following the recovered Devin `StopReason`:
//!
//! ```text
//! Complete · Error · OutputTruncated · AuthRequired · Restart
//! ```
//!
//! The important one is [`StopReason::OutputTruncated`]. A truncated turn is a
//! *known state* that can be resumed by asking the model to continue, not a
//! failure to be swallowed. Treating it as an error is how a long response
//! silently loses its tail.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StopReason {
    /// The model finished what it was saying.
    Complete,
    /// The model was cut off by an output limit. Resumable: continue the turn.
    OutputTruncated,
    /// The turn ended on an error.
    Error,
    /// Credentials are missing or rejected; requires the user.
    AuthRequired,
    /// The turn was restarted deliberately.
    Restart,
}

impl StopReason {
    /// Whether the turn can simply be continued.
    ///
    /// Only truncation is resumable. An error or an auth failure needs
    /// something to change first, and continuing would just repeat it.
    pub fn is_resumable(self) -> bool {
        matches!(self, StopReason::OutputTruncated)
    }

    /// Whether this reason should be surfaced to the user as a problem.
    pub fn is_failure(self) -> bool {
        matches!(self, StopReason::Error | StopReason::AuthRequired)
    }

    /// Normalise a provider's finish-reason string.
    ///
    /// Unrecognised strings map to [`StopReason::Complete`] rather than
    /// `Error`: an unknown word is not evidence of failure, and guessing wrong
    /// in that direction would surface spurious errors on every new provider.
    pub fn from_finish_reason(reason: &str) -> Self {
        let r = reason.trim().to_ascii_lowercase();
        match r.as_str() {
            // normal completion across providers
            "stop" | "end_turn" | "stop_sequence" | "completed" | "complete" | "tool_calls"
            | "tool_use" | "function_call" => StopReason::Complete,

            // output limits
            "length" | "max_tokens" | "max_output_tokens" | "maxtokens" | "incomplete"
            | "token_limit" => StopReason::OutputTruncated,

            // provider-side refusals and safety stops
            "content_filter" | "safety" | "recitation" | "blocked" | "failed" | "error" => {
                StopReason::Error
            }

            "unauthorized" | "authentication_error" | "auth_required" => StopReason::AuthRequired,

            _ => StopReason::Complete,
        }
    }

    /// Derive a reason from a provider usage record, if it carries one.
    ///
    /// Providers may report several; the most severe wins, so a turn that both
    /// truncated and tripped a filter is not reported as a clean stop.
    pub fn from_finish_reasons(reasons: &[String]) -> Option<Self> {
        let mut found: Option<Self> = None;
        for r in reasons {
            let mapped = Self::from_finish_reason(r);
            found = Some(match (found, mapped) {
                (None, m) => m,
                (Some(StopReason::Error), _) | (_, StopReason::Error) => StopReason::Error,
                (Some(StopReason::AuthRequired), _) | (_, StopReason::AuthRequired) => {
                    StopReason::AuthRequired
                }
                (Some(StopReason::OutputTruncated), _) | (_, StopReason::OutputTruncated) => {
                    StopReason::OutputTruncated
                }
                (Some(StopReason::Restart), _) | (_, StopReason::Restart) => StopReason::Restart,
                _ => StopReason::Complete,
            });
        }
        found
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_every_provider_spelling_of_a_clean_stop() {
        for s in [
            "stop",
            "end_turn",
            "endTurn",
            "STOP",
            " stop ",
            "completed",
            "tool_use",
            "tool_calls",
        ] {
            assert_eq!(
                StopReason::from_finish_reason(s),
                StopReason::Complete,
                "failed on {s:?}"
            );
        }
    }

    #[test]
    fn maps_every_provider_spelling_of_truncation() {
        // these are the strings actually observed across the format layers
        for s in [
            "length",
            "max_tokens",
            "MAX_TOKENS",
            "max_output_tokens",
            "incomplete",
        ] {
            assert_eq!(
                StopReason::from_finish_reason(s),
                StopReason::OutputTruncated,
                "failed on {s:?}"
            );
        }
    }

    #[test]
    fn maps_safety_and_error_stops() {
        for s in [
            "content_filter",
            "SAFETY",
            "RECITATION",
            "blocked",
            "failed",
        ] {
            assert_eq!(
                StopReason::from_finish_reason(s),
                StopReason::Error,
                "failed on {s:?}"
            );
        }
    }

    #[test]
    fn unknown_reason_is_not_treated_as_failure() {
        // guessing "error" for an unrecognised word would spam errors for every
        // provider that adds a new finish reason
        let r = StopReason::from_finish_reason("some_new_reason");
        assert_eq!(r, StopReason::Complete);
        assert!(!r.is_failure());
    }

    #[test]
    fn only_truncation_is_resumable() {
        assert!(StopReason::OutputTruncated.is_resumable());
        assert!(!StopReason::Complete.is_resumable());
        assert!(!StopReason::Error.is_resumable());
        assert!(!StopReason::AuthRequired.is_resumable());
        assert!(!StopReason::Restart.is_resumable());
    }

    #[test]
    fn failures_are_flagged() {
        assert!(StopReason::Error.is_failure());
        assert!(StopReason::AuthRequired.is_failure());
        assert!(!StopReason::OutputTruncated.is_failure());
    }

    #[test]
    fn severity_wins_when_several_reasons_are_reported() {
        let both = vec!["stop".to_string(), "content_filter".to_string()];
        assert_eq!(
            StopReason::from_finish_reasons(&both),
            Some(StopReason::Error)
        );

        let trunc = vec!["length".to_string(), "stop".to_string()];
        assert_eq!(
            StopReason::from_finish_reasons(&trunc),
            Some(StopReason::OutputTruncated),
            "truncation must outrank a clean stop"
        );
    }

    #[test]
    fn no_reasons_yields_none_not_a_guess() {
        assert_eq!(StopReason::from_finish_reasons(&[]), None);
    }

    #[test]
    fn serialises_as_camel_case() {
        let json = serde_json::to_string(&StopReason::OutputTruncated).unwrap();
        assert_eq!(json, "\"outputTruncated\"");
        let back: StopReason = serde_json::from_str(&json).unwrap();
        assert_eq!(back, StopReason::OutputTruncated);
    }
}
