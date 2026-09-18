//! Native screen-level computer use (macOS).
//!
//! The whole-screen lane `computer_control` can't reach: OCR (Vision) + the
//! accessibility tree turn the frontmost app into numbered items, Jev picks
//! the next action in one multi-Choice request, CGEvent/AXPress actuate.
//! Works on any app — Discord, Finder, Slack — not just Chrome.
//!
//! Loosely a Rust port of awlevin/typesafe-computer-use, minus its writer
//! model: `type_text` is offered only when the caller supplies `text`.

pub mod actuate;
pub mod axwalk;
pub mod capture;
pub mod decide;
pub mod items;
pub mod ocr;
pub mod runner;

use async_trait::async_trait;
use rmcp::model::{
    CallToolResult, ContentBlock, Implementation, InitializeResult, JsonObject, ListToolsResult,
    ServerCapabilities, Tool,
};
use rmcp::object;
use tokio_util::sync::CancellationToken;

use crate::agents::extension::PlatformExtensionContext;
use crate::agents::mcp_client::{Error, McpClientTrait};
use crate::agents::tool_execution::ToolCallContext;

pub static EXTENSION_NAME: &str = "screen_task";
pub const TASK_TOOL: &str = "screen_task";

pub struct ScreenTaskClient {
    info: InitializeResult,
}

impl ScreenTaskClient {
    pub fn new(_context: PlatformExtensionContext) -> anyhow::Result<Self> {
        let info = InitializeResult::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(
                Implementation::new(EXTENSION_NAME.to_string(), "1.0.0".to_string())
                    .with_title("Screen Task"),
            )
            .with_instructions(
                "Drive the whole screen toward a goal: any macOS app, not just the browser.",
            );
        Ok(Self { info })
    }

    fn get_tools() -> Vec<Tool> {
        vec![Tool::new(
            TASK_TOOL.to_string(),
            "Drive the screen toward a plain-English goal, one step at a time — works on \
             ANY app (Discord, Finder, Slack), not just Chrome.\n\n\
             Each step: OCR reads the frontmost window, the accessibility tree lists real \
             controls, Jev picks the next action (click an item, open a site, type, scroll, \
             press a hidden control), and it is performed. Use this for GUI tasks \
             computer_control cannot reach — native apps, cross-app flows.\n\n\
             Prefer computer_control/page_elements/browser_task when the target is a web \
             page in Chrome — structured DOM beats OCR. Actions that look destructive are \
             refused unless confirm: true is passed."
                .to_string(),
            object!({
                "type": "object",
                "properties": {
                    "goal": {
                        "type": "string",
                        "description": "What to achieve, in plain English: 'open discord and find the remote-jobs server'"
                    },
                    "steps": {
                        "type": "integer",
                        "description": "Max steps before stopping (default 12)"
                    },
                    "delay_ms": {
                        "type": "integer",
                        "description": "Pause between steps in ms (default 1500)"
                    },
                    "min_confidence": {
                        "type": "number",
                        "description": "Abort below this Jev confidence (default 0.4)"
                    },
                    "text": {
                        "type": "string",
                        "description": "Free text a type_text step may type into a focused field"
                    },
                    "dry_run": {
                        "type": "boolean",
                        "description": "Plan only — report each step's decision without acting"
                    },
                    "confirm": {
                        "type": "boolean",
                        "description": "Required when the goal could be destructive or sensitive"
                    }
                },
                "required": ["goal"]
            }),
        )]
    }
}

#[async_trait]
impl McpClientTrait for ScreenTaskClient {
    async fn list_tools(
        &self,
        _session_id: &str,
        _next_cursor: Option<String>,
        _cancellation_token: CancellationToken,
    ) -> Result<ListToolsResult, Error> {
        Ok(ListToolsResult::with_all_items(Self::get_tools()))
    }

    async fn call_tool(
        &self,
        _ctx: &ToolCallContext,
        name: &str,
        arguments: Option<JsonObject>,
        _cancellation_token: CancellationToken,
    ) -> Result<CallToolResult, Error> {
        if name != TASK_TOOL {
            return Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                "Unknown tool: {name}"
            ))]));
        }
        Ok(self.run(arguments.unwrap_or_default()).await)
    }

    fn get_info(&self) -> Option<&InitializeResult> {
        Some(&self.info)
    }
}

impl ScreenTaskClient {
    async fn run(&self, args: JsonObject) -> CallToolResult {
        #[cfg(not(target_os = "macos"))]
        {
            let _ = args;
            return CallToolResult::error(vec![ContentBlock::text(
                "screen_task drives macOS apps — unavailable on this platform".to_string(),
            )]);
        }
        #[cfg(target_os = "macos")]
        {
            let goal = args
                .get("goal")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if goal.is_empty() {
                return CallToolResult::error(vec![ContentBlock::text(
                    "missing required parameter: goal".to_string(),
                )]);
            }
            let sensitive = is_sensitive(&goal);
            let confirmed = args
                .get("confirm")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            if sensitive && !confirmed {
                return CallToolResult::error(vec![ContentBlock::text(
                    "goal looks sensitive/destructive — re-run with confirm: true if intended"
                        .to_string(),
                )]);
            }
            let cfg = runner::RunConfig {
                goal,
                steps: args.get("steps").and_then(|v| v.as_u64()).unwrap_or(12) as usize,
                delay_ms: args
                    .get("delay_ms")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(1500),
                min_confidence: args
                    .get("min_confidence")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.4),
                dry_run: args
                    .get("dry_run")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false),
                text: args
                    .get("text")
                    .and_then(|v| v.as_str())
                    .map(str::to_string),
            };
            let (traces, stop) = runner::run(&cfg).await;
            if traces.is_empty() && cfg.dry_run {
                return CallToolResult::error(vec![ContentBlock::text(stop)]);
            }
            let mut out = String::from(if cfg.dry_run { "dry run:\n" } else { "" });
            for t in &traces {
                out.push_str(&format!(
                    "step {}: app={} items={} {} ({:.2}){} {}  [{}ms]\n",
                    t.n,
                    t.app,
                    t.items,
                    t.kind,
                    t.confidence,
                    if t.acted { "" } else { " (not acted)" },
                    t.detail,
                    t.elapsed_ms,
                ));
            }
            out.push_str(&stop);
            CallToolResult::success(vec![ContentBlock::text(out)])
        }
    }
}

/// Words that mark a goal as risky enough to need confirm: true — mirrors the
/// browser lane's gate.
fn is_sensitive(goal: &str) -> bool {
    let g = goal.to_lowercase();
    [
        "delete",
        "remove",
        "pay",
        "purchase",
        "buy",
        "send",
        "submit",
        "post",
        "sign in",
        "log in",
        "password",
        "credential",
        "confirm",
        "erase",
        "format",
        "transfer",
    ]
    .iter()
    .any(|w| g.contains(w))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sensitive_goals_are_flagged() {
        assert!(is_sensitive("delete the folder"));
        assert!(is_sensitive("send the message"));
        assert!(is_sensitive("log in to discord"));
        assert!(!is_sensitive("find a remote jobs server"));
    }
}
