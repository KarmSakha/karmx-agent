//! Predictive compaction thresholds.
//!
//! goose's existing behaviour is to check a single fraction of the context
//! limit and, when exceeded, summarise **inline** — which stalls the turn for
//! however long the summariser takes, at the worst possible moment.
//!
//! Devin's recovered scheme uses three absolute token counts instead:
//!
//! ```text
//! --compaction-thresholds <spawn>,<apply>,<hard>     e.g. 183500,183500,262144
//! ```
//!
//! * **spawn** — begin producing a summary in the background
//! * **apply** — prefer the ready summary from here on
//! * **hard** — block inference until a summary exists
//!
//! The point is that the summariser runs while there is still headroom, so the
//! cost is paid against idle time rather than against the user's turn. A
//! two-value form `<spawn>,<hard>` degenerates to "only act at the hard limit",
//! which is goose's current reactive behaviour and is kept as the fallback.

use std::fmt;

/// Which side of the thresholds a token count falls on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// Below spawn. Nothing to do.
    Idle,
    /// At or past spawn, before apply. A summary may be started and kept warm.
    Spawning,
    /// At or past apply, before hard. Prefer the ready summary.
    Applying,
    /// At or past hard. Must not call inference without a summary.
    Blocking,
}

impl Phase {
    /// Whether a background summary should be running in this phase.
    pub fn wants_summary(self) -> bool {
        matches!(self, Phase::Spawning | Phase::Applying | Phase::Blocking)
    }

    /// Whether inference must wait for a summary before proceeding.
    pub fn is_blocking(self) -> bool {
        matches!(self, Phase::Blocking)
    }
}

/// Three absolute token thresholds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompactionThresholds {
    pub spawn: usize,
    pub apply: usize,
    pub hard: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThresholdParseError(String);

impl fmt::Display for ThresholdParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for ThresholdParseError {}

impl CompactionThresholds {
    pub fn new(spawn: usize, apply: usize, hard: usize) -> Result<Self, ThresholdParseError> {
        if spawn == 0 || apply == 0 || hard == 0 {
            return Err(ThresholdParseError(
                "thresholds must be positive token counts".into(),
            ));
        }
        if !(spawn <= apply && apply <= hard) {
            return Err(ThresholdParseError(format!(
                "thresholds must satisfy spawn <= apply <= hard, got {spawn},{apply},{hard}"
            )));
        }
        Ok(Self { spawn, apply, hard })
    }

    /// Parse `<spawn>,[<apply>,]<hard>`.
    ///
    /// Two values sets `apply = spawn`, so summarising starts at spawn but does
    /// not hold up inference until hard.
    pub fn parse(s: &str) -> Result<Self, ThresholdParseError> {
        let parts: Vec<&str> = s.split(',').map(str::trim).collect();
        let nums: Result<Vec<usize>, _> = parts
            .iter()
            .map(|p| {
                p.parse::<usize>().map_err(|_| {
                    ThresholdParseError(format!(
                        "expected positive token counts, got {p:?} in {s:?}"
                    ))
                })
            })
            .collect();
        let nums = nums?;
        match nums.as_slice() {
            [spawn, hard] => Self::new(*spawn, *spawn, *hard),
            [spawn, apply, hard] => Self::new(*spawn, *apply, *hard),
            _ => Err(ThresholdParseError(format!(
                "expected two or three comma-separated counts, got {}",
                nums.len()
            ))),
        }
    }

    /// Derive thresholds from a context limit and the existing fraction knob.
    ///
    /// Used when no explicit thresholds are configured, so behaviour degrades to
    /// the documented single-threshold model rather than to something invented.
    /// `spawn` starts a little earlier than the fraction so the summary has room
    /// to finish; `hard` is the context limit itself.
    pub fn from_context_limit(limit: usize, fraction: f64) -> Self {
        let fraction = fraction.clamp(0.05, 1.0);
        // Clamp every derived value to at least 1 token and to the limit, so a
        // tiny window combined with a small fraction cannot produce a zero or
        // out-of-order threshold. The ordering invariant is what callers rely
        // on; it must hold for every input, not just realistic ones.
        let hard = limit.max(1);
        let apply = (((limit as f64) * fraction) as usize).clamp(1, hard);
        // start a tenth of the remaining headroom early, so a summary for a
        // large window is not begun at the moment it is already needed
        let headroom = hard.saturating_sub(apply);
        let spawn = apply.saturating_sub(headroom / 10).clamp(1, apply);
        Self { spawn, apply, hard }
    }

    /// Which phase `tokens` falls in.
    ///
    /// Comparison is `>=` for every boundary: reaching a threshold means acting
    /// on it, not waiting until one token past it.
    pub fn phase(&self, tokens: usize) -> Phase {
        if tokens >= self.hard {
            Phase::Blocking
        } else if tokens >= self.apply {
            Phase::Applying
        } else if tokens >= self.spawn {
            Phase::Spawning
        } else {
            Phase::Idle
        }
    }

    /// Whether the summariser should be started at `tokens`.
    pub fn should_spawn(&self, tokens: usize) -> bool {
        self.phase(tokens).wants_summary()
    }

    /// Whether inference must wait for a summary at `tokens`.
    pub fn must_block(&self, tokens: usize) -> bool {
        self.phase(tokens).is_blocking()
    }
}

impl fmt::Display for CompactionThresholds {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{},{},{}", self.spawn, self.apply, self.hard)
    }
}

/// Whether a summary produced for an earlier token count is still usable.
///
/// A summary is only valid for the history it was built from. Once the
/// conversation has grown past the point the summary was started at, applying
/// it would silently drop everything in between — so a stale summary is
/// discarded rather than applied.
pub fn summary_is_fresh(
    started_at_tokens: usize,
    current_tokens: usize,
    stale_after: usize,
) -> bool {
    current_tokens.saturating_sub(started_at_tokens) <= stale_after
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_three_value_form() {
        let t = CompactionThresholds::parse("183500,183500,262144").unwrap();
        assert_eq!(t.spawn, 183_500);
        assert_eq!(t.apply, 183_500);
        assert_eq!(t.hard, 262_144);
    }

    #[test]
    fn parses_the_two_value_form_as_spawn_equals_apply() {
        let t = CompactionThresholds::parse("183500,262144").unwrap();
        assert_eq!(t.spawn, 183_500, "two-value form applies at spawn");
        assert_eq!(t.apply, 183_500);
        assert_eq!(t.hard, 262_144);
    }

    #[test]
    fn tolerates_whitespace() {
        let t = CompactionThresholds::parse(" 100 , 200 , 300 ").unwrap();
        assert_eq!((t.spawn, t.apply, t.hard), (100, 200, 300));
    }

    #[test]
    fn rejects_out_of_order_thresholds() {
        let e = CompactionThresholds::parse("300,200,100")
            .unwrap_err()
            .to_string();
        assert!(e.contains("spawn <= apply <= hard"), "got: {e}");
    }

    #[test]
    fn rejects_zero_and_non_numeric_and_wrong_arity() {
        assert!(CompactionThresholds::parse("0,100,200").is_err());
        assert!(CompactionThresholds::parse("a,b,c").is_err());
        assert!(CompactionThresholds::parse("100").is_err());
        assert!(CompactionThresholds::parse("1,2,3,4").is_err());
    }

    #[test]
    fn equality_across_all_three_is_allowed() {
        // spawn == apply == hard is degenerate but coherent: block immediately
        let t = CompactionThresholds::parse("100,100,100").unwrap();
        assert_eq!(t.phase(100), Phase::Blocking);
    }

    #[test]
    fn phase_boundaries_are_inclusive() {
        let t = CompactionThresholds::new(100, 200, 300).unwrap();
        assert_eq!(t.phase(99), Phase::Idle);
        assert_eq!(t.phase(100), Phase::Spawning, "reaching spawn acts on it");
        assert_eq!(t.phase(199), Phase::Spawning);
        assert_eq!(t.phase(200), Phase::Applying);
        assert_eq!(t.phase(299), Phase::Applying);
        assert_eq!(t.phase(300), Phase::Blocking);
        assert_eq!(t.phase(1_000_000), Phase::Blocking);
    }

    #[test]
    fn only_blocking_phase_holds_up_inference() {
        let t = CompactionThresholds::new(100, 200, 300).unwrap();
        assert!(!t.must_block(99));
        assert!(!t.must_block(100), "spawning must not stall the turn");
        assert!(!t.must_block(200), "applying must not stall the turn");
        assert!(t.must_block(300));
    }

    #[test]
    fn summary_is_wanted_from_spawn_onward() {
        let t = CompactionThresholds::new(100, 200, 300).unwrap();
        assert!(!t.should_spawn(99));
        assert!(t.should_spawn(100));
        assert!(t.should_spawn(250));
        assert!(t.should_spawn(300));
    }

    #[test]
    fn derived_thresholds_start_before_apply_and_end_at_the_limit() {
        let t = CompactionThresholds::from_context_limit(200_000, 0.8);
        assert_eq!(t.apply, 160_000);
        assert_eq!(t.hard, 200_000);
        assert!(t.spawn < t.apply, "spawn must lead apply to be predictive");
        assert_eq!(t.spawn, 156_000, "a tenth of the headroom early");
    }

    #[test]
    fn derived_thresholds_are_always_ordered() {
        for limit in [1usize, 2, 7, 1_000, 8_192, 200_000, 2_000_000] {
            for f in [0.05f64, 0.5, 0.8, 0.95, 1.0] {
                let t = CompactionThresholds::from_context_limit(limit, f);
                assert!(
                    t.spawn <= t.apply && t.apply <= t.hard,
                    "limit={limit} f={f} -> {t}"
                );
                assert!(t.spawn > 0);
            }
        }
    }

    #[test]
    fn derived_thresholds_reach_blocking_at_the_limit() {
        let t = CompactionThresholds::from_context_limit(1000, 0.8);
        assert_eq!(t.phase(t.hard), Phase::Blocking);
        assert!(!t.phase(t.hard - 1).is_blocking());
    }

    #[test]
    fn display_round_trips_through_parse() {
        let t = CompactionThresholds::new(10, 20, 30).unwrap();
        assert_eq!(t.to_string(), "10,20,30");
        assert_eq!(CompactionThresholds::parse(&t.to_string()).unwrap(), t);
    }

    #[test]
    fn fresh_summary_is_usable_stale_one_is_not() {
        assert!(summary_is_fresh(100, 150, 100), "grew less than the window");
        assert!(summary_is_fresh(100, 200, 100), "exactly at the window");
        assert!(!summary_is_fresh(100, 201, 100), "grew past the window");
        assert!(summary_is_fresh(100, 100, 0), "no growth is always fresh");
        assert!(!summary_is_fresh(100, 101, 0));
    }
}
