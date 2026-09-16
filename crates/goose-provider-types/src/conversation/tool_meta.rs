//! Reserved tool-result meta keys.
//!
//! Tools communicate structured facts about a call by setting these keys in the
//! `CallToolResult` meta object. The agent loop lifts them onto the
//! [`super::ToolResponse`] where they become addressable fields rather than
//! prose the model has to parse out of a message.
//!
//! The convention mirrors the pre-existing `goose.toolChain.summary` key, so a
//! tool opts in by setting meta and no loop code changes.

/// Steps that reverse the call's effects. See [`super::undo::META_KEY`].
pub const UNDO_ACTIONS: &str = super::undo::META_KEY;

/// Content that was moved out of the message because it was too large.
///
/// Set by the large-response handler so the original text stays retrievable
/// instead of being replaced by a pointer string.
pub const OVERFLOW_CONTENT: &str = "goose.overflowContent";

/// Why a call failed, when the tool can say more than "it failed".
pub const FAILURE_REASON: &str = "goose.failureReason";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_namespaced_and_distinct() {
        let keys = [UNDO_ACTIONS, OVERFLOW_CONTENT, FAILURE_REASON];
        for k in keys {
            assert!(k.starts_with("goose."), "{k} should be namespaced");
        }
        let mut sorted = keys.to_vec();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), keys.len(), "keys must be unique");
    }

    #[test]
    fn undo_key_is_re_exported_not_duplicated() {
        // one definition, so the two cannot drift apart
        assert_eq!(UNDO_ACTIONS, super::super::undo::META_KEY);
    }
}
