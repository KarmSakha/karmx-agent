//! Retrieval budget model — ported from the live capture.
//!
//! The captured `retrieval.json` shows the shape of a real retrieval call:
//!
//! ```json
//! { "total_budget": 20000, "codebase_budget": 20000, "conversation_budget": 0,
//!   "codebase_chunks_retrieved": 0, "conversation_chunks_initial": 0,
//!   "conversation_queries_generated": 0, "conversation_chunks_followup": 0,
//!   "conversation_chunks_combined": 0,
//!   "codebase_result_len_pre_truncation": 44, "codebase_result_len": 44,
//!   "combined_result_len": 44,
//!   "codebase_truncated": false, "conversation_truncated": false,
//!   "final_truncated": false }
//! ```
//!
//! Two things are worth noticing and are preserved here:
//!   * the budget is split between two sources (codebase, conversation) that
//!     share one total, and unused budget is not borrowed
//!   * every stage records a pre- and post-truncation length, so the caller can
//!     tell "there was nothing" apart from "we cut it off"

use serde::Serialize;

/// Default total retrieval budget, from the capture (`total_budget: 20000`).
pub const DEFAULT_TOTAL_BUDGET: usize = 20_000;

#[derive(Debug, Clone, Serialize, Default)]
pub struct PhaseTimings {
    pub codebase_retrieval_elapsed_ms: u64,
    pub conversation_retrieval_elapsed_ms: u64,
    pub conversation_initial_retrieval_ms: u64,
    pub conversation_query_rewriting_ms: u64,
    pub conversation_followup_retrieval_ms: u64,
    pub blob_name_lookup_ms: u64,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct ChunkCounts {
    pub codebase_chunks_retrieved: usize,
    pub conversation_chunks_initial: usize,
    pub conversation_queries_generated: usize,
    pub conversation_chunks_followup: usize,
    pub conversation_chunks_combined: usize,
}

/// Budget split across retrieval sources.
#[derive(Debug, Clone, Copy)]
pub struct Budget {
    pub total: usize,
    pub codebase: usize,
    pub conversation: usize,
}

impl Budget {
    /// Split a total budget. `conversation_share` is clamped to the total so the
    /// two halves can never exceed it (the capture shows conversation at 0 when
    /// there is no conversation history to search).
    pub fn split(total: usize, conversation_share: usize) -> Self {
        let conversation = conversation_share.min(total);
        Self {
            total,
            codebase: total - conversation,
            conversation,
        }
    }

    pub fn default_budget() -> Self {
        Self::split(DEFAULT_TOTAL_BUDGET, 0)
    }
}

/// Truncate `s` to at most `budget` bytes on a char boundary.
///
/// Returns `(text, was_truncated)`.
#[allow(clippy::string_slice)] // `end` is walked back to a char boundary before slicing.
pub fn truncate_to_budget(s: &str, budget: usize) -> (String, bool) {
    if s.len() <= budget {
        return (s.to_string(), false);
    }
    let mut end = budget;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    (s[..end].to_string(), true)
}

/// Per-call retrieval metrics, matching the captured field names.
#[derive(Debug, Clone, Serialize, Default)]
pub struct RetrievalMetrics {
    pub total_budget: usize,
    pub codebase_budget: usize,
    pub conversation_budget: usize,
    #[serde(flatten)]
    pub timings: PhaseTimings,
    #[serde(flatten)]
    pub counts: ChunkCounts,
    pub codebase_result_len_pre_truncation: usize,
    pub codebase_result_len: usize,
    pub conversation_result_len_pre_truncation: usize,
    pub conversation_result_len: usize,
    pub combined_result_len: usize,
    pub codebase_truncated: bool,
    pub conversation_truncated: bool,
    pub final_truncated: bool,
}

/// Combine two retrieval results under one shared budget.
///
/// Codebase results are kept first because they are the higher-signal source for
/// a coding task; conversation history fills what remains. Both stages record
/// their pre-truncation length so callers can distinguish "empty" from "cut".
pub fn combine(budget: Budget, codebase: &str, conversation: &str) -> (String, RetrievalMetrics) {
    let codebase_pre = codebase.len();
    let conversation_pre = conversation.len();

    let (cb, cb_trunc) = truncate_to_budget(codebase, budget.codebase);
    let (cv, cv_trunc) = truncate_to_budget(conversation, budget.conversation);

    let mut combined = cb.clone();
    if !cv.is_empty() {
        if !combined.is_empty() {
            combined.push_str("\n\n");
        }
        combined.push_str(&cv);
    }

    let combined_pre = combined.len();
    let (final_text, final_trunc) = truncate_to_budget(&combined, budget.total);

    let metrics = RetrievalMetrics {
        total_budget: budget.total,
        codebase_budget: budget.codebase,
        conversation_budget: budget.conversation,
        codebase_result_len_pre_truncation: codebase_pre,
        codebase_result_len: cb.len(),
        conversation_result_len_pre_truncation: conversation_pre,
        conversation_result_len: cv.len(),
        combined_result_len: final_text.len(),
        codebase_truncated: cb_trunc,
        conversation_truncated: cv_trunc,
        final_truncated: final_trunc || combined_pre > budget.total,
        ..Default::default()
    };
    (final_text, metrics)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn budget_halves_never_exceed_total() {
        let b = Budget::split(1000, 400);
        assert_eq!(b.codebase, 600);
        assert_eq!(b.conversation, 400);
        assert_eq!(b.codebase + b.conversation, b.total);

        // conversation share larger than total is clamped
        let b2 = Budget::split(1000, 5000);
        assert_eq!(b2.conversation, 1000);
        assert_eq!(b2.codebase, 0);
    }

    #[test]
    fn default_budget_matches_capture() {
        let b = Budget::default_budget();
        assert_eq!(b.total, 20_000);
        assert_eq!(b.codebase, 20_000);
        assert_eq!(b.conversation, 0);
    }

    #[test]
    fn truncate_reports_flag() {
        let (t, hit) = truncate_to_budget("abcdef", 3);
        assert_eq!(t, "abc");
        assert!(hit);
        let (t2, hit2) = truncate_to_budget("abc", 10);
        assert_eq!(t2, "abc");
        assert!(!hit2);
    }

    #[test]
    fn truncate_respects_multibyte_boundary() {
        let s = "αααα"; // 2 bytes each
        let (t, hit) = truncate_to_budget(s, 3);
        assert!(hit);
        assert_eq!(t, "α"); // 2 bytes, not a broken 3-byte slice
    }

    #[test]
    fn combine_records_both_stages() {
        let (text, m) = combine(Budget::split(100, 40), "code", "conv");
        assert!(text.contains("code"));
        assert!(text.contains("conv"));
        assert_eq!(m.total_budget, 100);
        assert_eq!(m.codebase_result_len_pre_truncation, 4);
        assert_eq!(m.conversation_result_len_pre_truncation, 4);
        assert!(!m.codebase_truncated);
    }

    #[test]
    fn combine_marks_truncation_and_distinguishes_empty() {
        let cb = "x".repeat(500);
        let (_, m) = combine(Budget::split(100, 0), &cb, "");
        assert!(m.codebase_truncated, "500 bytes into 100 must truncate");
        assert_eq!(m.codebase_result_len, 100);
        assert_eq!(m.conversation_result_len, 0);
        assert_eq!(m.codebase_result_len_pre_truncation, 500);
    }
}
