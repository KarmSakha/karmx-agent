//! Native computer control.
//!
//! Drives a real browser over CDP: pointer, keyboard, scroll, drag, and
//! screenshots. This is *actuation*, distinct from `browser_preview`, which
//! only observes what a page renders.
//!
//! Runs in the agent process. No MCP server, no subprocess.
//!
//! The action vocabulary is the recovered 9-field shape — see [`actions`].

pub mod actions;
pub mod cdp;

use std::sync::Arc;

use async_trait::async_trait;
use rmcp::model::{
    CallToolResult, ContentBlock, Implementation, InitializeResult, JsonObject, ListToolsResult,
    ServerCapabilities, Tool,
};
use rmcp::object;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use crate::agents::extension::PlatformExtensionContext;
use crate::agents::mcp_client::{Error, McpClientTrait};
use crate::agents::tool_execution::ToolCallContext;

use actions::{ComputerAction, Point};
use cdp::{CdpClient, CdpTarget};

pub static EXTENSION_NAME: &str = "computer_control";
pub const CONTROL_TOOL: &str = "computer_control";

/// Default debug port for a locally launched Chrome.
const DEFAULT_DEBUG_PORT: u16 = 9222;

/// A connected session, cached so successive tool calls reuse one browser.
struct Session {
    client: Arc<CdpClient>,
    port: u16,
}

pub struct ComputerControlClient {
    info: InitializeResult,
    session: Mutex<Option<Session>>,
}

impl ComputerControlClient {
    pub fn new(_context: PlatformExtensionContext) -> anyhow::Result<Self> {
        let info = InitializeResult::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(
                Implementation::new(EXTENSION_NAME.to_string(), "1.0.0".to_string())
                    .with_title("Computer Control"),
            )
            .with_instructions("Drive a browser over CDP: click, type, scroll, drag, screenshot.");
        Ok(Self {
            info,
            session: Mutex::new(None),
        })
    }

    fn get_tools() -> Vec<Tool> {
        vec![Tool::new(
            CONTROL_TOOL.to_string(),
            "Drive a real browser: click, double-click, right-click, move, type, press a key, \
             scroll, drag, capture a screenshot, or wait. Provide one or more actions; they run \
             in order.\n\n\
             This is actuation, not inspection: use it to operate a page. To read what a page \
             renders without interacting, use browser_preview instead.\n\n\
             Requires Chrome running with --remote-debugging-port=<port> (default 9222). \
             Coordinates are viewport pixels; a screenshot is returned afterwards by default so \
             you can see the result of what you did."
                .to_string(),
            object!({
                "type": "object",
                "properties": {
                    "actions": {
                        "type": "array",
                        "minItems": 1,
                        "description": "Actions to perform, in order.",
                        "items": {
                            "type": "object",
                            "required": ["action_type"],
                            "properties": {
                                "action_type": {
                                    "type": "string",
                                    "enum": ["click","double_click","right_click","move","type","key","scroll","drag","screenshot","wait"]
                                },
                                "coordinate": {
                                    "type": "object",
                                    "description": "Target position {x,y} for pointer actions.",
                                    "properties": {"x":{"type":"integer"},"y":{"type":"integer"}},
                                    "required": ["x","y"]
                                },
                                "start_coordinate": {
                                    "type": "object",
                                    "description": "Drag origin {x,y}.",
                                    "properties": {"x":{"type":"integer"},"y":{"type":"integer"}},
                                    "required": ["x","y"]
                                },
                                "key": { "type": "string", "description": "Key name for `key`, e.g. Enter, Tab, Escape." },
                                "text": { "type": "string", "description": "Text to insert for `type`." },
                                "scroll_amount": { "type": "integer", "description": "Scroll magnitude." },
                                "scroll_direction": { "type": "string", "enum": ["up","down","left","right"] },
                                "region": {
                                    "type": "object",
                                    "description": "Crop for `screenshot`.",
                                    "properties": {
                                        "x":{"type":"integer"},"y":{"type":"integer"},
                                        "width":{"type":"integer"},"height":{"type":"integer"}
                                    },
                                    "required": ["x","y","width","height"]
                                },
                                "duration": { "type": "integer", "description": "Milliseconds for `wait`." }
                            }
                        }
                    },
                    "url": {
                        "type": "string",
                        "description": "Navigate here before running the actions."
                    },
                    "port": {
                        "type": "integer",
                        "description": "Chrome remote-debugging port (default 9222)."
                    },
                    "screenshot": {
                        "type": "boolean",
                        "default": true,
                        "description": "Capture a screenshot after the actions (default true)."
                    }
                },
                "required": ["actions"]
            }),
        )]
    }

    /// Reuse the cached session, or attach to `port`.
    async fn ensure_session(&self, port: u16) -> Result<Arc<CdpClient>, String> {
        let mut guard = self.session.lock().await;
        if let Some(s) = guard.as_ref() {
            if s.port == port {
                return Ok(s.client.clone());
            }
        }
        let target = CdpTarget {
            host: "127.0.0.1".to_string(),
            port,
        };
        let client = CdpClient::attach(&target)
            .await
            .map_err(|e| format!("{e}"))?;
        let client = Arc::new(client);
        *guard = Some(Session {
            client: client.clone(),
            port,
        });
        Ok(client)
    }

    fn parse_actions(v: &serde_json::Value) -> Result<Vec<ComputerAction>, String> {
        let arr = v
            .as_array()
            .ok_or_else(|| "`actions` must be an array".to_string())?;
        if arr.is_empty() {
            return Err("`actions` must contain at least one action".into());
        }
        arr.iter()
            .enumerate()
            .map(|(i, a)| {
                serde_json::from_value::<ComputerAction>(a.clone())
                    .map_err(|e| format!("action {i} is malformed: {e}"))
            })
            .collect()
    }
}

#[async_trait]
impl McpClientTrait for ComputerControlClient {
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
        if name != CONTROL_TOOL {
            return Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                "Unknown tool: {name}"
            ))]));
        }
        let args = serde_json::Value::Object(arguments.unwrap_or_default());

        let actions =
            match Self::parse_actions(args.get("actions").unwrap_or(&serde_json::Value::Null)) {
                Ok(a) => a,
                Err(e) => return Ok(CallToolResult::error(vec![ContentBlock::text(e)])),
            };

        // validate everything before touching the browser, so a bad third action
        // cannot leave the page half-driven
        for (i, a) in actions.iter().enumerate() {
            if let Err(e) = a.validate() {
                return Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                    "action {i} ({}) is invalid: {e}",
                    a.action_type
                ))]));
            }
        }

        let port = args
            .get("port")
            .and_then(|v| v.as_u64())
            .map(|v| v as u16)
            .unwrap_or(DEFAULT_DEBUG_PORT);

        let client = match self.ensure_session(port).await {
            Ok(c) => c,
            Err(e) => {
                return Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                    "Could not attach to a browser on port {port}: {e}\n\n\
                     Start Chrome with: --remote-debugging-port={port}"
                ))]))
            }
        };

        if let Some(url) = args.get("url").and_then(|v| v.as_str()) {
            if let Err(e) = client
                .call("Page.navigate", serde_json::json!({ "url": url }))
                .await
            {
                return Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                    "navigation to {url} failed: {e}"
                ))]));
            }
        }

        // bring input online once per session
        let _ = client
            .call(
                "Runtime.evaluate",
                serde_json::json!({"expression": "1", "returnByValue": true}),
            )
            .await;

        let mut log = Vec::new();
        for a in &actions {
            let t = a.action_type.as_str();
            if t == "wait" {
                let ms = a.duration.unwrap_or(0);
                tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
                log.push(format!("wait {ms}ms"));
                continue;
            }
            let cmds = match a.to_cdp() {
                Ok(c) => c,
                Err(e) => {
                    return Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                        "action {t} could not be lowered: {e}"
                    ))]))
                }
            };
            for (method, params) in cmds {
                if let Err(e) = client.call(&method, params).await {
                    return Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                        "action {t} failed at {method}: {e}"
                    ))]));
                }
            }
            log.push(match a.coordinate {
                Some(Point { x, y }) => format!("{t} at ({x},{y})"),
                None => t.to_string(),
            });
        }

        let info = client.page_info().await.unwrap_or_default();
        let url = info.get("url").cloned().unwrap_or_default();
        let title = info.get("title").cloned().unwrap_or_default();

        let want_shot = args
            .get("screenshot")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        let mut out = format!(
            "performed {} action(s):\n{}\n\npage: {title}\nurl: {url}",
            log.len(),
            log.iter()
                .map(|l| format!("  - {l}"))
                .collect::<Vec<_>>()
                .join("\n")
        );

        if want_shot {
            match client
                .screenshot(serde_json::json!({ "format": "png" }))
                .await
            {
                Ok(png) => {
                    out.push_str(&format!("\n\nscreenshot: {} bytes (png)", png.len()));
                }
                Err(e) => out.push_str(&format!("\n\nscreenshot failed: {e}")),
            }
        }

        Ok(CallToolResult::success(vec![ContentBlock::text(out)]))
    }

    fn get_info(&self) -> Option<&InitializeResult> {
        Some(&self.info)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exposes_the_control_tool() {
        let tools = ComputerControlClient::get_tools();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name.as_ref(), CONTROL_TOOL);
    }

    #[test]
    fn action_parsing_rejects_empty_and_malformed() {
        assert!(ComputerControlClient::parse_actions(&serde_json::json!([])).is_err());
        assert!(ComputerControlClient::parse_actions(&serde_json::json!("nope")).is_err());
        assert!(ComputerControlClient::parse_actions(&serde_json::json!([{"nope":1}])).is_err());
    }

    #[test]
    fn action_parsing_accepts_a_click() {
        let v = serde_json::json!([{"action_type":"click","coordinate":{"x":1,"y":2}}]);
        let a = ComputerControlClient::parse_actions(&v).unwrap();
        assert_eq!(a.len(), 1);
        assert!(a[0].validate().is_ok());
    }

    #[tokio::test]
    async fn attach_failure_is_reported_not_panicked() {
        let ctx = || PlatformExtensionContext {
            extension_manager: None,
            session_manager: unreachable_sm(),
            scheduler: None,
            session: None,
            use_login_shell_path: false,
        };
        let c = ComputerControlClient::new(ctx()).unwrap();
        // port 1 is never listening
        let err = match c.ensure_session(1).await {
            Ok(_) => panic!("port 1 should never be reachable"),
            Err(e) => e,
        };
        assert!(err.contains("cannot reach Chrome debug port"), "got: {err}");
    }

    fn unreachable_sm() -> Arc<crate::session::SessionManager> {
        // only constructed, never used by these tests
        Arc::new(crate::session::SessionManager::new(
            std::env::temp_dir().join("karmx-cc-test"),
        ))
    }
}
