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
            "workstream.list",
            "List workstreams for a project.",
            BTreeMap::from([("project_id", field("Project ULID"))]),
            &["project_id"],
            true,
        ),
        definition(
            "task.list",
            "List tasks and dependencies in a workstream.",
            BTreeMap::from([("workstream_id", field("Workstream ULID"))]),
            &["workstream_id"],
            true,
        ),
        definition(
            "task.create",
            "Create a draft task through domain validation.",
            BTreeMap::from([
                ("workstream_id", field("Workstream ULID")),
                ("title", field("Concise task title")),
                ("description", field("Task acceptance-oriented description")),
            ]),
            &["workstream_id", "title"],
            false,
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
            ]),
            &["task_id", "agent_profile_id"],
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
}
