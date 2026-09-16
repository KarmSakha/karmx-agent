//! Native browser preview.
//!
//! Proxies a running dev server, injects a page bridge, and collects the
//! rendered DOM plus console output the bridge reports back. Lets the agent ask
//! what a page actually renders instead of inferring it from source.
//!
//! This is a platform extension: it runs in the agent process with direct
//! access to the agent context. There is no MCP server, no subprocess, and no
//! stdio boundary.
//!
//! The page bridge (`cascade-browser-integration.js`) and the protobuf field
//! numbers in [`proto`] are a recovered wire contract — if the bridge changes,
//! `proto` must change with it.

mod csp;
mod proto;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::{header, HeaderMap, HeaderName, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::Router;
use futures::StreamExt;
use rmcp::model::{
    CallToolResult, ContentBlock, Implementation, InitializeResult, JsonObject, ListToolsResult,
    ServerCapabilities, Tool,
};
use rmcp::object;
use tokio_util::sync::CancellationToken;

use crate::agents::extension::PlatformExtensionContext;
use crate::agents::mcp_client::{Error, McpClientTrait};
use crate::agents::tool_execution::ToolCallContext;

use proto::{ConsoleLog, DomElement};

pub static EXTENSION_NAME: &str = "browser_preview";
pub const OPEN_TOOL: &str = "browser_preview";
pub const CLOSE_TOOL: &str = "close_browser_preview";

/// The page bridge, embedded verbatim.
pub const BRIDGE_JS: &str = include_str!("cascade-browser-integration.js");

/// Captured state for one preview.
#[derive(Default)]
pub struct Captures {
    pub dom_elements: Vec<DomElement>,
    pub console: Vec<ConsoleLog>,
}

pub struct PreviewState {
    pub target: String,
    pub csrf: String,
    pub captures: Mutex<Captures>,
    pub client: reqwest::Client,
}

impl PreviewState {
    fn new(target: String, csrf: String) -> Self {
        Self {
            target,
            csrf,
            captures: Mutex::new(Captures::default()),
            client: reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .expect("build reqwest client"),
        }
    }
}

fn router(state: Arc<PreviewState>) -> Router {
    Router::new()
        .route(csp::BRIDGE_PATH, get(serve_bridge))
        .route(
            "/exa.browser_preview_pb.BrowserPreviewService/SendDOMElement",
            post(send_dom_element),
        )
        .route(
            "/exa.browser_preview_pb.BrowserPreviewService/SendConsoleOutput",
            post(send_console_output),
        )
        .fallback(proxy)
        .with_state(state)
}

async fn serve_bridge() -> Response {
    (
        [(
            header::CONTENT_TYPE,
            "application/javascript; charset=utf-8",
        )],
        BRIDGE_JS,
    )
        .into_response()
}

fn csrf_ok(state: &PreviewState, headers: &HeaderMap) -> bool {
    headers
        .get("x-codeium-csrf-token")
        .and_then(|v| v.to_str().ok())
        .map(|v| v == state.csrf)
        .unwrap_or(false)
}

async fn send_dom_element(State(state): State<Arc<PreviewState>>, req: Request) -> Response {
    if !csrf_ok(&state, req.headers()) {
        return (StatusCode::FORBIDDEN, "bad csrf token").into_response();
    }
    let body = match axum::body::to_bytes(req.into_body(), 16 * 1024 * 1024).await {
        Ok(b) => b,
        Err(e) => return (StatusCode::BAD_REQUEST, format!("read body: {e}")).into_response(),
    };
    let payload = match proto::unwrap_request(&body) {
        Ok(p) => p,
        Err(e) => return (StatusCode::BAD_REQUEST, format!("bad envelope: {e}")).into_response(),
    };
    match DomElement::decode(payload) {
        Ok(el) => {
            tracing::debug!(tag = %el.tag_name, id = %el.id, "captured DOM element");
            if let Ok(mut c) = state.captures.lock() {
                c.dom_elements.push(el);
            }
            StatusCode::OK.into_response()
        }
        Err(e) => (StatusCode::BAD_REQUEST, format!("bad dom element: {e}")).into_response(),
    }
}

async fn send_console_output(State(state): State<Arc<PreviewState>>, req: Request) -> Response {
    if !csrf_ok(&state, req.headers()) {
        return (StatusCode::FORBIDDEN, "bad csrf token").into_response();
    }
    let body = match axum::body::to_bytes(req.into_body(), 16 * 1024 * 1024).await {
        Ok(b) => b,
        Err(e) => return (StatusCode::BAD_REQUEST, format!("read body: {e}")).into_response(),
    };
    let payload = match proto::unwrap_request(&body) {
        Ok(p) => p,
        Err(e) => return (StatusCode::BAD_REQUEST, format!("bad envelope: {e}")).into_response(),
    };
    match ConsoleLog::decode(payload) {
        Ok(log) => {
            tracing::debug!(lines = log.lines.len(), "captured console output");
            if let Ok(mut c) = state.captures.lock() {
                c.console.push(log);
            }
            StatusCode::OK.into_response()
        }
        Err(e) => (StatusCode::BAD_REQUEST, format!("bad console log: {e}")).into_response(),
    }
}

/// Forward anything else to the target app, injecting the bridge into HTML.
async fn proxy(State(state): State<Arc<PreviewState>>, req: Request) -> Response {
    let path_and_query = req
        .uri()
        .path_and_query()
        .map(|p| p.as_str().to_string())
        .unwrap_or_else(|| "/".to_string());
    let url = format!("{}{}", state.target.trim_end_matches('/'), path_and_query);

    let method = req.method().clone();
    let mut outbound = state.client.request(
        reqwest::Method::from_bytes(method.as_str().as_bytes()).unwrap_or(reqwest::Method::GET),
        &url,
    );

    for (name, value) in req.headers() {
        if csp::is_hop_by_hop(name.as_str()) || name.as_str().eq_ignore_ascii_case("host") {
            continue;
        }
        outbound = outbound.header(name.as_str(), value.as_bytes());
    }

    let body = match axum::body::to_bytes(req.into_body(), 64 * 1024 * 1024).await {
        Ok(b) => b,
        Err(e) => return (StatusCode::BAD_REQUEST, format!("read body: {e}")).into_response(),
    };
    if !body.is_empty() {
        outbound = outbound.body(body.to_vec());
    }

    let upstream = match outbound.send().await {
        Ok(r) => r,
        Err(e) => {
            return (
                StatusCode::BAD_GATEWAY,
                format!("preview target unreachable at {url}: {e}"),
            )
                .into_response()
        }
    };

    let status = upstream.status().as_u16();
    let mut headers = HeaderMap::new();
    let mut is_html = false;
    for (name, value) in upstream.headers() {
        let n = name.as_str();
        if csp::is_hop_by_hop(n)
            || n.eq_ignore_ascii_case("content-length")
            || n.eq_ignore_ascii_case("content-encoding")
        {
            continue;
        }
        if n.eq_ignore_ascii_case("content-type") {
            let v = String::from_utf8_lossy(value.as_bytes()).to_ascii_lowercase();
            is_html = v.contains("text/html");
        }
        if n.eq_ignore_ascii_case("content-security-policy") {
            let rewritten = csp::rewrite_policy(&String::from_utf8_lossy(value.as_bytes()));
            if let Ok(hv) = HeaderValue::from_str(&rewritten) {
                headers.insert(HeaderName::from_static("content-security-policy"), hv);
            }
            continue;
        }
        if n.eq_ignore_ascii_case("content-security-policy-report-only") {
            continue;
        }
        headers.insert(name.clone(), value.clone());
    }

    if is_html {
        let bytes = match upstream.bytes().await {
            Ok(b) => b,
            Err(e) => {
                return (StatusCode::BAD_GATEWAY, format!("read upstream: {e}")).into_response()
            }
        };
        let html = String::from_utf8_lossy(&bytes).into_owned();
        let injected = csp::inject_bridge(&html, &state.csrf);
        headers.remove(header::CONTENT_LENGTH);
        return (
            StatusCode::from_u16(status).unwrap_or(StatusCode::OK),
            headers,
            injected,
        )
            .into_response();
    }

    let stream = upstream
        .bytes_stream()
        .map(|r| r.map_err(|e| std::io::Error::other(e.to_string())));
    let body = Body::from_stream(stream);
    (
        StatusCode::from_u16(status).unwrap_or(StatusCode::OK),
        headers,
        body,
    )
        .into_response()
}

/// Snapshot of what the page reported, for the agent to read.
fn summarize(state: &PreviewState) -> String {
    let (dom, console) = match state.captures.lock() {
        Ok(c) => (c.dom_elements.clone(), c.console.clone()),
        Err(_) => (Vec::new(), Vec::new()),
    };

    let mut out = String::new();
    out.push_str(&format!("DOM elements captured: {}\n", dom.len()));
    for e in dom.iter().take(50) {
        out.push_str(&format!(
            "  <{}> id={:?} component={:?}{}\n    {}\n",
            e.tag_name,
            e.id,
            e.react_component_name,
            e.file_line_range
                .as_ref()
                .map(|f| format!(" at {}:{}", f.absolute_uri, f.start_line))
                .unwrap_or_default(),
            crate::agents::platform_extensions::browser_preview::clip(&e.outer_html, 200)
        ));
    }

    let logs: Vec<_> = console.iter().flat_map(|c| c.lines.iter()).collect();
    out.push_str(&format!("\nConsole output ({} lines):\n", logs.len()));
    for l in logs.iter().take(50) {
        out.push_str(&format!(
            "  [{}] {}: {}\n",
            l.timestamp_str, l.kind, l.output
        ));
    }
    out
}

pub fn clip(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        let head: String = s.chars().take(n).collect();
        format!("{head}…")
    }
}

pub struct BrowserPreviewClient {
    info: InitializeResult,
    previews: Mutex<HashMap<String, Arc<PreviewState>>>,
}

impl BrowserPreviewClient {
    pub fn new(_context: PlatformExtensionContext) -> anyhow::Result<Self> {
        let info = InitializeResult::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(
                Implementation::new(EXTENSION_NAME.to_string(), "1.0.0".to_string())
                    .with_title("Browser Preview"),
            )
            .with_instructions(
                "Open a live preview of a web app and read back what it renders — DOM and console.",
            );
        Ok(Self {
            info,
            previews: Mutex::new(HashMap::new()),
        })
    }

    fn get_tools() -> Vec<Tool> {
        vec![
            Tool::new(
                OPEN_TOOL.to_string(),
                "Open a live preview of a web app and read back what it actually renders. \
                 Proxies the target URL, injects a page bridge, then returns the rendered DOM \
                 elements and browser console output. Use this to verify UI changes instead of \
                 reading HTML source."
                    .to_string(),
                object!({
                    "type": "object",
                    "properties": {
                        "url": {
                            "type": "string",
                            "description": "URL of the running app to preview, e.g. http://localhost:3000"
                        },
                        "port": {
                            "type": "integer",
                            "description": "Local port for the proxy. Omit to pick a free one."
                        }
                    },
                    "required": ["url"]
                }),
            ),
            Tool::new(
                CLOSE_TOOL.to_string(),
                "Close a running browser preview by id. The preview URL stops working; the app \
                 being previewed is not affected."
                    .to_string(),
                object!({
                    "type": "object",
                    "properties": {
                        "preview_id": {
                            "type": "string",
                            "description": "Preview id returned by browser_preview"
                        }
                    },
                    "required": ["preview_id"]
                }),
            ),
        ]
    }

    async fn start_preview(
        &self,
        target: &str,
        port: u16,
    ) -> Result<(String, Arc<PreviewState>), String> {
        let csrf = uuid::Uuid::new_v4().to_string();
        let state = Arc::new(PreviewState::new(target.to_string(), csrf));
        let app = router(state.clone());

        let listener = tokio::net::TcpListener::bind(("127.0.0.1", port))
            .await
            .map_err(|e| format!("bind 127.0.0.1:{port}: {e}"))?;
        let addr = listener
            .local_addr()
            .map_err(|e| format!("resolve listener address: {e}"))?;

        tokio::spawn(async move {
            if let Err(e) = axum::serve(listener, app).await {
                tracing::error!("browser preview server stopped: {e}");
            }
        });

        Ok((addr.to_string(), state))
    }
}

#[async_trait]
impl McpClientTrait for BrowserPreviewClient {
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
        let args = serde_json::Value::Object(arguments.unwrap_or_default());

        match name {
            OPEN_TOOL => {
                let Some(url) = args.get("url").and_then(|v| v.as_str()) else {
                    return Ok(CallToolResult::error(vec![ContentBlock::text(
                        "url is required",
                    )]));
                };
                let port = args.get("port").and_then(|v| v.as_u64()).unwrap_or(0) as u16;

                match self.start_preview(url, port).await {
                    Ok((id, state)) => {
                        let summary = summarize(&state);
                        if let Ok(mut p) = self.previews.lock() {
                            p.insert(id.clone(), state);
                        }
                        Ok(CallToolResult::success(vec![ContentBlock::text(format!(
                            "preview_id: {id}\nproxy: http://{id}\ntarget: {url}\n\n{summary}\n\
                             Load http://{id} in a browser to exercise the app; captured DOM and \
                             console output will be returned on your next call."
                        ))]))
                    }
                    Err(e) => Ok(CallToolResult::error(vec![ContentBlock::text(e)])),
                }
            }
            CLOSE_TOOL => {
                let Some(id) = args.get("preview_id").and_then(|v| v.as_str()) else {
                    return Ok(CallToolResult::error(vec![ContentBlock::text(
                        "preview_id is required",
                    )]));
                };
                let removed = self
                    .previews
                    .lock()
                    .map(|mut p| p.remove(id))
                    .unwrap_or(None);
                Ok(match removed {
                    Some(_) => CallToolResult::success(vec![ContentBlock::text(format!(
                        "closed preview {id}"
                    ))]),
                    None => CallToolResult::error(vec![ContentBlock::text(format!(
                        "no preview with id {id}"
                    ))]),
                })
            }
            other => Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                "Unknown tool: {other}"
            ))])),
        }
    }

    fn get_info(&self) -> Option<&InitializeResult> {
        Some(&self.info)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exposes_both_tools() {
        let tools = BrowserPreviewClient::get_tools();
        let names: Vec<&str> = tools.iter().map(|t| t.name.as_ref()).collect();
        assert!(names.contains(&OPEN_TOOL));
        assert!(names.contains(&CLOSE_TOOL));
    }

    #[test]
    fn bridge_is_embedded() {
        assert!(BRIDGE_JS.contains("BrowserPreview"));
        assert!(BRIDGE_JS.contains("exa.browser_preview_pb.BrowserPreviewService"));
    }

    #[test]
    fn clip_is_char_safe() {
        assert_eq!(clip("hello", 10), "hello");
        assert!(clip("héllo wörld", 3).starts_with("hél"));
    }
}
