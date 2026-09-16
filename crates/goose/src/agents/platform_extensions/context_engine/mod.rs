//! Native context engine.
//!
//! Three capabilities, all running on the session's own provider and model:
//!
//!   * `codebase_retrieval` — search the workspace within a character budget
//!   * `enhance_prompt`      — rewrite a vague request into a specific one
//!   * `rerank_context`      — order context chunks by relevance, drop unrelated
//!
//! This is a platform extension: it runs in the agent process with direct access
//! to the agent context. There is no MCP server, no subprocess, and no stdio
//! boundary.
//!
//! Ported from a decrypted design; the retrieval model was server-side and
//! embedding-based there, so the index here is lexical instead. See
//! `docs/CONTEXT-ENGINE.md` for what is preserved and what diverges.

mod budget;
pub mod enhance;
mod rerank;
pub mod workspace;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

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
use crate::providers::base::Provider;

pub static EXTENSION_NAME: &str = "context_engine";
pub const RETRIEVAL_TOOL: &str = "codebase_retrieval";
pub const ENHANCE_TOOL: &str = "enhance_prompt";
pub const RERANK_TOOL: &str = "rerank_context";

/// Mirrors the original's `maxTrackableFileCount` guard.
const MAX_TRACKABLE_FILES: usize = 50_000;
/// Max chunks returned before ranking.
const MAX_HITS: usize = 40;
/// Max characters per chunk when reranking.
const CHUNK_CHARS: usize = 4000;

pub struct ContextEngineClient {
    info: InitializeResult,
    context: PlatformExtensionContext,
    /// Workspace indexes are built lazily and cached per directory, like the
    /// original's workspace manager.
    workspaces: Mutex<HashMap<PathBuf, Arc<workspace::WorkspaceIndex>>>,
}

impl ContextEngineClient {
    pub fn new(context: PlatformExtensionContext) -> anyhow::Result<Self> {
        let info = InitializeResult::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(
                Implementation::new(EXTENSION_NAME.to_string(), "1.0.0".to_string())
                    .with_title("Context Engine"),
            )
            .with_instructions(
                "Retrieve codebase context within a budget, enhance prompts, and rank context by relevance.",
            );
        Ok(Self {
            info,
            context,
            workspaces: Mutex::new(HashMap::new()),
        })
    }

    /// Resolve the session's provider, exactly as other platform extensions do.
    async fn get_provider(&self) -> Result<Arc<dyn Provider>, String> {
        let extension_manager = self
            .context
            .extension_manager
            .as_ref()
            .and_then(|weak| weak.upgrade())
            .ok_or("Extension manager not available")?;
        let provider_guard = extension_manager.get_provider().lock().await;
        let provider = provider_guard
            .as_ref()
            .ok_or("Provider not available")?
            .clone();
        Ok(provider)
    }

    fn get_tools() -> Vec<Tool> {
        vec![
            Tool::new(
                RETRIEVAL_TOOL.to_string(),
                "Search the workspace for code relevant to a query and return the matching \
                 sections within a character budget. Respects .gitignore, .augmentignore and \
                 .karmxignore. Use this to gather context before editing."
                    .to_string(),
                object!({
                    "type": "object",
                    "properties": {
                        "query": { "type": "string", "description": "What to look for" },
                        "directory": {
                            "type": "string",
                            "description": "Workspace root. Defaults to the current directory."
                        },
                        "budget": {
                            "type": "integer",
                            "description": "Character budget for the returned context. Defaults to 20000."
                        }
                    },
                    "required": ["query"]
                }),
            ),
            Tool::new(
                ENHANCE_TOOL.to_string(),
                "Rewrite a vague or underspecified user request into a clearer, more specific \
                 instruction. Returns the enhanced prompt, or the original if enhancement fails."
                    .to_string(),
                object!({
                    "type": "object",
                    "properties": {
                        "prompt": { "type": "string", "description": "The original prompt to enhance" }
                    },
                    "required": ["prompt"]
                }),
            ),
            Tool::new(
                RERANK_TOOL.to_string(),
                "Order a list of context chunks by relevance to a query, dropping clearly \
                 unrelated ones. Useful for trimming conversation history before it enters the \
                 context window."
                    .to_string(),
                object!({
                    "type": "object",
                    "properties": {
                        "query": { "type": "string", "description": "The current request to rank against" },
                        "chunks": {
                            "type": "array",
                            "description": "Chunks to rank, most recent last",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "text": { "type": "string" },
                                    "timestamp_ms": { "type": "integer" }
                                },
                                "required": ["text"]
                            }
                        }
                    },
                    "required": ["query", "chunks"]
                }),
            ),
        ]
    }

    fn index_for(&self, dir: &str) -> anyhow::Result<Arc<workspace::WorkspaceIndex>> {
        let path = PathBuf::from(dir);
        let key = path.canonicalize().unwrap_or(path);
        if let Ok(cache) = self.workspaces.lock() {
            if let Some(hit) = cache.get(&key) {
                return Ok(hit.clone());
            }
        }
        let idx = Arc::new(workspace::WorkspaceIndex::build(&key, MAX_TRACKABLE_FILES)?);
        if let Ok(mut cache) = self.workspaces.lock() {
            cache.insert(key, idx.clone());
        }
        Ok(idx)
    }
}

/// Walk a JSON value into a `tracing`-friendly short string.
fn brief(v: &serde_json::Value) -> String {
    let s = v.to_string();
    if s.len() <= 120 {
        s
    } else {
        format!("{}…", s.chars().take(120).collect::<String>())
    }
}

#[async_trait]
impl McpClientTrait for ContextEngineClient {
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
        ctx: &ToolCallContext,
        name: &str,
        arguments: Option<JsonObject>,
        _cancellation_token: CancellationToken,
    ) -> Result<CallToolResult, Error> {
        let args = serde_json::Value::Object(arguments.unwrap_or_default());

        match name {
            RETRIEVAL_TOOL => {
                let Some(query) = args.get("query").and_then(|v| v.as_str()) else {
                    return Ok(CallToolResult::error(vec![ContentBlock::text(
                        "query is required",
                    )]));
                };
                let dir = args
                    .get("directory")
                    .and_then(|v| v.as_str())
                    .map(str::to_string)
                    .or_else(|| {
                        std::env::current_dir()
                            .ok()
                            .map(|p| p.to_string_lossy().into_owned())
                    })
                    .unwrap_or_else(|| ".".to_string());
                let budget = args
                    .get("budget")
                    .and_then(|v| v.as_u64())
                    .map(|v| budget::Budget::split(v as usize, 0))
                    .unwrap_or_else(budget::Budget::default_budget);

                let idx = match self.index_for(&dir) {
                    Ok(i) => i,
                    Err(e) => {
                        return Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                            "workspace index failed: {e}"
                        ))]))
                    }
                };

                let hits = idx.search(query, MAX_HITS);
                let text = idx.format_hits(&hits);
                let (out, m) = budget::combine(budget, &text, "");

                Ok(CallToolResult::success(vec![ContentBlock::text(format!(
                    "{out}\n\n[files indexed: {} | hits: {} | truncated: {}]",
                    idx.file_count(),
                    hits.len(),
                    m.codebase_truncated
                ))]))
            }
            ENHANCE_TOOL => {
                let Some(prompt) = args.get("prompt").and_then(|v| v.as_str()) else {
                    return Ok(CallToolResult::error(vec![ContentBlock::text(
                        "prompt is required",
                    )]));
                };
                let provider = match self.get_provider().await {
                    Ok(p) => p,
                    Err(e) => {
                        return Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                            "provider unavailable: {e}"
                        ))]))
                    }
                };
                let model_config =
                    match self.context.model_config_for_session(&ctx.session_id).await {
                        Ok(m) => m,
                        Err(e) => {
                            return Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                                "model config unavailable: {e}"
                            ))]))
                        }
                    };

                let e = enhance::enhance(&provider, &model_config, prompt).await;
                let mut out = format!(
                    "status: {}\nenhanced: {}\n",
                    e.status.as_str(),
                    e.was_enhanced
                );
                if let Some(err) = &e.error_message {
                    out.push_str(&format!("error: {err}\n"));
                }
                out.push('\n');
                out.push_str(&e.prompt);
                Ok(CallToolResult::success(vec![ContentBlock::text(out)]))
            }
            RERANK_TOOL => {
                let Some(query) = args.get("query").and_then(|v| v.as_str()) else {
                    return Ok(CallToolResult::error(vec![ContentBlock::text(
                        "query is required",
                    )]));
                };
                let Some(raw) = args.get("chunks").and_then(|v| v.as_array()) else {
                    return Ok(CallToolResult::error(vec![ContentBlock::text(
                        "chunks must be an array",
                    )]));
                };

                let msgs: Vec<rerank::Message> = raw
                    .iter()
                    .map(|c| rerank::Message {
                        role: "chunk".to_string(),
                        content: c
                            .get("text")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                        timestamp_ms: c.get("timestamp_ms").and_then(|v| v.as_i64()).unwrap_or(0),
                    })
                    .collect();

                let chunks = rerank::chunk_messages(&msgs, CHUNK_CHARS);
                let before = chunks.len();

                let provider = match self.get_provider().await {
                    Ok(p) => p,
                    Err(e) => {
                        return Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                            "provider unavailable: {e}"
                        ))]))
                    }
                };
                let model_config =
                    match self.context.model_config_for_session(&ctx.session_id).await {
                        Ok(m) => m,
                        Err(e) => {
                            return Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                                "model config unavailable: {e}"
                            ))]))
                        }
                    };

                tracing::debug!(args = %brief(&args), "reranking context");
                let ranked = rerank::rerank(&provider, &model_config, query, chunks).await;

                let body = ranked
                    .iter()
                    .map(|c| c.text.as_str())
                    .collect::<Vec<_>>()
                    .join("\n\n---\n\n");

                Ok(CallToolResult::success(vec![ContentBlock::text(format!(
                    "kept {}/{} chunks (ranked)\n\n{}",
                    ranked.len(),
                    before,
                    if body.is_empty() {
                        "(nothing relevant)"
                    } else {
                        &body
                    }
                ))]))
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
    fn exposes_three_tools() {
        let tools = ContextEngineClient::get_tools();
        let names: Vec<&str> = tools.iter().map(|t| t.name.as_ref()).collect();
        assert!(names.contains(&RETRIEVAL_TOOL));
        assert!(names.contains(&ENHANCE_TOOL));
        assert!(names.contains(&RERANK_TOOL));
    }

    #[test]
    fn tool_schemas_declare_required_args() {
        for tool in ContextEngineClient::get_tools() {
            let schema = &tool.input_schema;
            assert!(
                schema.contains_key("properties"),
                "tool {} has no properties",
                tool.name
            );
        }
    }

    #[test]
    fn budget_default_matches_capture() {
        assert_eq!(budget::DEFAULT_TOTAL_BUDGET, 20_000);
    }
}
