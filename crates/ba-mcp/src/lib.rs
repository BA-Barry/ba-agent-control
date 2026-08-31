//! Model Context Protocol surface for BA Agent.
//!
//! MCP requests are deliberately routed through [`ToolBackend`]. An agent never receives a
//! database handle and cannot bypass domain policy by writing SQLite directly.

use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncWrite, AsyncWriteExt, BufReader};

/// MCP protocol version implemented by the local server.
pub const PROTOCOL_VERSION: &str = "2025-06-18";

/// A structured MCP tool visible to agents.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<ToolAnnotations>,
}

/// Read/write hints exposed to compatible MCP clients.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ToolAnnotations {
    pub read_only_hint: bool,
    pub destructive_hint: bool,
    pub idempotent_hint: bool,
}

/// Domain-service adapter used by the protocol layer.
#[async_trait]
pub trait ToolBackend: Send + Sync + 'static {
    async fn call(&self, tool: &str, arguments: Value) -> Result<Value, ToolError>;
}

/// A policy or validation error returned by a tool backend.
#[derive(Debug, Clone, Serialize, Deserialize, thiserror::Error)]
#[error("{message}")]
pub struct ToolError {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl ToolError {
    #[must_use]
    pub fn invalid(message: impl Into<String>) -> Self {
        Self {
            code: "invalid_arguments".into(),
            message: message.into(),
            data: None,
        }
    }

    #[must_use]
    pub fn internal(message: impl Into<String>) -> Self {
        Self {
            code: "internal_error".into(),
            message: message.into(),
            data: None,
        }
    }
}

/// Newline-delimited JSON-RPC MCP server.
#[derive(Debug)]
pub struct McpServer<B> {
    backend: Arc<B>,
    tools: Vec<ToolDefinition>,
}

impl<B: ToolBackend> McpServer<B> {
    #[must_use]
    pub fn new(backend: Arc<B>) -> Self {
        Self {
            backend,
            tools: default_tools(),
        }
    }

    #[must_use]
    pub fn tools(&self) -> &[ToolDefinition] {
        &self.tools
    }

    /// Serve MCP over stdio-compatible streams. One JSON-RPC object is accepted per line.
    ///
    /// # Errors
    ///
    /// Returns an error when input/output fails or a response cannot be serialized.
    pub async fn serve<R, W>(&self, reader: R, mut writer: W) -> anyhow::Result<()>
    where
        R: AsyncBufRead + Unpin,
        W: AsyncWrite + Unpin,
    {
        let mut lines = reader.lines();
        while let Some(line) = lines.next_line().await? {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let response = match serde_json::from_str::<JsonRpcRequest>(trimmed) {
                Ok(request) => self.handle(request).await,
                Err(error) => Some(JsonRpcResponse::error(
                    Value::Null,
                    -32700,
                    "Parse error",
                    Some(json!({"detail": error.to_string()})),
                )),
            };
            if let Some(response) = response {
                let mut bytes = serde_json::to_vec(&response)?;
                bytes.push(b'\n');
                writer.write_all(&bytes).await?;
                writer.flush().await?;
            }
        }
        Ok(())
    }

    async fn handle(&self, request: JsonRpcRequest) -> Option<JsonRpcResponse> {
        let id = request.id?;
        let params = request.params.unwrap_or(Value::Null);
        let response = match request.method.as_str() {
            "initialize" => JsonRpcResponse::success(
                id,
                json!({
                    "protocolVersion": PROTOCOL_VERSION,
                    "capabilities": {"tools": {"listChanged": false}},
                    "serverInfo": {"name": "ba-agent-control", "version": env!("CARGO_PKG_VERSION")}
                }),
            ),
            "ping" => JsonRpcResponse::success(id, json!({})),
            "tools/list" => JsonRpcResponse::success(id, json!({"tools": self.tools})),
            "tools/call" => self.handle_tool_call(id, params).await,
            _ => JsonRpcResponse::error(id, -32601, "Method not found", None),
        };
        Some(response)
    }

    async fn handle_tool_call(&self, id: Value, params: Value) -> JsonRpcResponse {
        let Some(name) = params.get("name").and_then(Value::as_str) else {
            return JsonRpcResponse::error(id, -32602, "Missing tool name", None);
        };
        if !self.tools.iter().any(|tool| tool.name == name) {
            return JsonRpcResponse::error(id, -32602, "Unknown tool", None);
        }
        let arguments = params
            .get("arguments")
            .cloned()
            .unwrap_or_else(|| json!({}));
        match self.backend.call(name, arguments).await {
            Ok(value) => JsonRpcResponse::success(
                id,
                json!({
                    "content": [{"type": "text", "text": value.to_string()}],
                    "structuredContent": value,
                    "isError": false
                }),
            ),
            Err(error) => JsonRpcResponse::success(
                id,
                json!({
                    "content": [{"type": "text", "text": error.message}],
                    "structuredContent": {"code": error.code, "data": error.data},
                    "isError": true
                }),
            ),
        }
    }
}

#[derive(Debug, Deserialize)]
struct JsonRpcRequest {
    #[allow(dead_code)]
    jsonrpc: Option<String>,
    id: Option<Value>,
    method: String,
    params: Option<Value>,
}

#[derive(Debug, Serialize)]
struct JsonRpcResponse {
    jsonrpc: &'static str,
    id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
}

impl JsonRpcResponse {
    fn success(id: Value, result: Value) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            result: Some(result),
            error: None,
        }
    }

    fn error(id: Value, code: i64, message: impl Into<String>, data: Option<Value>) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            result: None,
            error: Some(JsonRpcError {
                code,
                message: message.into(),
                data,
            }),
        }
    }
}

#[derive(Debug, Serialize)]
struct JsonRpcError {
    code: i64,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<Value>,
}

fn object_schema(properties: BTreeMap<&str, Value>, required: &[&str]) -> Value {
    let properties = properties
        .into_iter()
        .map(|(key, value)| (key.to_owned(), value))
        .collect::<serde_json::Map<_, _>>();
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": properties,
        "required": required,
    })
}

fn field(description: &str) -> Value {
    json!({"type": "string", "description": description})
}

fn definition(
    name: &str,
    description: &str,
    properties: BTreeMap<&str, Value>,
    required: &[&str],
    read_only: bool,
) -> ToolDefinition {
    ToolDefinition {
        name: name.into(),
        description: description.into(),
        input_schema: object_schema(properties, required),
        annotations: Some(ToolAnnotations {
            read_only_hint: read_only,
            destructive_hint: false,
            idempotent_hint: read_only,
        }),
    }
}

/// Stable MCP tool catalog. Mutating tools describe intent and are still checked by domain policy.
#[must_use]
#[allow(
    clippy::too_many_lines,
    reason = "the declarative MCP catalog is clearer when kept in protocol order"
)]
pub fn default_tools() -> Vec<ToolDefinition> {
    vec![
        definition(
            "project.list",
            "List registered local projects.",
            BTreeMap::new(),
            &[],
            true,
        ),
        definition(
            "project.get",
            "Read one project and its health summary.",
            BTreeMap::from([("project_id", field("Project ULID"))]),
            &["project_id"],
            true,
        ),
        definition(
            "project.import",
            "Initialize BA Agent metadata and register an existing local Git repository root without replacing user configuration.",
            BTreeMap::from([
                ("path", field("Absolute local Git repository root")),
                ("name", field("Optional display name")),
                ("description", field("Optional project description")),
                ("language", field("Optional zh-CN or en project language")),
            ]),
            &["path"],
            false,
        ),
        definition(
            "project.create",
            "Create a missing or empty local directory, initialize Git and BA Agent metadata, and register the project. No remote repository is created.",
            BTreeMap::from([
                ("path", field("Absolute new local project path")),
                ("name", field("Optional display name")),
                ("description", field("Optional project description")),
                ("language", field("Optional zh-CN or en project language")),
            ]),
            &["path"],
            false,
        ),
        definition(
            "workstream.list",
            "List workstreams for a project.",
            BTreeMap::from([("project_id", field("Project ULID"))]),
            &["project_id"],
            true,
        ),
        definition(
            "workflow-template.list",
            "List six built-in workflow templates plus valid project-local TOML overrides and diagnostics.",
            BTreeMap::from([("project_id", field("Project ULID"))]),
            &["project_id"],
            true,
        ),
        definition(
            "workflow-template.materialize",
            "Copy one built-in template into the project's .ba-agent/workflows directory for explicit editing without overwriting an existing file.",
            BTreeMap::from([
                ("project_id", field("Project ULID")),
                ("template_id", field("Built-in workflow template id")),
            ]),
            &["project_id", "template_id"],
            false,
        ),
        definition(
            "workflow.instantiate",
            "Atomically create a workstream and assigned dependency graph from a validated workflow template.",
            BTreeMap::from([
                ("project_id", field("Project ULID")),
                ("template_id", field("Workflow template id")),
                ("workstream_name", field("Optional instance name")),
                (
                    "milestone_id",
                    field("Optional project-level milestone ULID"),
                ),
                (
                    "write_scope",
                    json!({"type":"array","items":{"type":"string"},"description":"Repository-relative scope substituted into write tasks"}),
                ),
            ]),
            &["project_id", "template_id"],
            false,
        ),
        definition(
            "review-loop.start",
            "Create a durable bounded implement, test, review, repair, and confirmation loop. Repairs are generated only after an explicit local-user decision.",
            BTreeMap::from([
                ("project_id", field("Project ULID")),
                ("title", field("Loop title")),
                ("objective", field("Acceptance-oriented objective")),
                (
                    "write_scope",
                    json!({"type":"array","items":{"type":"string"},"minItems":1,"description":"Repository-relative write scope"}),
                ),
                (
                    "max_repairs",
                    json!({"type":"integer","minimum":1,"maximum":3,"default":2}),
                ),
            ]),
            &["project_id", "title", "objective", "write_scope"],
            false,
        ),
        definition(
            "review-loop.list",
            "List and refresh bounded review/repair loops for a project.",
            BTreeMap::from([("project_id", field("Project ULID"))]),
            &["project_id"],
            true,
        ),
        definition(
            "review-loop.get",
            "Read and refresh one bounded review/repair loop.",
            BTreeMap::from([("loop_id", field("Review/repair loop ULID"))]),
            &["loop_id"],
            true,
        ),
        definition(
            "review-loop.decide",
            "Persist an explicit local-user decision after a completed review: accept and create confirmation, generate the next bounded repair, or block.",
            BTreeMap::from([
                ("loop_id", field("Review/repair loop ULID")),
                (
                    "decision",
                    json!({"type":"string","enum":["accept","repair","block"]}),
                ),
                ("note", field("Decision evidence or rationale")),
            ]),
            &["loop_id", "decision", "note"],
            false,
        ),
        definition(
            "milestone.list",
            "List durable milestones for a project.",
            BTreeMap::from([("project_id", field("Project ULID"))]),
            &["project_id"],
            true,
        ),
        definition(
            "milestone.create",
            "Create a planned or active project milestone through domain validation.",
            BTreeMap::from([
                ("project_id", field("Project ULID")),
                ("workstream_id", field("Optional workstream ULID")),
                ("name", field("Milestone name")),
                (
                    "description",
                    field("Milestone outcome or acceptance boundary"),
                ),
                (
                    "status",
                    json!({"type": "string", "enum": ["planned", "active"]}),
                ),
                ("due_at", field("Optional UTC RFC3339 due timestamp")),
            ]),
            &["project_id", "name"],
            false,
        ),
        definition(
            "task.list",
            "List active tasks and dependencies in a workstream, with an explicit option to include archived or superseded history.",
            BTreeMap::from([
                ("workstream_id", field("Workstream ULID")),
                (
                    "include_inactive",
                    json!({"type":"boolean","default":false,"description":"Include archived and superseded Tasks"}),
                ),
            ]),
            &["workstream_id"],
            true,
        ),
        definition(
            "task.create",
            "Create a draft task through domain validation.",
            BTreeMap::from([
                ("workstream_id", field("Workstream ULID")),
                ("milestone_id", field("Optional milestone ULID")),
                ("title", field("Concise task title")),
                ("description", field("Task acceptance-oriented description")),
                ("write", json!({"type": "boolean"})),
                (
                    "path_scope",
                    json!({"type": "array", "items": {"type": "string"}}),
                ),
                (
                    "depends_on",
                    json!({"type": "array", "items": {"type": "string"}}),
                ),
                (
                    "agent_profile_id",
                    field("Optional durable Agent profile assignment"),
                ),
            ]),
            &["workstream_id", "title"],
            false,
        ),
        definition(
            "task.archive",
            "Hide an obsolete Task from the default active register without changing its Task status, Runs, events, gates, or receipts.",
            BTreeMap::from([
                ("task_id", field("Task ULID")),
                ("reason", field("Required durable archival reason")),
            ]),
            &["task_id", "reason"],
            false,
        ),
        definition(
            "task.supersede",
            "Hide an obsolete Task and preserve a reference to a verified same-project replacement without rewriting history.",
            BTreeMap::from([
                ("task_id", field("Task ULID")),
                ("replacement_task_id", field("Replacement Task ULID")),
                ("reason", field("Required durable supersede reason")),
            ]),
            &["task_id", "replacement_task_id", "reason"],
            false,
        ),
        definition(
            "task.restore",
            "Restore an archived or superseded Task to the default active register while retaining its disposition history.",
            BTreeMap::from([
                ("task_id", field("Task ULID")),
                ("reason", field("Required durable restoration reason")),
            ]),
            &["task_id", "reason"],
            false,
        ),
        definition(
            "task.disposition-history",
            "Read append-only archive, supersede, and restore decisions for one Task.",
            BTreeMap::from([("task_id", field("Task ULID"))]),
            &["task_id"],
            true,
        ),
        definition(
            "handoff.create",
            "Persist a bounded structured handoff from one task to directly dependent downstream tasks so state survives restart and compaction.",
            BTreeMap::from([
                ("source_task_id", field("Source task ULID")),
                ("source_run_id", field("Optional source run ULID")),
                (
                    "destination_task_ids",
                    json!({"type":"array","items":{"type":"string"}}),
                ),
                ("outcome", field("Verified outcome summary")),
                (
                    "decisions",
                    json!({"type":"array","items":{"type":"string"}}),
                ),
                (
                    "evidence",
                    json!({"type":"array","items":{"type":"string"}}),
                ),
                (
                    "changed_paths",
                    json!({"type":"array","items":{"type":"string"}}),
                ),
                (
                    "commands",
                    json!({"type":"array","items":{"type":"object","properties":{"command":{"type":"string"},"result":{"type":"string"}},"required":["command","result"],"additionalProperties":false}}),
                ),
                ("risks", json!({"type":"array","items":{"type":"string"}})),
                (
                    "next_steps",
                    json!({"type":"array","items":{"type":"string"}}),
                ),
                (
                    "artifact_ids",
                    json!({"type":"array","items":{"type":"string"}}),
                ),
                (
                    "receipt_ids",
                    json!({"type":"array","items":{"type":"string"}}),
                ),
            ]),
            &["source_task_id", "destination_task_ids", "outcome"],
            false,
        ),
        definition(
            "handoff.list",
            "List verified incoming or outgoing structured handoffs for one task.",
            BTreeMap::from([
                ("task_id", field("Task ULID")),
                (
                    "direction",
                    json!({"type":"string","enum":["incoming","outgoing"]}),
                ),
            ]),
            &["task_id"],
            true,
        ),
        definition(
            "handoff.get",
            "Read and verify one structured handoff artifact.",
            BTreeMap::from([("handoff_id", field("Handoff Artifact ULID"))]),
            &["handoff_id"],
            true,
        ),
        definition(
            "task.transition",
            "Request a valid task state transition.",
            BTreeMap::from([
                ("task_id", field("Task ULID")),
                ("status", field("Target task status")),
            ]),
            &["task_id", "status"],
            false,
        ),
        definition(
            "agent-profile.list",
            "List available built-in and user-defined agent profiles.",
            BTreeMap::new(),
            &[],
            true,
        ),
        definition(
            "dispatch.create",
            "Queue a task for an agent profile after policy checks.",
            BTreeMap::from([
                ("task_id", field("Task ULID")),
                ("agent_profile_id", field("Agent profile ULID")),
                ("prompt", field("Optional task prompt override")),
            ]),
            &["task_id"],
            false,
        ),
        definition(
            "dispatch.queue",
            "List the global durable dispatch queue in launch order.",
            BTreeMap::new(),
            &[],
            true,
        ),
        definition(
            "dispatch.reorder",
            "Replace the queued dispatch order using every queued dispatch exactly once.",
            BTreeMap::from([(
                "dispatch_ids",
                json!({"type":"array","items":{"type":"string"}}),
            )]),
            &["dispatch_ids"],
            false,
        ),
        definition(
            "dispatch.cancel",
            "Cancel one queued dispatch before it starts.",
            BTreeMap::from([("dispatch_id", field("Dispatch ULID"))]),
            &["dispatch_id"],
            false,
        ),
        definition(
            "scheduler.get",
            "Read global and per-project concurrency policy.",
            BTreeMap::new(),
            &[],
            true,
        ),
        definition(
            "scheduler.set",
            "Set the global shared concurrency maximum.",
            BTreeMap::from([(
                "max_concurrency",
                json!({"type":"integer","minimum":1,"maximum":64}),
            )]),
            &["max_concurrency"],
            false,
        ),
        definition(
            "scheduler.project.set",
            "Set one project's concurrency maximum, not exceeding the global maximum.",
            BTreeMap::from([
                ("project_id", field("Project ULID")),
                (
                    "max_concurrency",
                    json!({"type":"integer","minimum":1,"maximum":64}),
                ),
            ]),
            &["project_id", "max_concurrency"],
            false,
        ),
        definition(
            "run.get",
            "Read run state, capability mode, and latest event sequence.",
            BTreeMap::from([("run_id", field("Run ULID"))]),
            &["run_id"],
            true,
        ),
        definition(
            "run.events",
            "Read structured run events after a sequence number.",
            BTreeMap::from([
                ("run_id", field("Run ULID")),
                ("after", json!({"type": "integer", "minimum": 0})),
            ]),
            &["run_id"],
            true,
        ),
        definition(
            "run.message",
            "Send a structured guidance message to a live run.",
            BTreeMap::from([
                ("run_id", field("Run ULID")),
                ("message", field("Guidance for the running agent")),
            ]),
            &["run_id", "message"],
            false,
        ),
        definition(
            "run.cancel",
            "Request cancellation of a run; policy decides whether approval is needed.",
            BTreeMap::from([("run_id", field("Run ULID"))]),
            &["run_id"],
            false,
        ),
        definition(
            "gate.list",
            "List gate definitions and results for a task or run.",
            BTreeMap::from([("task_id", field("Task ULID"))]),
            &["task_id"],
            true,
        ),
        definition(
            "gate.run",
            "Request an allowed project quality gate.",
            BTreeMap::from([
                ("task_id", field("Task ULID")),
                ("gate", field("Configured gate identifier")),
                (
                    "run_id",
                    field("Optional run ULID receiving the gate event"),
                ),
            ]),
            &["task_id", "gate"],
            false,
        ),
        definition(
            "receipt.verify",
            "Verify a project's permanent receipt hash chain.",
            BTreeMap::from([("project_id", field("Project ULID"))]),
            &["project_id"],
            true,
        ),
        definition(
            "approval.list",
            "List pending and resolved approvals.",
            BTreeMap::from([("project_id", field("Optional project ULID"))]),
            &[],
            true,
        ),
        definition(
            "approval.respond",
            "Approve or deny a pending protected action.",
            BTreeMap::from([
                ("approval_id", field("Approval ULID")),
                (
                    "decision",
                    json!({"type": "string", "enum": ["approved", "approved_for_session", "denied", "cancelled"]}),
                ),
                ("reason", field("Human-readable decision reason")),
            ]),
            &["approval_id", "decision"],
            false,
        ),
    ]
}

/// Convenience entrypoint for the CLI.
///
/// # Errors
///
/// Returns an error when the process stdio stream fails or a response cannot be serialized.
pub async fn serve_stdio<B: ToolBackend>(backend: Arc<B>) -> anyhow::Result<()> {
    McpServer::new(backend)
        .serve(BufReader::new(tokio::io::stdin()), tokio::io::stdout())
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct EchoBackend;

    #[async_trait]
    impl ToolBackend for EchoBackend {
        async fn call(&self, tool: &str, arguments: Value) -> Result<Value, ToolError> {
            Ok(json!({"tool": tool, "arguments": arguments}))
        }
    }

    #[tokio::test]
    async fn lists_tools_and_routes_calls() -> anyhow::Result<()> {
        let input = concat!(
            "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"tools/list\"}\n",
            "{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/call\",\"params\":{\"name\":\"project.list\",\"arguments\":{}}}\n"
        );
        let mut output = Vec::new();
        McpServer::new(Arc::new(EchoBackend))
            .serve(BufReader::new(input.as_bytes()), &mut output)
            .await?;
        let lines = String::from_utf8(output)?;
        let responses = lines
            .lines()
            .map(serde_json::from_str::<Value>)
            .collect::<Result<Vec<_>, _>>()?;
        assert_eq!(responses.len(), 2);
        assert!(
            responses[0]["result"]["tools"]
                .as_array()
                .is_some_and(|tools| !tools.is_empty())
        );
        assert_eq!(
            responses[1]["result"]["structuredContent"]["tool"],
            "project.list"
        );
        Ok(())
    }

    #[test]
    fn catalog_never_marks_tools_destructive() {
        assert!(default_tools().iter().all(|tool| {
            tool.annotations
                .as_ref()
                .is_some_and(|annotations| !annotations.destructive_hint)
        }));
    }

    #[test]
    fn catalog_exposes_both_project_onboarding_modes() {
        let tools = default_tools();
        assert!(tools.iter().any(|tool| tool.name == "project.import"));
        assert!(tools.iter().any(|tool| tool.name == "project.create"));
        assert!(tools.iter().any(|tool| tool.name == "workflow.instantiate"));
        assert!(tools.iter().any(|tool| tool.name == "review-loop.start"));
        assert!(tools.iter().any(|tool| tool.name == "review-loop.decide"));
        assert!(tools.iter().any(|tool| tool.name == "task.archive"));
        assert!(tools.iter().any(|tool| tool.name == "task.supersede"));
        assert!(
            tools
                .iter()
                .any(|tool| tool.name == "task.disposition-history")
        );
        assert!(tools.iter().any(|tool| tool.name == "handoff.create"));
        assert!(tools.iter().any(|tool| tool.name == "handoff.list"));
    }
}
