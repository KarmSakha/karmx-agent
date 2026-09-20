//! The step loop: AX snapshot → capture → OCR → merge → Jev decides → actuate.
//! Stops on done/none, low confidence, or two consecutive no-op decisions.

#![cfg(target_os = "macos")]

use super::{actuate, axwalk, capture, decide, items, ocr};
use items::{Item, Screen};
use std::time::Duration;

pub struct RunConfig {
    pub goal: String,
    pub app: Option<String>,
    pub steps: usize,
    pub delay_ms: u64,
    pub min_confidence: f64,
    pub dry_run: bool,
    /// Free text `type_text` may type — the caller supplies it; Jev only
    /// decides *that* typing is the move.
    pub text: Option<String>,
}

pub struct StepTrace {
    pub n: usize,
    pub app: String,
    pub items: usize,
    pub kind: String,
    pub detail: String,
    pub confidence: f64,
    pub acted: bool,
    pub elapsed_ms: u128,
}

/// One full pass, preserving the reason if sensing fails.
fn sense(
    expected_app: Option<&str>,
) -> anyhow::Result<(
    axwalk::AxInfo,
    capture::Capture,
    Vec<items::OcrLine>,
    Vec<Item>,
)> {
    {
        let display = display_bounds();
        let ax = axwalk::snapshot(&display).ok_or_else(|| {
            anyhow::anyhow!(
                "Cannot identify the foreground application through macOS Accessibility"
            )
        })?;
        check_app(expected_app, &ax.app)?;
        let cap = capture::grab(ax.window)?;
        let lines = ocr::read_lines(&cap);
        let merged = items::merge_items(&lines, &ax.pressables);
        Ok((ax, cap, lines, merged))
    }
}

fn check_app(expected: Option<&str>, actual: &str) -> anyhow::Result<()> {
    if let Some(expected) = expected {
        if !expected.eq_ignore_ascii_case(actual) {
            anyhow::bail!("Expected foreground app '{expected}', but found '{actual}'. Bring the requested app forward before retrying. No action performed.");
        }
    }
    Ok(())
}

fn display_bounds() -> items::Frame {
    let d = core_graphics::display::CGDisplay::main();
    let b = d.bounds();
    items::Frame {
        x: b.origin.x,
        y: b.origin.y,
        w: b.size.width,
        h: b.size.height,
    }
}

fn chrome_url() -> Option<String> {
    let out = std::process::Command::new("osascript")
        .args([
            "-e",
            "tell application \"Google Chrome\" to get URL of active tab of front window",
        ])
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
        .filter(|s| s.starts_with("http"))
}

pub fn check_permissions() -> anyhow::Result<()> {
    unsafe extern "C" {
        fn CGPreflightScreenCaptureAccess() -> bool;
        fn AXIsProcessTrusted() -> bool;
    }
    let mut missing = Vec::new();
    unsafe {
        if !AXIsProcessTrusted() {
            missing.push("Accessibility");
        }
        if !CGPreflightScreenCaptureAccess() {
            missing.push("Screen Recording");
        }
    }
    if !missing.is_empty() {
        anyhow::bail!("macOS permission required: {}. Enable KarmX (or the terminal hosting the CLI) in System Settings > Privacy & Security, then restart the app. No actions were performed.", missing.join(" and "));
    }
    Ok(())
}

/// Run the loop. Returns per-step traces plus a stop reason.
pub async fn run(cfg: &RunConfig) -> (Vec<StepTrace>, String) {
    let mut traces = Vec::new();
    if let Err(error) = check_permissions() {
        return (traces, error.to_string());
    }
    let mut history: Vec<String> = Vec::new();
    let mut noops = 0usize;

    for n in 1..=cfg.steps {
        let started = std::time::Instant::now();
        let (ax, cap, lines, mut merged) = match sense(cfg.app.as_deref()) {
            Ok(screen) => screen,
            Err(error) => return (traces, format!("Screen sensing failed: {error:#}")),
        };
        merged.truncate(240);
        let url = (ax.app == "Google Chrome").then(chrome_url).flatten();
        let screen = Screen {
            app: ax.app.clone(),
            url,
            region: cap.region,
            image_w: cap.image_w,
            image_h: cap.image_h,
            field: ax.field.clone(),
            offscreen: ax.offscreen.clone(),
        };
        let state = items::state_payload(&cfg.goal, &screen, &merged, &history);

        let Some(decision) = decide::decide(&state, &merged, &screen).await else {
            return (
                traces,
                "Jev unavailable or timed out — set TYPESAFE_API_KEY".to_string(),
            );
        };

        if axwalk::frontmost().map(|(_, pid)| pid) != Some(ax.pid) {
            return (traces, "Foreground app changed while planning. Stopped without performing the pending action; retry with the intended app in front.".to_string());
        }

        if decision.confidence < cfg.min_confidence {
            traces.push(StepTrace {
                n,
                app: ax.app.clone(),
                items: merged.len(),
                kind: decision.kind.clone(),
                detail: String::new(),
                confidence: decision.confidence,
                acted: false,
                elapsed_ms: started.elapsed().as_millis(),
            });
            return (
                traces,
                format!(
                    "stopping: confidence {:.2} below {:.2}",
                    decision.confidence, cfg.min_confidence
                ),
            );
        }

        if decision.stops() {
            traces.push(StepTrace {
                n,
                app: ax.app.clone(),
                items: merged.len(),
                kind: decision.kind.clone(),
                detail: String::new(),
                confidence: decision.confidence,
                acted: false,
                elapsed_ms: started.elapsed().as_millis(),
            });
            let observed = lines
                .iter()
                .map(|line| line.text.as_str())
                .collect::<Vec<_>>()
                .join("\n");
            return (
                traces,
                format!(
                    "stopping: {}\nObserved screen text:\n{}",
                    decision.kind, observed
                ),
            );
        }

        let (detail, noop) = if cfg.dry_run {
            (describe(&decision, &merged), false)
        } else {
            match act(&decision, &merged, &ax, cfg).await {
                Ok(detail) => (detail, is_noop(&decision)),
                Err(e) => (format!("action failed: {e}"), true),
            }
        };
        history.push(format!("{} {}", decision.kind, detail));
        if noop {
            noops += 1;
        } else {
            noops = 0;
        }
        traces.push(StepTrace {
            n,
            app: ax.app.clone(),
            items: merged.len(),
            kind: decision.kind.clone(),
            detail: detail.clone(),
            confidence: decision.confidence,
            acted: !cfg.dry_run && !detail.starts_with("action failed:"),
            elapsed_ms: started.elapsed().as_millis(),
        });
        if noops >= 2 {
            return (traces, "stopping: repeated no-op actions".to_string());
        }
        tokio::time::sleep(Duration::from_millis(cfg.delay_ms)).await;
    }
    (
        traces,
        format!("stopping: step budget {} reached", cfg.steps),
    )
}

fn is_noop(decision: &decide::Decision) -> bool {
    matches!(decision.kind.as_str(), "wait")
}

fn describe(decision: &decide::Decision, items: &[Item]) -> String {
    match decision.kind.as_str() {
        "click_item" => decision
            .item_index
            .and_then(|i| items.get(i))
            .map(|it| format!("[{}] '{}'", it.index, it.text))
            .unwrap_or_default(),
        "open_site" => decision.site.clone().unwrap_or_default(),
        _ => String::new(),
    }
}

async fn act(
    decision: &decide::Decision,
    items: &[Item],
    ax: &axwalk::AxInfo,
    cfg: &RunConfig,
) -> anyhow::Result<String> {
    match decision.kind.as_str() {
        "click_item" => {
            let i = decision
                .item_index
                .ok_or_else(|| anyhow::anyhow!("no item chosen"))?;
            let item = items
                .get(i)
                .ok_or_else(|| anyhow::anyhow!("item {i} out of range"))?;
            actuate::click(&item.frame)?;
            Ok(format!("[{}] '{}'", item.index, item.text))
        }
        "press_offscreen" => {
            let key = decision
                .offscreen_key
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("no offscreen control chosen"))?;
            let idx: usize = key
                .trim_start_matches("off")
                .parse()
                .map_err(|_| anyhow::anyhow!("bad offscreen key {key}"))?;
            let element = ax
                .offscreen_elements
                .get(idx)
                .ok_or_else(|| anyhow::anyhow!("offscreen {key} out of range"))?;
            actuate::ax_press(&element.0)?;
            Ok(key.to_string())
        }
        "open_site" => {
            let site = decision
                .site
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("no site chosen"))?;
            let url = site
                .starts_with("http")
                .then(|| site.to_string())
                .unwrap_or_else(|| format!("https://{site}"));
            std::process::Command::new("open").arg(&url).spawn()?;
            Ok(url)
        }
        "switch_to_browser" => {
            std::process::Command::new("open")
                .args(["-a", "Google Chrome"])
                .spawn()?;
            Ok(String::new())
        }
        "type_text" => {
            let text = cfg
                .text
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("type_text chosen but no `text` arg given"))?;
            actuate::type_text(text)?;
            Ok(format!("typed {} chars", text.chars().count()))
        }
        "press_enter" => {
            actuate::press_key("return")?;
            Ok(String::new())
        }
        "press_escape" => {
            actuate::press_key("escape")?;
            Ok(String::new())
        }
        "scroll_down" => {
            actuate::scroll(-3)?;
            Ok(String::new())
        }
        "scroll_up" => {
            actuate::scroll(3)?;
            Ok(String::new())
        }
        "wait" => {
            tokio::time::sleep(Duration::from_secs(1)).await;
            Ok(String::new())
        }
        other => Err(anyhow::anyhow!("unhandled action {other}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_guard_rejects_the_wrong_foreground() {
        assert!(check_app(Some("Calculator"), "ChatGPT").is_err());
        assert!(check_app(Some("Calculator"), "calculator").is_ok());
        assert!(check_app(None, "Finder").is_ok());
    }

    /// Drives the real screen — run manually with `cargo test -- --ignored`.
    #[tokio::test]
    #[ignore = "needs Screen Recording + Accessibility and a live Jev key"]
    async fn dry_run_senses_the_frontmost_app() {
        let cfg = RunConfig {
            goal: "open the browser".to_string(),
            app: None,
            steps: 1,
            delay_ms: 0,
            min_confidence: 0.0,
            dry_run: true,
            text: None,
        };
        let (traces, stop) = run(&cfg).await;
        assert!(!traces.is_empty(), "no step ran: {stop}");
        let t = &traces[0];
        println!(
            "app={} items={} kind={} conf={:.2} detail={} stop={stop}",
            t.app, t.items, t.kind, t.confidence, t.detail
        );
    }
}
