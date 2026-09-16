//! Devin-style presentation for the goose REPL.
//!
//! These are the UI ideas worth taking from the recovered Devin interface —
//! hint bar, session stats, diff rendering — implemented as pure formatting so
//! they work on goose's existing line-based REPL.
//!
//! Deliberately not a renderer. Devin's TUI is a hand-rolled cell-level surface
//! (`scrollback` + `chisel-ui`, 27 modules) with its own input controller.
//! Reimplementing that replaces the part of a terminal application that is
//! hardest to get right and easiest to get subtly wrong. The visible value is
//! in what is *shown*, not in who owns the cursor.

use console::{style, Term};

/// One key hint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hint {
    pub key: &'static str,
    pub label: &'static str,
}

/// What the user can do right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Context {
    /// Waiting for input.
    Idle,
    /// The model is producing output.
    Streaming,
    /// A tool is running.
    ToolRunning,
    /// A question is waiting on the user.
    AwaitingAnswer,
}

impl Context {
    fn hints(self) -> &'static [Hint] {
        match self {
            Context::Idle => &[
                Hint {
                    key: "enter",
                    label: "send",
                },
                Hint {
                    key: "ctrl+j",
                    label: "newline",
                },
                Hint {
                    key: "ctrl+c",
                    label: "exit",
                },
                Hint {
                    key: "?",
                    label: "shortcuts",
                },
            ],
            Context::Streaming => &[
                Hint {
                    key: "esc",
                    label: "interrupt",
                },
                Hint {
                    key: "ctrl+o",
                    label: "expand",
                },
            ],
            Context::ToolRunning => &[
                Hint {
                    key: "esc",
                    label: "cancel tool",
                },
                Hint {
                    key: "ctrl+o",
                    label: "show output",
                },
            ],
            Context::AwaitingAnswer => &[
                Hint {
                    key: "1-9",
                    label: "choose",
                },
                Hint {
                    key: "enter",
                    label: "confirm",
                },
            ],
        }
    }
}

/// Render the hint bar for the current context.
///
/// `width` is the terminal width; hints are dropped from the end rather than
/// wrapped, because a wrapped hint bar steals a line from the conversation on
/// every render.
pub fn hint_bar(context: Context, width: usize) -> String {
    let hints = context.hints();
    let mut out = String::new();
    for h in hints {
        // "key label" + separator
        let piece = format!("{} {}", style(h.key).dim(), style(h.label).dim());
        let projected = out.len() + piece.len() + 3;
        if projected > width {
            break;
        }
        if !out.is_empty() {
            out.push_str("  ");
        }
        out.push_str(&piece);
    }
    out
}

/// Usage summary for the session.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SessionStats {
    pub turns: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cached_tokens: u64,
    pub tool_calls: u64,
    pub elapsed_secs: u64,
}

fn thousands(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

impl SessionStats {
    /// Fraction of input tokens served from cache, 0.0 when there were none.
    ///
    /// Worth surfacing because prompt-cache hits are the difference between a
    /// cheap long session and an expensive one.
    pub fn cache_hit_rate(&self) -> f64 {
        let total = self.input_tokens + self.cached_tokens;
        if total == 0 {
            0.0
        } else {
            self.cached_tokens as f64 / total as f64
        }
    }

    pub fn format_elapsed(&self) -> String {
        let (m, s) = (self.elapsed_secs / 60, self.elapsed_secs % 60);
        if m >= 60 {
            format!("{}h{:02}m", m / 60, m % 60)
        } else if m > 0 {
            format!("{m}m{s:02}s")
        } else {
            format!("{s}s")
        }
    }

    /// One compact line, or two when there is cache activity worth showing.
    pub fn render(&self) -> String {
        let mut line = format!(
            "{} turns · {} in · {} out · {} tools · {}",
            self.turns,
            thousands(self.input_tokens),
            thousands(self.output_tokens),
            self.tool_calls,
            self.format_elapsed()
        );
        if self.cached_tokens > 0 {
            line.push_str(&format!(" · cache {:.0}%", self.cache_hit_rate() * 100.0));
        }
        style(line).dim().to_string()
    }
}

/// A single line of a unified diff.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffLine<'a> {
    Added(&'a str),
    Removed(&'a str),
    Context(&'a str),
    Header(&'a str),
}

/// Classify one line of unified diff text.
pub fn classify_diff_line(line: &str) -> DiffLine<'_> {
    if line.starts_with("+++") || line.starts_with("---") {
        DiffLine::Header(line)
    } else if line.starts_with('+') {
        DiffLine::Added(line)
    } else if line.starts_with('-') {
        DiffLine::Removed(line)
    } else {
        DiffLine::Context(line)
    }
}

/// Render a unified diff with colour.
///
/// `max_lines` bounds the output; an elision marker is appended rather than
/// silently truncating, so a partial diff never reads as the whole change.
pub fn render_diff(diff: &str, max_lines: usize) -> String {
    let lines: Vec<&str> = diff.lines().collect();
    let mut out = String::new();
    for line in lines.iter().take(max_lines) {
        let rendered = match classify_diff_line(line) {
            DiffLine::Added(l) => style(l).green().to_string(),
            DiffLine::Removed(l) => style(l).red().to_string(),
            DiffLine::Header(l) => style(l).cyan().to_string(),
            DiffLine::Context(l) => l.to_string(),
        };
        out.push_str(&rendered);
        out.push('\n');
    }
    if lines.len() > max_lines {
        out.push_str(
            &style(format!(
                "… {} more line(s) not shown",
                lines.len() - max_lines
            ))
            .dim()
            .to_string(),
        );
        out.push('\n');
    }
    out
}

/// Count added and removed lines, for a one-line summary.
pub fn diff_summary(diff: &str) -> (usize, usize) {
    let (mut add, mut del) = (0, 0);
    for line in diff.lines() {
        match classify_diff_line(line) {
            DiffLine::Added(_) => add += 1,
            DiffLine::Removed(_) => del += 1,
            _ => {}
        }
    }
    (add, del)
}

/// Terminal width, falling back to 80 when not a tty.
pub fn term_width() -> usize {
    Term::stdout().size().1 as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_shows_send_and_exit() {
        let b = hint_bar(Context::Idle, 200);
        assert!(b.contains("send"));
        assert!(b.contains("exit"));
    }

    #[test]
    fn streaming_offers_interrupt_not_exit() {
        let b = hint_bar(Context::Streaming, 200);
        assert!(b.contains("interrupt"));
        assert!(
            !b.contains("exit"),
            "exit is not the useful hint mid-stream"
        );
    }

    #[test]
    fn hint_bar_never_exceeds_width() {
        for w in [10, 20, 40, 80] {
            let b = hint_bar(Context::Idle, w);
            assert!(b.len() <= w, "width {w} produced {} chars: {b:?}", b.len());
        }
    }

    #[test]
    fn narrow_terminal_still_renders_something_or_nothing() {
        // must not panic or emit a partial escape sequence
        let b = hint_bar(Context::Idle, 1);
        assert!(b.len() <= 1);
    }

    #[test]
    fn stats_render_includes_turns_and_tools() {
        let s = SessionStats {
            turns: 12,
            input_tokens: 1_234,
            output_tokens: 567,
            cached_tokens: 0,
            tool_calls: 5,
            elapsed_secs: 90,
        };
        let r = s.render();
        assert!(r.contains("12 turns"));
        assert!(r.contains("5 tools"));
        assert!(r.contains("1m30s"));
    }

    #[test]
    fn thousands_separates_groups() {
        let s = SessionStats {
            input_tokens: 1_234_567,
            ..Default::default()
        };
        assert!(s.render().contains("1,234,567"));
    }

    #[test]
    fn cache_rate_is_zero_without_cache() {
        let s = SessionStats {
            input_tokens: 100,
            cached_tokens: 0,
            ..Default::default()
        };
        assert_eq!(s.cache_hit_rate(), 0.0);
        assert!(!s.render().contains("cache"));
    }

    #[test]
    fn cache_rate_is_reported_when_present() {
        let s = SessionStats {
            input_tokens: 100,
            cached_tokens: 300,
            ..Default::default()
        };
        assert!((s.cache_hit_rate() - 0.75).abs() < 1e-9);
        assert!(s.render().contains("cache 75%"));
    }

    #[test]
    fn elapsed_formats_hours_minutes_seconds() {
        let f = |s| {
            SessionStats {
                elapsed_secs: s,
                ..Default::default()
            }
            .format_elapsed()
        };
        assert_eq!(f(45), "45s");
        assert_eq!(f(90), "1m30s");
        assert_eq!(f(3700), "1h01m");
    }

    #[test]
    fn diff_lines_classify_correctly() {
        assert!(matches!(classify_diff_line("+added"), DiffLine::Added(_)));
        assert!(matches!(classify_diff_line("-gone"), DiffLine::Removed(_)));
        assert!(matches!(classify_diff_line("+++ b/f"), DiffLine::Header(_)));
        assert!(matches!(classify_diff_line("--- a/f"), DiffLine::Header(_)));
        assert!(matches!(classify_diff_line(" ctx"), DiffLine::Context(_)));
    }

    #[test]
    fn diff_headers_are_not_counted_as_changes() {
        let d = "--- a/f\n+++ b/f\n+one\n-two\n ctx";
        assert_eq!(diff_summary(d), (1, 1));
    }

    #[test]
    fn render_diff_elides_visibly() {
        let d = (0..20)
            .map(|i| format!("+line {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let r = render_diff(&d, 5);
        assert!(r.contains("line 4"));
        assert!(!r.contains("line 5"));
        assert!(
            r.contains("15 more line(s) not shown"),
            "must say what was hidden"
        );
    }

    #[test]
    fn render_diff_includes_all_when_under_limit() {
        let r = render_diff("+a\n-b\n", 10);
        assert!(r.contains("+a") && r.contains("-b"));
        assert!(!r.contains("not shown"));
    }

    #[test]
    fn term_width_is_sane_when_not_a_tty() {
        let w = term_width();
        assert!(w > 0, "width must never be zero");
    }
}
