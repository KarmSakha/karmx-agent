use crate::config::GooseMode;
use crate::conversation::message::{Message, MessageContent, ToolRequest};
use crate::tool_inspection::{InspectionAction, InspectionResult, ToolInspector};
use anyhow::Result;
use async_trait::async_trait;
use rmcp::model::CallToolRequestParams;
use serde_json::Value;
use std::collections::HashMap;

// Helper struct for internal tracking
#[derive(Debug, Clone)]
struct InternalToolCall {
    name: String,
    parameters: Value,
}

impl InternalToolCall {
    fn matches(&self, other: &InternalToolCall) -> bool {
        self.name == other.name && self.parameters == other.parameters
    }

    fn from_tool_call(tool_call: &CallToolRequestParams) -> Self {
        let name = tool_call.name.to_string();
        let parameters = tool_call
            .arguments
            .as_ref()
            .map(|obj| Value::Object(obj.clone()))
            .unwrap_or(Value::Null);
        Self { name, parameters }
    }
}

#[derive(Debug)]
pub struct RepetitionInspector {
    max_repetitions: Option<u32>,
    last_call: Option<InternalToolCall>,
    repeat_count: u32,
    call_counts: HashMap<String, u32>,
}

impl RepetitionInspector {
    pub fn new(max_repetitions: Option<u32>) -> Self {
        Self {
            max_repetitions,
            last_call: None,
            repeat_count: 0,
            call_counts: HashMap::new(),
        }
    }

    pub fn check_tool_call(&mut self, tool_call: CallToolRequestParams) -> bool {
        let internal_call = InternalToolCall::from_tool_call(&tool_call);
        let total_calls = self
            .call_counts
            .entry(internal_call.name.clone())
            .or_insert(0);
        *total_calls += 1;

        if self.max_repetitions.is_none() {
            self.last_call = Some(internal_call);
            self.repeat_count = 1;
            return true;
        }

        if let Some(last) = &self.last_call {
            if last.matches(&internal_call) {
                self.repeat_count += 1;
                if self.repeat_count > self.max_repetitions.unwrap() {
                    return false;
                }
            } else {
                self.repeat_count = 1;
            }
        } else {
            self.repeat_count = 1;
        }

        self.last_call = Some(internal_call);
        true
    }

    pub fn reset(&mut self) {
        self.last_call = None;
        self.repeat_count = 0;
        self.call_counts.clear();
    }
}

#[async_trait]
impl ToolInspector for RepetitionInspector {
    fn name(&self) -> &'static str {
        "repetition"
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    async fn inspect(
        &self,
        _session_id: &str,
        tool_requests: &[ToolRequest],
        messages: &[Message],
        _goose_mode: GooseMode,
    ) -> Result<Vec<InspectionResult>> {
        let Some(max_repetitions) = self.max_repetitions else {
            return Ok(Vec::new());
        };

        // The inspector's own counters never advance — inspect sees &self —
        // so the streak is reconstructed from history instead: the run of
        // identical tool calls in this turn, extended by the pending batch.
        let mut streak_call: Option<InternalToolCall> = None;
        let mut streak = 0u32;
        let mut track = |tool_call: &CallToolRequestParams| -> u32 {
            let call = InternalToolCall::from_tool_call(tool_call);
            match &streak_call {
                Some(last) if last.matches(&call) => streak += 1,
                _ => streak = 1,
            }
            streak_call = Some(call);
            streak
        };

        let history_start = messages
            .iter()
            .rposition(|message| {
                message.role == rmcp::model::Role::User && message.is_user_visible()
            })
            .map(|i| i + 1)
            .unwrap_or(0);
        for message in &messages[history_start..] {
            for content in &message.content {
                if let MessageContent::ToolRequest(request) = content {
                    if let Ok(tool_call) = &request.tool_call {
                        track(tool_call);
                    }
                }
            }
        }

        let mut results = Vec::new();
        for tool_request in tool_requests {
            if let Ok(tool_call) = &tool_request.tool_call {
                let count = track(tool_call);
                if count > max_repetitions {
                    results.push(InspectionResult {
                        tool_request_id: tool_request.id.clone(),
                        action: InspectionAction::Deny,
                        reason: format!(
                            "Tool '{}' has repeated the identical call {} times this turn — \
                             stop retrying and explain the blocker or change approach",
                            tool_call.name, count
                        ),
                        confidence: 1.0,
                        inspector_name: "repetition".to_string(),
                        finding_id: Some("REP-001".to_string()),
                    });
                }
            }
        }

        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmcp::object;

    fn call(name: &str, args: serde_json::Value) -> CallToolRequestParams {
        CallToolRequestParams::new(name.to_string())
            .with_arguments(args.as_object().unwrap().clone())
    }

    fn request(id: &str, name: &str, args: serde_json::Value) -> ToolRequest {
        ToolRequest {
            id: id.to_string(),
            tool_call: Ok(call(name, args)),
            metadata: None,
            tool_meta: None,
        }
    }

    fn shot_args() -> serde_json::Value {
        serde_json::json!({"actions": [{"action_type": "screenshot"}]})
    }

    #[tokio::test]
    async fn inspect_denies_identical_call_streak_from_history() {
        let inspector = RepetitionInspector::new(Some(3));
        let mut turn = Message::assistant();
        for i in 0..3 {
            turn =
                turn.with_tool_request(format!("r{i}"), Ok(call("computer_control", shot_args())));
        }
        let messages = vec![Message::user().with_text("check the page"), turn];

        // 3 identical in history; the 4th pending is denied
        let pending = vec![request("p0", "computer_control", shot_args())];
        let results = inspector
            .inspect("s", &pending, &messages, GooseMode::Auto)
            .await
            .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].tool_request_id, "p0");
        assert_eq!(results[0].action, InspectionAction::Deny);
    }

    #[tokio::test]
    async fn streak_resets_when_args_change() {
        let inspector = RepetitionInspector::new(Some(2));
        let turn = Message::assistant()
            .with_tool_request(
                "r0",
                Ok(call("shell", serde_json::json!({"command": "ls"}))),
            )
            .with_tool_request(
                "r1",
                Ok(call("shell", serde_json::json!({"command": "pwd"}))),
            );
        let messages = vec![Message::user().with_text("work"), turn];

        // "ls" appeared earlier but not consecutively — this is a fresh run
        let pending = vec![request("p0", "shell", serde_json::json!({"command": "ls"}))];
        let results = inspector
            .inspect("s", &pending, &messages, GooseMode::Auto)
            .await
            .unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn streak_resets_at_each_turn() {
        let inspector = RepetitionInspector::new(Some(2));
        let turn = Message::assistant()
            .with_tool_request(
                "r0",
                Ok(call("shell", serde_json::json!({"command": "ls"}))),
            )
            .with_tool_request(
                "r1",
                Ok(call("shell", serde_json::json!({"command": "ls"}))),
            );
        let messages = vec![
            Message::user().with_text("first turn"),
            turn,
            Message::user().with_text("second turn"),
        ];

        // same call is fine again in the new turn — only the current turn counts
        let pending = vec![request("p0", "shell", serde_json::json!({"command": "ls"}))];
        let results = inspector
            .inspect("s", &pending, &messages, GooseMode::Auto)
            .await
            .unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn no_limit_allows_everything() {
        let inspector = RepetitionInspector::new(None);
        let pending = vec![
            request("p0", "shell", serde_json::json!({"command": "ls"})),
            request("p1", "shell", serde_json::json!({"command": "ls"})),
            request("p2", "shell", serde_json::json!({"command": "ls"})),
        ];
        let results = inspector
            .inspect("s", &pending, &[], GooseMode::Auto)
            .await
            .unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn object_macro_import_sanity() {
        let _ = object!({"a": 1});
    }
}
