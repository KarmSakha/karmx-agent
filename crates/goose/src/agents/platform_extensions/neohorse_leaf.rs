use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::sync::Arc;

use crate::agents::extension::PlatformExtensionContext;
use crate::agents::mcp_client::{Error, McpClientTrait};
use crate::agents::platform_extensions::developer::edit::{
    resolve_path, EditTools, FileEditParams, FileWriteParams,
};
use crate::agents::platform_extensions::developer::shell::{ShellOutput, ShellParams, ShellTool};
use crate::agents::ToolCallContext;
use anyhow::Result;
use async_trait::async_trait;
use globset::Glob;
use ignore::WalkBuilder;
use rmcp::model::{
    CallToolResult, ContentBlock, Implementation, InitializeResult, JsonObject, ListToolsResult,
    ServerCapabilities, Tool, ToolAnnotations,
};
use schemars::{schema_for, JsonSchema};
use serde::Deserialize;
use serde_json::Value;
use tokio_util::sync::CancellationToken;

pub const EXTENSION_NAME: &str = "neohorse_leaf";

const MAX_READ_BYTES: usize = 64 * 1024;
const DEFAULT_READ_LINES: u32 = 200;
const MAX_READ_LINES: u32 = 1_000;
const DEFAULT_GLOB_RESULTS: u32 = 100;
const MAX_GLOB_RESULTS: u32 = 200;
const MAX_GLOB_ENTRIES: usize = 50_000;

#[derive(Debug, Deserialize, JsonSchema)]
struct ReadParams {
    path: String,
    #[schemars(range(min = 1))]
    line: Option<u32>,
    #[schemars(range(min = 1, max = 1000))]
    limit: Option<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct GlobParams {
    pattern: String,
    path: Option<String>,
    #[schemars(range(min = 1, max = 200))]
    limit: Option<u32>,
}

pub struct NeoHorseLeafClient {
    info: InitializeResult,
    shell_tool: Arc<ShellTool>,
    edit_tools: Arc<EditTools>,
}

impl NeoHorseLeafClient {
    pub fn new(context: PlatformExtensionContext) -> Result<Self> {
        let info = InitializeResult::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(
                Implementation::new(EXTENSION_NAME, "1.0.0").with_title("NeoHorse Leaf"),
            )
            .with_instructions(
                "Use read and glob to inspect the repository, then write or make a unique edit. \
                 Use bash to run checks. Tool calls in a response are executed in order. \
                 A normal answer without tool calls ends the task.",
            );

        Ok(Self {
            info,
            shell_tool: Arc::new(ShellTool::new(context.use_login_shell_path)?),
            edit_tools: Arc::new(EditTools::new()),
        })
    }

    fn schema<T: JsonSchema>() -> JsonObject {
        serde_json::to_value(schema_for!(T))
            .expect("schema serialization should succeed")
            .as_object()
            .expect("schema should serialize to an object")
            .clone()
    }

    fn parse_args<T: serde::de::DeserializeOwned>(
        arguments: Option<JsonObject>,
    ) -> std::result::Result<T, String> {
        let value = arguments
            .map(Value::Object)
            .ok_or_else(|| "Missing arguments".to_string())?;
        serde_json::from_value(value).map_err(|error| format!("Invalid arguments: {error}"))
    }

    pub fn get_tools() -> Vec<Tool> {
        vec![
            Tool::new(
                "read",
                "Read a text file. Paths are relative to the task directory unless absolute. \
                 Reads are capped at 64 KiB and 1,000 lines.",
                Self::schema::<ReadParams>(),
            )
            .annotate(ToolAnnotations::from_raw(
                Some("Read".to_string()),
                Some(true),
                Some(false),
                Some(false),
                Some(false),
            )),
            Tool::new(
                "write",
                "Create a file or replace its contents. Paths are relative to the task directory \
                 unless absolute.",
                Self::schema::<FileWriteParams>(),
            )
            .annotate(ToolAnnotations::from_raw(
                Some("Write".to_string()),
                Some(false),
                Some(true),
                Some(false),
                Some(false),
            )),
            Tool::new(
                "edit",
                "Replace one unique exact text match in a file. Use write to create or overwrite.",
                Self::schema::<FileEditParams>(),
            )
            .annotate(ToolAnnotations::from_raw(
                Some("Edit".to_string()),
                Some(false),
                Some(true),
                Some(false),
                Some(false),
            )),
            Tool::new(
                "glob",
                "Find paths matching a glob pattern, such as **/*.rs. Respects repository ignore \
                 files and returns at most 200 sorted paths.",
                Self::schema::<GlobParams>(),
            )
            .annotate(ToolAnnotations::from_raw(
                Some("Glob".to_string()),
                Some(true),
                Some(false),
                Some(true),
                Some(false),
            )),
            Tool::new(
                "bash",
                "Run a bash command in the task directory. Pass timeout_secs to cap execution time.",
                Self::schema::<ShellParams>(),
            )
            .with_output_schema::<ShellOutput>()
            .annotate(ToolAnnotations::from_raw(
                Some("Bash".to_string()),
                Some(false),
                Some(true),
                Some(false),
                Some(true),
            )),
        ]
    }

    fn read(&self, params: ReadParams, working_dir: Option<&Path>) -> CallToolResult {
        let path = resolve_path(&params.path, working_dir);
        let mut bytes = Vec::new();
        match File::open(&path).and_then(|file| {
            file.take((MAX_READ_BYTES + 1) as u64)
                .read_to_end(&mut bytes)
        }) {
            Ok(_) => {}
            Err(error) => {
                return CallToolResult::error(vec![ContentBlock::text(format!(
                    "Failed to read {}: {error}",
                    params.path
                ))]);
            }
        }

        let byte_truncated = bytes.len() > MAX_READ_BYTES;
        bytes.truncate(MAX_READ_BYTES);
        let content = String::from_utf8_lossy(&bytes);
        let lines: Vec<&str> = content.split_inclusive('\n').collect();
        let start = params.line.unwrap_or(1).saturating_sub(1) as usize;
        let limit = params
            .limit
            .unwrap_or(DEFAULT_READ_LINES)
            .clamp(1, MAX_READ_LINES) as usize;
        let selected = lines
            .iter()
            .skip(start)
            .take(limit)
            .copied()
            .collect::<String>();
        let line_truncated = start.saturating_add(limit) < lines.len();
        let mut output = selected;
        if line_truncated || byte_truncated {
            output.push_str("\n[read output truncated]");
        }
        CallToolResult::success(vec![ContentBlock::text(output)])
    }

    fn glob(&self, params: GlobParams, working_dir: Option<&Path>) -> CallToolResult {
        if params.pattern.trim().is_empty() {
            return CallToolResult::error(vec![ContentBlock::text("Pattern cannot be empty")]);
        }
        if params.pattern.len() > 512 {
            return CallToolResult::error(vec![ContentBlock::text("Pattern exceeds 512 bytes")]);
        }
        let pattern = match Glob::new(&params.pattern) {
            Ok(pattern) => pattern.compile_matcher(),
            Err(error) => {
                return CallToolResult::error(vec![ContentBlock::text(format!(
                    "Invalid glob pattern: {error}"
                ))]);
            }
        };
        let root = resolve_path(params.path.as_deref().unwrap_or("."), working_dir);
        if !root.is_dir() {
            return CallToolResult::error(vec![ContentBlock::text(format!(
                "Directory does not exist: {}",
                root.display()
            ))]);
        }

        let limit = params
            .limit
            .unwrap_or(DEFAULT_GLOB_RESULTS)
            .clamp(1, MAX_GLOB_RESULTS) as usize;
        let mut walker = WalkBuilder::new(&root);
        walker
            .require_git(false)
            .git_ignore(true)
            .git_exclude(true)
            .git_global(false)
            .ignore(true)
            .hidden(false)
            .sort_by_file_name(|left, right| left.cmp(right));

        let mut matches = Vec::new();
        let mut scanned = 0;
        for entry in walker.build() {
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) => {
                    return CallToolResult::error(vec![ContentBlock::text(format!(
                        "Failed while searching {}: {error}",
                        root.display()
                    ))]);
                }
            };
            if entry.path() == root {
                continue;
            }
            scanned += 1;
            if scanned > MAX_GLOB_ENTRIES {
                break;
            }
            let Ok(relative) = entry.path().strip_prefix(&root) else {
                continue;
            };
            if pattern.is_match(relative) {
                matches.push(relative.to_string_lossy().into_owned());
                if matches.len() >= limit {
                    break;
                }
            }
        }
        matches.sort();
        let mut output = matches.join("\n");
        if scanned > MAX_GLOB_ENTRIES {
            output.push_str("\n[search stopped after 50,000 paths]");
        }
        if output.is_empty() {
            output.push_str("(no matches)");
        }
        CallToolResult::success(vec![ContentBlock::text(output)])
    }
}

#[async_trait]
impl McpClientTrait for NeoHorseLeafClient {
    async fn list_tools(
        &self,
        _session_id: &str,
        _next_cursor: Option<String>,
        _cancellation_token: CancellationToken,
    ) -> Result<ListToolsResult, Error> {
        Ok(ListToolsResult {
            tools: Self::get_tools(),
            next_cursor: None,
            meta: None,
            ..Default::default()
        })
    }

    async fn call_tool(
        &self,
        context: &ToolCallContext,
        name: &str,
        arguments: Option<JsonObject>,
        cancellation_token: CancellationToken,
    ) -> Result<CallToolResult, Error> {
        let working_dir = context.working_dir.as_deref();
        match name {
            "read" => match Self::parse_args::<ReadParams>(arguments) {
                Ok(params) => Ok(self.read(params, working_dir)),
                Err(error) => Ok(CallToolResult::error(vec![ContentBlock::text(error)])),
            },
            "write" => match Self::parse_args::<FileWriteParams>(arguments) {
                Ok(params) => Ok(self.edit_tools.file_write_with_cwd(params, working_dir)),
                Err(error) => Ok(CallToolResult::error(vec![ContentBlock::text(error)])),
            },
            "edit" => match Self::parse_args::<FileEditParams>(arguments) {
                Ok(params) => Ok(self.edit_tools.file_edit_with_cwd(params, working_dir)),
                Err(error) => Ok(CallToolResult::error(vec![ContentBlock::text(error)])),
            },
            "glob" => match Self::parse_args::<GlobParams>(arguments) {
                Ok(params) => Ok(self.glob(params, working_dir)),
                Err(error) => Ok(CallToolResult::error(vec![ContentBlock::text(error)])),
            },
            "bash" => match Self::parse_args::<ShellParams>(arguments) {
                Ok(params) => Ok(self
                    .shell_tool
                    .shell_with_cwd_and_emitter(
                        params,
                        working_dir,
                        Some(&context.session_id),
                        context.notification_emitter().cloned(),
                        cancellation_token,
                    )
                    .await),
                Err(error) => Ok(ShellTool::error_result(&error, None)),
            },
            _ => Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                "Unknown tool: {name}"
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
    use crate::session::SessionManager;
    use rmcp::object;
    use std::fs;

    fn context(working_dir: &Path) -> ToolCallContext {
        ToolCallContext::new(
            "leaf-test".to_string(),
            Some(working_dir.to_path_buf()),
            None,
        )
    }

    fn extension(sessions: &Path) -> NeoHorseLeafClient {
        NeoHorseLeafClient::new(PlatformExtensionContext {
            extension_manager: None,
            session_manager: Arc::new(SessionManager::new(sessions.to_path_buf())),
            scheduler: None,
            session: None,
            use_login_shell_path: false,
        })
        .unwrap()
    }

    fn text(result: &CallToolResult) -> &str {
        match &result.content[0] {
            ContentBlock::Text(content) => &content.text,
            _ => panic!("expected text"),
        }
    }

    #[test]
    fn exposes_only_the_typed_leaf_tools_in_order() {
        let tools = NeoHorseLeafClient::get_tools();
        let names: Vec<_> = tools.iter().map(|tool| tool.name.as_ref()).collect();
        assert_eq!(names, ["read", "write", "edit", "glob", "bash"]);
        assert!(tools
            .iter()
            .all(|tool| tool.input_schema.get("type").is_some()));
        assert!(tools.iter().all(|tool| !tool.name.contains("__")));
    }

    #[tokio::test]
    async fn read_is_capped_and_write_edit_use_the_task_directory() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("task");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("notes.txt"), "first line\nsecond line\n").unwrap();
        let client = extension(&temp.path().join("sessions"));
        let context = context(&root);

        let read = client
            .call_tool(
                &context,
                "read",
                Some(object!({"path":"notes.txt", "line":1, "limit":1})),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert_eq!(text(&read), "first line\n\n[read output truncated]");

        let write = client
            .call_tool(
                &context,
                "write",
                Some(object!({"path":"new.txt", "content":"before"})),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert_eq!(write.is_error, Some(false));

        let edit = client
            .call_tool(
                &context,
                "edit",
                Some(object!({"path":"new.txt", "before":"before", "after":"after"})),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert_eq!(edit.is_error, Some(false));
        assert_eq!(fs::read_to_string(root.join("new.txt")).unwrap(), "after");
    }

    #[tokio::test]
    async fn glob_uses_patterns_and_rejects_invalid_patterns() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("task");
        fs::create_dir_all(root.join("src/nested")).unwrap();
        fs::write(root.join("src/nested/main.rs"), "fn main() {}\n").unwrap();
        let client = extension(&temp.path().join("sessions"));
        let context = context(&root);

        let result = client
            .call_tool(
                &context,
                "glob",
                Some(object!({"pattern":"**/*.rs"})),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert_eq!(text(&result), "src/nested/main.rs");

        let invalid = client
            .call_tool(
                &context,
                "glob",
                Some(object!({"pattern":"["})),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert_eq!(invalid.is_error, Some(true));
    }

    #[cfg(not(windows))]
    #[tokio::test]
    async fn bash_runs_from_the_task_directory_and_honors_cancellation() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("task");
        fs::create_dir_all(&root).unwrap();
        let client = extension(&temp.path().join("sessions"));
        let context = context(&root);
        let result = client
            .call_tool(
                &context,
                "bash",
                Some(object!({"command":"pwd"})),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert!(text(&result).contains(root.to_string_lossy().as_ref()));
    }
}
