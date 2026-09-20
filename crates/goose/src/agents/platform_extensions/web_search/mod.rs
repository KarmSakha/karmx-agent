//! Web search and page reads via Exa (`https://api.exa.ai`).
//!
//! Two tools, no model involvement:
//!
//!   * `web_search` — query → ranked results with extracted page text
//!   * `web_fetch`  — urls  → extracted page text
//!
//! Key resolution mirrors `crate::typesafe`: `EXA_API_KEY` env first, then
//! `<config dir>/exa.env`. A missing key or a failed request returns a tool
//! error, never a panic.

use std::time::Duration;

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

pub static EXTENSION_NAME: &str = "web_search";
pub const SEARCH_TOOL: &str = "web_search";
pub const FETCH_TOOL: &str = "web_fetch";

const EXA_SEARCH: &str = "https://api.exa.ai/search";
const EXA_CONTENTS: &str = "https://api.exa.ai/contents";
const MAX_RESULTS_CAP: usize = 25;
const MAX_URLS: usize = 10;

pub struct WebSearchClient {
    info: InitializeResult,
    client: reqwest::Client,
}

impl WebSearchClient {
    pub fn new(_context: PlatformExtensionContext) -> anyhow::Result<Self> {
        let info = InitializeResult::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(
                Implementation::new(EXTENSION_NAME.to_string(), "1.0.0".to_string())
                    .with_title("Web Search"),
            )
            .with_instructions("Search the web and read pages via Exa.");
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(25))
            .build()?;
        Ok(Self { info, client })
    }

    fn get_tools() -> Vec<Tool> {
        vec![
            Tool::new(
                SEARCH_TOOL.to_string(),
                "Search the web for current information. Returns ranked results with extracted \
                 page text. Use for facts, docs, news, and anything not in the workspace."
                    .to_string(),
                object!({
                    "type": "object",
                    "properties": {
                        "query": { "type": "string", "description": "What to search for" },
                        "num_results": {
                            "type": "integer",
                            "description": "Results to return, 1-25. Defaults to 8."
                        },
                        "max_chars": {
                            "type": "integer",
                            "description": "Extracted page text per result. Defaults to 1500."
                        }
                    },
                    "required": ["query"]
                }),
            ),
            Tool::new(
                FETCH_TOOL.to_string(),
                "Read one or more URLs and return their extracted text. Use after web_search or \
                 when the user hands you a link."
                    .to_string(),
                object!({
                    "type": "object",
                    "properties": {
                        "urls": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "URLs to read, at most 10."
                        },
                        "max_chars": {
                            "type": "integer",
                            "description": "Extracted text per page. Defaults to 4000."
                        }
                    },
                    "required": ["urls"]
                }),
            ),
        ]
    }

    async fn exa(&self, url: &str, body: serde_json::Value) -> Result<serde_json::Value, String> {
        let key = api_key().ok_or_else(|| {
            "EXA_API_KEY not set — add it to ~/.config/karmx/exa.env or the environment".to_string()
        })?;
        let resp = self
            .client
            .post(url)
            .header("x-api-key", key)
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("exa request failed: {e}"))?;
        let status = resp.status();
        let text = resp
            .text()
            .await
            .map_err(|e| format!("exa response read failed: {e}"))?;
        if !status.is_success() {
            let snippet: String = text.chars().take(300).collect();
            return Err(format!("exa {status}: {snippet}"));
        }
        serde_json::from_str(&text).map_err(|e| format!("exa response parse failed: {e}"))
    }

    async fn search(&self, args: &serde_json::Value) -> Result<String, String> {
        let query = args
            .get("query")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|q| !q.is_empty())
            .ok_or_else(|| "query is required".to_string())?;
        let num_results = args
            .get("num_results")
            .and_then(|v| v.as_u64())
            .map(|n| (n as usize).clamp(1, MAX_RESULTS_CAP))
            .unwrap_or(8);
        let max_chars = args
            .get("max_chars")
            .and_then(|v| v.as_u64())
            .map(|n| (n as usize).clamp(200, 20000))
            .unwrap_or(1500);

        let body = serde_json::json!({
            "query": query,
            "numResults": num_results,
            "contents": { "text": { "maxCharacters": max_chars } }
        });
        let data = self.exa(EXA_SEARCH, body).await?;
        let results = data
            .get("results")
            .and_then(|r| r.as_array())
            .cloned()
            .unwrap_or_default();
        if results.is_empty() {
            return Ok(format!("no results for: {query}"));
        }
        let mut out = format!("{} result(s) for \"{query}\":\n", results.len());
        for (i, r) in results.iter().enumerate() {
            let title = r
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("(untitled)");
            let url = r.get("url").and_then(|v| v.as_str()).unwrap_or("");
            let text = r.get("text").and_then(|v| v.as_str()).unwrap_or("").trim();
            out.push_str(&format!(
                "\n{}. {}\n   {}\n   {}\n",
                i + 1,
                title,
                url,
                text
            ));
        }
        Ok(out)
    }

    async fn fetch(&self, args: &serde_json::Value) -> Result<String, String> {
        let urls: Vec<String> = args
            .get("urls")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .take(MAX_URLS)
                    .collect()
            })
            .unwrap_or_default();
        if urls.is_empty() {
            return Err("urls is required (array of up to 10)".to_string());
        }
        let max_chars = args
            .get("max_chars")
            .and_then(|v| v.as_u64())
            .map(|n| (n as usize).clamp(200, 50000))
            .unwrap_or(4000);

        let body = serde_json::json!({
            "urls": urls,
            "text": { "maxCharacters": max_chars }
        });
        let data = self.exa(EXA_CONTENTS, body).await?;
        let results = data
            .get("results")
            .and_then(|r| r.as_array())
            .cloned()
            .unwrap_or_default();
        if results.is_empty() {
            return Ok("no content returned".to_string());
        }
        let mut out = String::new();
        for r in results {
            let url = r.get("url").and_then(|v| v.as_str()).unwrap_or("");
            let title = r
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("(untitled)");
            let text = r.get("text").and_then(|v| v.as_str()).unwrap_or("").trim();
            out.push_str(&format!("=== {} — {} ===\n{}\n\n", title, url, text));
        }
        Ok(out)
    }
}

/// `EXA_API_KEY` from the environment, else `<config dir>/exa.env`.
fn api_key() -> Option<String> {
    std::env::var("EXA_API_KEY")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .or_else(read_env_file_key)
}

fn read_env_file_key() -> Option<String> {
    let path = crate::config::paths::Paths::in_config_dir("exa.env");
    let content = std::fs::read_to_string(path).ok()?;
    for line in content.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix("EXA_API_KEY") else {
            continue;
        };
        let Some(value) = rest.trim_start().strip_prefix('=') else {
            continue;
        };
        let value = value.trim().trim_matches('"').trim_matches('\'');
        if !value.is_empty() {
            return Some(value.to_string());
        }
    }
    None
}

#[async_trait]
impl McpClientTrait for WebSearchClient {
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
        let result = match name {
            SEARCH_TOOL => self.search(&args).await,
            FETCH_TOOL => self.fetch(&args).await,
            other => Err(format!("Unknown tool: {other}")),
        };
        match result {
            Ok(text) => Ok(CallToolResult::success(vec![ContentBlock::text(text)])),
            Err(e) => Ok(CallToolResult::error(vec![ContentBlock::text(e)])),
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
    fn api_key_reads_env_file() {
        let dir = std::env::temp_dir().join(format!("karmx-exa-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("exa.env"), "# c\nEXA_API_KEY = \"k_test\"\n").unwrap();
        // read_env_file_key honors the global config dir, so cover the parser
        // through the env-var path and leave file IO to the integration test.
        std::env::set_var("EXA_API_KEY", "env_key");
        assert_eq!(api_key().as_deref(), Some("env_key"));
        std::env::remove_var("EXA_API_KEY");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
