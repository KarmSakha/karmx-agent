//! Undo actions: a tool result declares how to reverse itself.
//!
//! Ported from the recovered Devin tool contract, where reversibility is a
//! property of the *result* rather than something the agent loop infers. Every
//! mutation arrives with its own inverse, which is what makes revert and
//! sidechains tractable — the loop never has to reconstruct intent from a diff.
//!
//! Variant and field names follow the recovered shape:
//! `restore_file`, `expected_post_hash`, `delete_created_file`, `created_dirs`,
//! `recreate`.
//!
//! ## One deliberate addition
//!
//! [`UndoAction::Irreversible`] is not in the recovered set, which expresses
//! "nothing to undo" as an empty list. Those are different facts — "this call
//! made no changes" versus "this call cannot be taken back" — and conflating
//! them means a destructive action looks identical to a no-op. Tools that
//! genuinely cannot be reversed say so explicitly.

use serde::{Deserialize, Serialize};

/// One step needed to reverse a tool call.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum UndoAction {
    /// Put a file back to its pre-call content.
    ///
    /// `expected_post_hash` is a guard: if the file's current content does not
    /// match the hash recorded when the call completed, the file changed
    /// underneath us and the restore must not silently clobber that change.
    RestoreFile {
        path: String,
        expected_post_hash: Option<String>,
    },

    /// Remove a file the call created.
    DeleteCreatedFile { path: String },

    /// Remove directories the call created.
    ///
    /// Ordered deepest-first so a nested set removes cleanly.
    DeleteCreatedDirs { dirs: Vec<String> },

    /// Put back a file the call deleted.
    RecreateFile {
        path: String,
        content: String,
        expected_post_hash: Option<String>,
    },

    /// The call changed something that cannot be restored.
    ///
    /// Carries a reason because the caller has to surface it: an agent about to
    /// revert needs to know which step will not come back.
    Irreversible { reason: String },
}

/// Reserved meta key: a tool declares its reverse steps under this key in the
/// `CallToolResult` meta object.
///
/// Follows the existing `goose.toolChain.summary` convention, so tool authors
/// opt in by setting meta rather than by changing any loop code.
pub const META_KEY: &str = "goose.undoActions";

impl UndoActions {
    /// Lift undo actions out of a tool result's meta object.
    ///
    /// Absent key means "no undo steps recorded", which is the honest default:
    /// we do not invent a revert path for a tool that declared none. A present
    /// but malformed value is also treated as absent rather than failing the
    /// tool call — losing a revert hint must never break the call itself.
    pub fn from_tool_meta(meta: Option<&serde_json::Map<String, serde_json::Value>>) -> Self {
        let Some(value) = meta.and_then(|m| m.get(META_KEY)) else {
            return Self::none();
        };
        match serde_json::from_value::<UndoActions>(value.clone()) {
            Ok(actions) => actions,
            Err(e) => {
                tracing::warn!("ignoring malformed {META_KEY} in tool result meta: {e}");
                Self::none()
            }
        }
    }
}

impl UndoAction {
    pub fn restore(path: impl Into<String>, expected_post_hash: Option<String>) -> Self {
        Self::RestoreFile {
            path: path.into(),
            expected_post_hash,
        }
    }

    pub fn delete_created(path: impl Into<String>) -> Self {
        Self::DeleteCreatedFile { path: path.into() }
    }

    pub fn irreversible(reason: impl Into<String>) -> Self {
        Self::Irreversible {
            reason: reason.into(),
        }
    }

    /// The path this action touches, when it touches exactly one.
    pub fn path(&self) -> Option<&str> {
        match self {
            UndoAction::RestoreFile { path, .. }
            | UndoAction::DeleteCreatedFile { path }
            | UndoAction::RecreateFile { path, .. } => Some(path),
            UndoAction::DeleteCreatedDirs { .. } | UndoAction::Irreversible { .. } => None,
        }
    }
}

/// The set of steps that reverse one tool call.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct UndoActions(Vec<UndoAction>);

impl UndoActions {
    pub fn none() -> Self {
        Self(Vec::new())
    }

    pub fn new(actions: Vec<UndoAction>) -> Self {
        Self(actions)
    }

    pub fn push(&mut self, action: UndoAction) {
        self.0.push(action);
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn iter(&self) -> std::slice::Iter<'_, UndoAction> {
        self.0.iter()
    }

    /// True when at least one step cannot be reversed.
    ///
    /// Callers ask this before advertising a revert path, so a partial undo is
    /// never presented as a clean one.
    pub fn is_partially_irreversible(&self) -> bool {
        self.0
            .iter()
            .any(|a| matches!(a, UndoAction::Irreversible { .. }))
    }

    /// Every action that would fail its content-hash guard.
    ///
    /// `current_hash` is consulted per path; a `None` from it means the file is
    /// gone, which for a restore is also a conflict.
    pub fn conflicting<F>(&self, mut current_hash: F) -> Vec<String>
    where
        F: FnMut(&str) -> Option<String>,
    {
        let mut out = Vec::new();
        for action in &self.0 {
            if let UndoAction::RestoreFile {
                path,
                expected_post_hash: Some(expected),
            } = action
            {
                match current_hash(path) {
                    Some(actual) if &actual == expected => {}
                    _ => out.push(path.clone()),
                }
            }
        }
        out
    }
}

impl FromIterator<UndoAction> for UndoActions {
    fn from_iter<T: IntoIterator<Item = UndoAction>>(iter: T) -> Self {
        Self(iter.into_iter().collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_means_nothing_to_undo() {
        let u = UndoActions::none();
        assert!(u.is_empty());
        assert!(!u.is_partially_irreversible());
    }

    #[test]
    fn irreversible_is_distinct_from_empty() {
        let u = UndoActions::new(vec![UndoAction::irreversible("git push")]);
        assert!(!u.is_empty(), "a real action was recorded");
        assert!(u.is_partially_irreversible());
    }

    #[test]
    fn restore_round_trips_with_guard_hash() {
        let u = UndoActions::new(vec![UndoAction::restore("src/a.rs", Some("abc123".into()))]);
        let json = serde_json::to_string(&u).unwrap();
        let back: UndoActions = serde_json::from_str(&json).unwrap();
        assert_eq!(u, back);
        assert!(json.contains("expectedPostHash"));
    }

    #[test]
    fn omitted_guard_hash_deserializes() {
        let u: UndoActions =
            serde_json::from_str(r#"[{"kind":"restoreFile","path":"a.rs"}]"#).unwrap();
        match &u.iter().next().unwrap() {
            UndoAction::RestoreFile {
                expected_post_hash, ..
            } => assert!(expected_post_hash.is_none()),
            other => panic!("wrong variant: {other:?}"),
        }
    }

    #[test]
    fn conflicting_detects_changed_file() {
        let u = UndoActions::new(vec![UndoAction::restore("a.rs", Some("hash1".into()))]);
        // unchanged -> no conflict
        assert!(u.conflicting(|_| Some("hash1".into())).is_empty());
        // changed -> conflict
        assert_eq!(u.conflicting(|_| Some("hash2".into())), vec!["a.rs"]);
        // missing -> conflict, a restore onto a deleted file is not safe
        assert_eq!(u.conflicting(|_| None), vec!["a.rs"]);
    }

    #[test]
    fn unguarded_restore_never_conflicts() {
        let u = UndoActions::new(vec![UndoAction::restore("a.rs", None)]);
        assert!(u.conflicting(|_| Some("whatever".into())).is_empty());
    }

    #[test]
    fn path_accessor_is_absent_for_multi_path_actions() {
        assert_eq!(UndoAction::delete_created("a.rs").path(), Some("a.rs"));
        assert_eq!(
            UndoAction::DeleteCreatedDirs {
                dirs: vec!["a".into(), "b".into()]
            }
            .path(),
            None
        );
    }

    #[test]
    fn lifts_actions_from_tool_meta() {
        let mut meta = serde_json::Map::new();
        meta.insert(
            META_KEY.to_string(),
            serde_json::json!([{"kind":"deleteCreatedFile","path":"new.rs"}]),
        );
        let u = UndoActions::from_tool_meta(Some(&meta));
        assert_eq!(u.len(), 1);
        assert_eq!(u.iter().next().unwrap().path(), Some("new.rs"));
    }

    #[test]
    fn absent_or_malformed_meta_yields_no_actions_not_an_error() {
        assert!(UndoActions::from_tool_meta(None).is_empty());

        let mut meta = serde_json::Map::new();
        meta.insert(META_KEY.to_string(), serde_json::json!("not an array"));
        assert!(UndoActions::from_tool_meta(Some(&meta)).is_empty());

        let empty = serde_json::Map::new();
        assert!(UndoActions::from_tool_meta(Some(&empty)).is_empty());
    }

    #[test]
    fn collected_from_iterator() {
        let u: UndoActions = vec![
            UndoAction::delete_created("a"),
            UndoAction::delete_created("b"),
        ]
        .into_iter()
        .collect();
        assert_eq!(u.len(), 2);
    }
}
