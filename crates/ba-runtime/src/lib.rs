//! Runtime adapter contracts and the Codex CLI adapter.
//!
//! Codex App Server is preferred after a version/capability handshake. If that
//! handshake is unavailable or incompatible, launches automatically use the stable
//! `codex exec --json` event stream. This crate never reads or stores model credentials.

use std::collections::{BTreeMap, HashMap};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use thiserror::Error;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::process::{Child, ChildStdout, Command};
use tokio::sync::{Mutex, broadcast, mpsc};
use ulid::Ulid;

pub const MINIMUM_CODEX_VERSION: CodexVersion = CodexVersion::new(0, 130, 0);
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(8);
const PROBE_TIMEOUT: Duration = Duration::from_secs(8);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct CodexVersion {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
}

impl CodexVersion {
    #[must_use]
    pub const fn new(major: u64, minor: u64, patch: u64) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }

    /// Parse the semantic version from `codex --version` output.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeError::InvalidVersion`] when no three-part numeric version
    /// can be found.
    pub fn parse(output: &str) -> Result<Self, RuntimeError> {
        let token = output
            .split_whitespace()
            .find(|part| {
                part.chars()
                    .next()
                    .is_some_and(|character| character.is_ascii_digit() || character == 'v')
            })
            .ok_or_else(|| RuntimeError::InvalidVersion(output.trim().to_owned()))?;
        let mut segments = token.trim_start_matches(['v', 'V']).split('.');
        let major = parse_version_segment(segments.next(), output)?;
        let minor = parse_version_segment(segments.next(), output)?;
        let patch_text = segments
            .next()
            .ok_or_else(|| RuntimeError::InvalidVersion(output.trim().to_owned()))?;
        let patch_digits = patch_text
            .chars()
            .take_while(char::is_ascii_digit)
            .collect::<String>();
        let patch = patch_digits
            .parse::<u64>()
            .map_err(|_| RuntimeError::InvalidVersion(output.trim().to_owned()))?;
        Ok(Self::new(major, minor, patch))
    }
}

fn parse_version_segment(segment: Option<&str>, original: &str) -> Result<u64, RuntimeError> {
    segment
        .ok_or_else(|| RuntimeError::InvalidVersion(original.trim().to_owned()))?
        .parse::<u64>()
        .map_err(|_| RuntimeError::InvalidVersion(original.trim().to_owned()))
}

fn resolve_binary_on_path(
    binary: &Path,
    path_value: Option<&OsStr>,
    pathext: Option<&OsStr>,
) -> Option<PathBuf> {
    if binary.is_absolute()
        || binary
            .parent()
            .is_some_and(|parent| !parent.as_os_str().is_empty())
    {
        return binary.is_file().then(|| binary.to_path_buf());
    }

    let path_value = path_value?;
    for directory in std::env::split_paths(path_value) {
        let candidate = directory.join(binary);
        #[cfg(windows)]
        {
            if binary.extension().is_some() && candidate.is_file() {
                return Some(candidate);
            }
            if binary.extension().is_none() {
                let extensions = pathext
                    .and_then(OsStr::to_str)
                    .unwrap_or(".COM;.EXE;.BAT;.CMD");
                for extension in extensions.split(';').filter(|value| !value.is_empty()) {
                    let candidate = candidate.with_extension(extension.trim_start_matches('.'));
                    if candidate.is_file() {
                        return Some(candidate);
                    }
                }
            }
        }
        #[cfg(not(windows))]
        {
            let _ = pathext;
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeTransport {
    AppServer,
    ExecJson,
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeCapabilities {
    pub adapter: String,
    pub available: bool,
    pub version: Option<CodexVersion>,
    pub minimum_version: CodexVersion,
    pub app_server: bool,
    pub exec_json_fallback: bool,
    pub approvals: bool,
    pub resume: bool,
    pub guide: bool,
    pub cancellation: bool,
    pub degraded_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunRequest {
    pub prompt: String,
    pub cwd: PathBuf,
    pub sandbox: SandboxMode,
    #[serde(default)]
    pub environment: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SandboxMode {
    ReadOnly,
    WorkspaceWrite,
}

impl SandboxMode {
    const fn cli_value(self) -> &'static str {
        match self {
            Self::ReadOnly => "read-only",
            Self::WorkspaceWrite => "workspace-write",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResumeRequest {
    pub thread_id: String,
    pub prompt: String,
    pub cwd: PathBuf,
    pub sandbox: SandboxMode,
    #[serde(default)]
    pub environment: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeRunHandle {
    pub run_id: String,
    pub runtime_thread_id: Option<String>,
    pub transport: RuntimeTransport,
    pub degraded_reason: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeEventKind {
    Lifecycle,
    Output,
    Protocol,
    ApprovalRequested,
    Diagnostic,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuntimeEvent {
    pub sequence: u64,
    pub run_id: String,
    pub kind: RuntimeEventKind,
    pub name: String,
    pub payload: Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalDecision {
    Accept,
    AcceptForSession,
    Decline,
    Cancel,
}

impl ApprovalDecision {
    const fn app_server_value(self) -> &'static str {
        match self {
            Self::Accept => "accept",
            Self::AcceptForSession => "acceptForSession",
            Self::Decline => "decline",
            Self::Cancel => "cancel",
        }
    }
}

#[async_trait]
pub trait RuntimeAdapter: Send + Sync + std::fmt::Debug {
    async fn probe(&self) -> Result<RuntimeCapabilities, RuntimeError>;
    async fn launch(&self, request: RunRequest) -> Result<RuntimeRunHandle, RuntimeError>;
    async fn resume(&self, request: ResumeRequest) -> Result<RuntimeRunHandle, RuntimeError>;
    async fn guide(&self, run_id: &str, prompt: String) -> Result<(), RuntimeError>;
    async fn cancel(&self, run_id: &str) -> Result<(), RuntimeError>;
    fn subscribe(&self) -> broadcast::Receiver<RuntimeEvent>;
    async fn respond_approval(
        &self,
        run_id: &str,
        request_id: &str,
        decision: ApprovalDecision,
    ) -> Result<(), RuntimeError>;
}

#[derive(Debug, Clone)]
pub struct CodexAdapter {
    binary: PathBuf,
    events: broadcast::Sender<RuntimeEvent>,
    sessions: Arc<Mutex<HashMap<String, mpsc::Sender<ControlMessage>>>>,
    sequence: Arc<AtomicU64>,
}

impl Default for CodexAdapter {
    fn default() -> Self {
        Self::new("codex")
    }
}

impl CodexAdapter {
    #[must_use]
    pub fn new(binary: impl Into<PathBuf>) -> Self {
        let (events, _) = broadcast::channel(2_048);
        Self {
            binary: binary.into(),
            events,
            sessions: Arc::new(Mutex::new(HashMap::new())),
            sequence: Arc::new(AtomicU64::new(0)),
        }
    }

    fn resolved_binary(&self) -> PathBuf {
        resolve_binary_on_path(
            &self.binary,
            std::env::var_os("PATH").as_deref(),
            std::env::var_os("PATHEXT").as_deref(),
        )
        .unwrap_or_else(|| self.binary.clone())
    }

    async fn command_output(&self, args: &[&str]) -> Result<std::process::Output, RuntimeError> {
        let mut command = Command::new(self.resolved_binary());
        command
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        tokio::time::timeout(PROBE_TIMEOUT, command.output())
            .await
            .map_err(|_| RuntimeError::CommandTimeout(args.join(" ")))?
            .map_err(RuntimeError::Io)
    }

    fn emit(&self, run_id: &str, kind: RuntimeEventKind, name: &str, payload: Value) {
        let event = RuntimeEvent {
            sequence: self.sequence.fetch_add(1, Ordering::Relaxed) + 1,
            run_id: run_id.to_owned(),
            kind,
            name: name.to_owned(),
            payload,
        };
        let _ignored = self.events.send(event);
    }

    async fn start(
        &self,
        request: RunRequest,
        resume_thread: Option<String>,
    ) -> Result<RuntimeRunHandle, RuntimeError> {
        validate_working_directory(&request.cwd)?;
        if request.prompt.trim().is_empty() {
            return Err(RuntimeError::EmptyPrompt);
        }
        if resume_thread.as_deref().is_some_and(str::is_empty) {
            return Err(RuntimeError::InvalidThreadId);
        }
        let capabilities = self.probe().await?;
        if !capabilities.available {
            return Err(RuntimeError::Unavailable(
                capabilities
                    .degraded_reason
                    .unwrap_or_else(|| "Codex CLI is unavailable".to_owned()),
            ));
        }
        let run_id = Ulid::new().to_string();

        if capabilities.app_server {
            match self
                .start_app_server(&run_id, &request, resume_thread.as_deref())
                .await
            {
                Ok((thread_id, sender)) => {
                    self.sessions.lock().await.insert(run_id.clone(), sender);
                    return Ok(RuntimeRunHandle {
                        run_id,
                        runtime_thread_id: Some(thread_id),
                        transport: RuntimeTransport::AppServer,
                        degraded_reason: None,
                    });
                }
                Err(error) => {
                    self.emit(
                        &run_id,
                        RuntimeEventKind::Diagnostic,
                        "app_server_fallback",
                        json!({ "reason": error.to_string(), "fallback": "codex exec --json" }),
                    );
                    let reason = format!("App Server handshake failed: {error}");
                    let sender = self
                        .start_exec(&run_id, &request, resume_thread.as_deref())
                        .await?;
                    self.sessions.lock().await.insert(run_id.clone(), sender);
                    return Ok(RuntimeRunHandle {
                        run_id,
                        runtime_thread_id: resume_thread,
                        transport: RuntimeTransport::ExecJson,
                        degraded_reason: Some(reason),
                    });
                }
            }
        }

        let sender = self
            .start_exec(&run_id, &request, resume_thread.as_deref())
            .await?;
        self.sessions.lock().await.insert(run_id.clone(), sender);
        Ok(RuntimeRunHandle {
            run_id,
            runtime_thread_id: resume_thread,
            transport: RuntimeTransport::ExecJson,
            degraded_reason: capabilities.degraded_reason,
        })
    }

    #[allow(clippy::too_many_lines)]
    async fn start_app_server(
        &self,
        run_id: &str,
        request: &RunRequest,
        resume_thread: Option<&str>,
    ) -> Result<(String, mpsc::Sender<ControlMessage>), RuntimeError> {
        let mut child = Command::new(self.resolved_binary())
            .args(["app-server", "--listen", "stdio://"])
            .envs(&request.environment)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()?;
        let mut stdin = child
            .stdin
            .take()
            .ok_or(RuntimeError::MissingProcessPipe("stdin"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or(RuntimeError::MissingProcessPipe("stdout"))?;
        let mut lines = BufReader::new(stdout).lines();

        write_json_line(
            &mut stdin,
            &json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": {
                    "clientInfo": { "name": "ba-agent-control", "version": env!("CARGO_PKG_VERSION") },
                    "capabilities": { "experimentalApi": false }
                }
            }),
        )
        .await?;
        let (_initialize_response, _initialize_messages) = read_response(&mut lines, 1).await?;
        write_json_line(
            &mut stdin,
            &json!({ "jsonrpc": "2.0", "method": "initialized" }),
        )
        .await?;

        let thread_method = if resume_thread.is_some() {
            "thread/resume"
        } else {
            "thread/start"
        };
        let thread_params = if let Some(thread_id) = resume_thread {
            json!({
                "threadId": thread_id,
                "cwd": request.cwd,
                "approvalPolicy": "on-request",
                "sandbox": request.sandbox.cli_value()
            })
        } else {
            json!({
                "cwd": request.cwd,
                "approvalPolicy": "on-request",
                "sandbox": request.sandbox.cli_value(),
                "ephemeral": false
            })
        };
        write_json_line(
            &mut stdin,
            &json!({ "jsonrpc": "2.0", "id": 2, "method": thread_method, "params": thread_params }),
        )
        .await?;
        let (thread_response, mut buffered_messages) = read_response(&mut lines, 2).await?;
        let thread_id = thread_response
            .pointer("/result/thread/id")
            .and_then(Value::as_str)
            .or(resume_thread)
            .ok_or_else(|| RuntimeError::Protocol("thread response did not contain an id".into()))?
            .to_owned();

        write_json_line(
            &mut stdin,
            &turn_start_message(3, &thread_id, &request.prompt),
        )
        .await?;
        let (turn_response, turn_messages) = read_response(&mut lines, 3).await?;
        buffered_messages.extend(turn_messages);
        let active_turn = turn_response
            .pointer("/result/turn/id")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned);

        let (sender, receiver) = mpsc::channel(64);
        let adapter = self.clone();
        let owned_run_id = run_id.to_owned();
        let owned_thread_id = thread_id.clone();
        tokio::spawn(async move {
            adapter.emit(
                &owned_run_id,
                RuntimeEventKind::Lifecycle,
                "running",
                json!({ "transport": "app_server", "thread_id": owned_thread_id }),
            );
            run_app_server_session(
                adapter,
                owned_run_id,
                owned_thread_id,
                active_turn,
                child,
                stdin,
                lines,
                buffered_messages,
                receiver,
            )
            .await;
        });
        Ok((thread_id, sender))
    }

    async fn start_exec(
        &self,
        run_id: &str,
        request: &RunRequest,
        resume_thread: Option<&str>,
    ) -> Result<mpsc::Sender<ControlMessage>, RuntimeError> {
        let mut command = Command::new(self.resolved_binary());
        command
            .arg("exec")
            .args([
                "--json",
                "--color",
                "never",
                "--sandbox",
                request.sandbox.cli_value(),
            ])
            .arg("-C")
            .arg(&request.cwd)
            .envs(&request.environment);
        if let Some(thread_id) = resume_thread {
            command.args(["resume", "--", thread_id, "-"]);
        } else {
            command.arg("-");
        }
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()?;
        let mut stdin = child
            .stdin
            .take()
            .ok_or(RuntimeError::MissingProcessPipe("stdin"))?;
        if let Err(error) = async {
            stdin.write_all(request.prompt.as_bytes()).await?;
            stdin.shutdown().await
        }
        .await
        {
            let _ignored = child.kill().await;
            return Err(RuntimeError::Io(error));
        }
        let stdout = child
            .stdout
            .take()
            .ok_or(RuntimeError::MissingProcessPipe("stdout"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or(RuntimeError::MissingProcessPipe("stderr"))?;
        let (sender, receiver) = mpsc::channel(16);
        let adapter = self.clone();
        let owned_run_id = run_id.to_owned();
        tokio::spawn(async move {
            run_exec_session(adapter, owned_run_id, child, stdout, stderr, receiver).await;
        });
        Ok(sender)
    }

    async fn control(&self, run_id: &str, message: ControlMessage) -> Result<(), RuntimeError> {
        let sender = self
            .sessions
            .lock()
            .await
            .get(run_id)
            .cloned()
            .ok_or_else(|| RuntimeError::UnknownRun(run_id.to_owned()))?;
        sender
            .send(message)
            .await
            .map_err(|_| RuntimeError::RunClosed(run_id.to_owned()))
    }
}

#[async_trait]
impl RuntimeAdapter for CodexAdapter {
    async fn probe(&self) -> Result<RuntimeCapabilities, RuntimeError> {
        let version_output = match self.command_output(&["--version"]).await {
            Ok(output) => output,
            Err(RuntimeError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(unavailable_capabilities("Codex CLI was not found on PATH"));
            }
            Err(error) => return Err(error),
        };
        if !version_output.status.success() {
            return Ok(unavailable_capabilities(
                String::from_utf8_lossy(&version_output.stderr).trim(),
            ));
        }
        let version_text = String::from_utf8_lossy(&version_output.stdout);
        let version = CodexVersion::parse(&version_text)?;
        if version < MINIMUM_CODEX_VERSION {
            return Ok(RuntimeCapabilities {
                adapter: "codex".into(),
                available: false,
                version: Some(version),
                minimum_version: MINIMUM_CODEX_VERSION,
                app_server: false,
                exec_json_fallback: false,
                approvals: false,
                resume: false,
                guide: false,
                cancellation: false,
                degraded_reason: Some(format!(
                    "Codex CLI {version:?} is older than the required {MINIMUM_CODEX_VERSION:?}"
                )),
            });
        }
        let app_server_output = self.command_output(&["app-server", "--help"]).await?;
        let app_server = app_server_output.status.success();
        Ok(RuntimeCapabilities {
            adapter: "codex".into(),
            available: true,
            version: Some(version),
            minimum_version: MINIMUM_CODEX_VERSION,
            app_server,
            exec_json_fallback: true,
            approvals: app_server,
            resume: true,
            guide: app_server,
            cancellation: true,
            degraded_reason: (!app_server).then(|| {
                "App Server unavailable; using codex exec --json with no live approval or guidance channel"
                    .to_owned()
            }),
        })
    }

    async fn launch(&self, request: RunRequest) -> Result<RuntimeRunHandle, RuntimeError> {
        self.start(request, None).await
    }

    async fn resume(&self, request: ResumeRequest) -> Result<RuntimeRunHandle, RuntimeError> {
        self.start(
            RunRequest {
                prompt: request.prompt,
                cwd: request.cwd,
                sandbox: request.sandbox,
                environment: request.environment,
            },
            Some(request.thread_id),
        )
        .await
    }

    async fn guide(&self, run_id: &str, prompt: String) -> Result<(), RuntimeError> {
        self.control(run_id, ControlMessage::Guide(prompt)).await
    }

    async fn cancel(&self, run_id: &str) -> Result<(), RuntimeError> {
        self.control(run_id, ControlMessage::Cancel).await
    }

    fn subscribe(&self) -> broadcast::Receiver<RuntimeEvent> {
        self.events.subscribe()
    }

    async fn respond_approval(
        &self,
        run_id: &str,
        request_id: &str,
        decision: ApprovalDecision,
    ) -> Result<(), RuntimeError> {
        self.control(
            run_id,
            ControlMessage::Approval {
                request_id: request_id.to_owned(),
                decision,
            },
        )
        .await
    }
}

#[derive(Debug)]
enum ControlMessage {
    Guide(String),
    Cancel,
    Approval {
        request_id: String,
        decision: ApprovalDecision,
    },
}

#[allow(clippy::too_many_arguments)]
#[allow(clippy::too_many_lines)]
async fn run_app_server_session(
    adapter: CodexAdapter,
    run_id: String,
    thread_id: String,
    mut active_turn: Option<String>,
    mut child: Child,
    mut stdin: tokio::process::ChildStdin,
    mut lines: Lines<BufReader<ChildStdout>>,
    buffered_messages: Vec<Value>,
    mut control: mpsc::Receiver<ControlMessage>,
) {
    let mut request_id = 10_u64;
    let mut pending_approvals = HashMap::<String, PendingApproval>::new();
    let mut terminal_emitted = false;
    for message in buffered_messages {
        terminal_emitted = handle_app_server_message(
            &adapter,
            &run_id,
            message,
            &mut active_turn,
            &mut pending_approvals,
        );
        if terminal_emitted {
            break;
        }
    }
    while !terminal_emitted {
        tokio::select! {
            line = lines.next_line() => {
                match line {
                    Ok(Some(line)) => match serde_json::from_str::<Value>(&line) {
                        Ok(message) => {
                            terminal_emitted = handle_app_server_message(
                                &adapter,
                                &run_id,
                                message,
                                &mut active_turn,
                                &mut pending_approvals,
                            );
                        }
                        Err(error) => adapter.emit(&run_id, RuntimeEventKind::Diagnostic, "invalid_json", json!({ "line": line, "error": error.to_string() })),
                    },
                    Ok(None) => break,
                    Err(error) => {
                        adapter.emit(&run_id, RuntimeEventKind::Diagnostic, "read_error", json!({ "error": error.to_string() }));
                        break;
                    }
                }
            }
            command = control.recv() => {
                match command {
                    Some(ControlMessage::Guide(prompt)) => {
                        request_id += 1;
                        let message = if let Some(turn_id) = &active_turn {
                            json!({
                                "jsonrpc": "2.0", "id": request_id, "method": "turn/steer",
                                "params": { "threadId": thread_id, "expectedTurnId": turn_id, "input": [{"type":"text", "text":prompt}] }
                            })
                        } else {
                            turn_start_message(request_id, &thread_id, &prompt)
                        };
                        if let Err(error) = write_json_line(&mut stdin, &message).await {
                            adapter.emit(&run_id, RuntimeEventKind::Diagnostic, "guide_failed", json!({ "error": error.to_string() }));
                        }
                    }
                    Some(ControlMessage::Approval { request_id: approval_id, decision }) => {
                        if let Some(approval) = pending_approvals.remove(&approval_id) {
                            let message = approval.response(decision);
                            if let Err(error) = write_json_line(&mut stdin, &message).await {
                                adapter.emit(&run_id, RuntimeEventKind::Diagnostic, "approval_response_failed", json!({ "error": error.to_string() }));
                            }
                        } else {
                            adapter.emit(
                                &run_id,
                                RuntimeEventKind::Diagnostic,
                                "unknown_approval",
                                json!({ "request_id": approval_id }),
                            );
                        }
                    }
                    Some(ControlMessage::Cancel) => {
                        if let Some(turn_id) = &active_turn {
                            request_id += 1;
                            let message = json!({
                                "jsonrpc":"2.0", "id":request_id, "method":"turn/interrupt",
                                "params":{"threadId":thread_id, "turnId":turn_id}
                            });
                            let _ignored = write_json_line(&mut stdin, &message).await;
                        }
                        let _ignored = child.kill().await;
                        adapter.emit(&run_id, RuntimeEventKind::Lifecycle, "cancelled", json!({}));
                        terminal_emitted = true;
                        break;
                    }
                    None => break,
                }
            }
        }
    }
    let _ignored = child.kill().await;
    match child.wait().await {
        Ok(status) if !terminal_emitted => adapter.emit(
            &run_id,
            RuntimeEventKind::Lifecycle,
            "failed",
            json!({ "status": status.code(), "reason": "app_server_closed_before_turn_completed" }),
        ),
        Err(error) => adapter.emit(
            &run_id,
            RuntimeEventKind::Diagnostic,
            "wait_failed",
            json!({ "error": error.to_string() }),
        ),
        Ok(_) => {}
    }
    adapter.sessions.lock().await.remove(&run_id);
}

fn handle_app_server_message(
    adapter: &CodexAdapter,
    run_id: &str,
    message: Value,
    active_turn: &mut Option<String>,
    pending_approvals: &mut HashMap<String, PendingApproval>,
) -> bool {
    let method = message
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or("protocol/message")
        .to_owned();
    let terminal = method == "turn/completed";
    if method == "turn/started" {
        *active_turn = message
            .pointer("/params/turn/id")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned);
    } else if terminal {
        *active_turn = None;
        let lifecycle = app_server_completion_name(&message);
        adapter.emit(
            run_id,
            RuntimeEventKind::Lifecycle,
            lifecycle,
            message.clone(),
        );
    }
    let kind = if is_approval_request(&method) && message.get("id").is_some() {
        if let Some(approval) = PendingApproval::from_message(&message) {
            pending_approvals.insert(approval.key.clone(), approval);
        }
        RuntimeEventKind::ApprovalRequested
    } else {
        RuntimeEventKind::Protocol
    };
    adapter.emit(run_id, kind, &method, message);
    terminal
}

#[allow(clippy::too_many_lines)]
async fn run_exec_session(
    adapter: CodexAdapter,
    run_id: String,
    mut child: Child,
    stdout: ChildStdout,
    stderr: tokio::process::ChildStderr,
    mut control: mpsc::Receiver<ControlMessage>,
) {
    adapter.emit(
        &run_id,
        RuntimeEventKind::Lifecycle,
        "running",
        json!({ "transport": "exec_json", "degraded": true }),
    );
    let stderr_adapter = adapter.clone();
    let stderr_run_id = run_id.clone();
    tokio::spawn(async move {
        let mut stderr_lines = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = stderr_lines.next_line().await {
            stderr_adapter.emit(
                &stderr_run_id,
                RuntimeEventKind::Diagnostic,
                "stderr",
                json!({ "text": line }),
            );
        }
    });

    let mut lines = BufReader::new(stdout).lines();
    let mut cancelled = false;
    let mut terminal_emitted = false;
    loop {
        tokio::select! {
            line = lines.next_line() => match line {
                Ok(Some(line)) => {
                    match serde_json::from_str::<Value>(&line) {
                        Ok(payload) => {
                            let name = payload.get("type").and_then(Value::as_str).unwrap_or("output").to_owned();
                            let (kind, normalized_name, lifecycle) = classify_exec_event(&name);
                            terminal_emitted |= lifecycle;
                            adapter.emit(&run_id, kind, normalized_name, payload);
                        }
                        Err(error) => adapter.emit(
                            &run_id,
                            RuntimeEventKind::Diagnostic,
                            "invalid_json",
                            json!({ "line": line, "error": error.to_string() }),
                        ),
                    }
                }
                Ok(None) => break,
                Err(error) => {
                    adapter.emit(&run_id, RuntimeEventKind::Diagnostic, "read_error", json!({ "error": error.to_string() }));
                    break;
                }
            },
            command = control.recv() => match command {
                Some(ControlMessage::Cancel) => {
                    cancelled = true;
                    let _ignored = child.kill().await;
                    break;
                }
                Some(ControlMessage::Guide(_)) => adapter.emit(
                    &run_id,
                    RuntimeEventKind::Diagnostic,
                    "capability_unavailable",
                    json!({ "capability": "guide", "reason": "exec_json_fallback" }),
                ),
                Some(ControlMessage::Approval { .. }) => adapter.emit(
                    &run_id,
                    RuntimeEventKind::Diagnostic,
                    "capability_unavailable",
                    json!({ "capability": "approval_response", "reason": "exec_json_fallback" }),
                ),
                None => break,
            }
        }
    }
    match child.wait().await {
        Ok(status) if cancelled => adapter.emit(
            &run_id,
            RuntimeEventKind::Lifecycle,
            "cancelled",
            json!({ "status": status.code() }),
        ),
        Ok(status) if terminal_emitted => {
            if !status.success() {
                adapter.emit(
                    &run_id,
                    RuntimeEventKind::Diagnostic,
                    "process_exit_after_terminal_event",
                    json!({ "status": status.code() }),
                );
            }
        }
        Ok(status) if status.success() => adapter.emit(
            &run_id,
            RuntimeEventKind::Lifecycle,
            "succeeded",
            json!({ "status": status.code() }),
        ),
        Ok(status) => adapter.emit(
            &run_id,
            RuntimeEventKind::Lifecycle,
            "failed",
            json!({ "status": status.code() }),
        ),
        Err(error) => adapter.emit(
            &run_id,
            RuntimeEventKind::Diagnostic,
            "wait_failed",
            json!({ "error": error.to_string() }),
        ),
    }
    adapter.sessions.lock().await.remove(&run_id);
}

fn turn_start_message(id: u64, thread_id: &str, prompt: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "turn/start",
        "params": {
            "threadId": thread_id,
            "input": [{ "type": "text", "text": prompt }]
        }
    })
}

#[derive(Debug)]
struct PendingApproval {
    key: String,
    id: Value,
    method: String,
    requested_permissions: Option<Value>,
}

impl PendingApproval {
    fn from_message(message: &Value) -> Option<Self> {
        let id = message.get("id")?.clone();
        let key = match &id {
            Value::String(value) => value.clone(),
            Value::Number(value) => value.to_string(),
            _ => return None,
        };
        let method = message.get("method")?.as_str()?.to_owned();
        Some(Self {
            key,
            requested_permissions: message.pointer("/params/permissions").cloned(),
            id,
            method,
        })
    }

    fn response(self, decision: ApprovalDecision) -> Value {
        let result = if self.method == "item/permissions/requestApproval" {
            let permissions = if matches!(
                decision,
                ApprovalDecision::Accept | ApprovalDecision::AcceptForSession
            ) {
                self.requested_permissions.unwrap_or_else(|| json!({}))
            } else {
                json!({})
            };
            json!({
                "permissions": permissions,
                "scope": if decision == ApprovalDecision::AcceptForSession { "session" } else { "turn" }
            })
        } else {
            json!({ "decision": decision.app_server_value() })
        };
        json!({ "jsonrpc": "2.0", "id": self.id, "result": result })
    }
}

fn is_approval_request(method: &str) -> bool {
    matches!(
        method,
        "item/commandExecution/requestApproval"
            | "item/fileChange/requestApproval"
            | "item/permissions/requestApproval"
    )
}

fn app_server_completion_name(message: &Value) -> &'static str {
    match message
        .pointer("/params/turn/status")
        .and_then(Value::as_str)
    {
        Some("interrupted") => "cancelled",
        Some("completed") => "succeeded",
        _ => "failed",
    }
}

fn classify_exec_event(name: &str) -> (RuntimeEventKind, &str, bool) {
    match name {
        "turn.completed" => (RuntimeEventKind::Lifecycle, "succeeded", true),
        "turn.failed" => (RuntimeEventKind::Lifecycle, "failed", true),
        "error" => (RuntimeEventKind::Diagnostic, name, false),
        "thread.started" | "turn.started" => (RuntimeEventKind::Protocol, name, false),
        _ => (RuntimeEventKind::Output, name, false),
    }
}

async fn write_json_line(
    stdin: &mut tokio::process::ChildStdin,
    message: &Value,
) -> Result<(), RuntimeError> {
    let mut encoded = serde_json::to_vec(message)?;
    encoded.push(b'\n');
    stdin.write_all(&encoded).await?;
    stdin.flush().await?;
    Ok(())
}

async fn read_response(
    lines: &mut Lines<BufReader<ChildStdout>>,
    expected_id: u64,
) -> Result<(Value, Vec<Value>), RuntimeError> {
    tokio::time::timeout(HANDSHAKE_TIMEOUT, async {
        let mut buffered = Vec::new();
        loop {
            let line = lines.next_line().await?.ok_or_else(|| {
                RuntimeError::Protocol("App Server closed during handshake".into())
            })?;
            let message = serde_json::from_str::<Value>(&line)?;
            if message.get("id").and_then(Value::as_u64) == Some(expected_id) {
                if let Some(error) = message.get("error") {
                    return Err(RuntimeError::Protocol(error.to_string()));
                }
                return Ok((message, buffered));
            }
            buffered.push(message);
        }
    })
    .await
    .map_err(|_| RuntimeError::HandshakeTimeout)?
}

fn validate_working_directory(path: &Path) -> Result<(), RuntimeError> {
    let metadata = std::fs::metadata(path)?;
    if !metadata.is_dir() || !path.is_absolute() {
        return Err(RuntimeError::InvalidWorkingDirectory(path.to_path_buf()));
    }
    Ok(())
}

fn unavailable_capabilities(reason: impl Into<String>) -> RuntimeCapabilities {
    RuntimeCapabilities {
        adapter: "codex".into(),
        available: false,
        version: None,
        minimum_version: MINIMUM_CODEX_VERSION,
        app_server: false,
        exec_json_fallback: false,
        approvals: false,
        resume: false,
        guide: false,
        cancellation: false,
        degraded_reason: Some(reason.into()),
    }
}

#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error("runtime is unavailable: {0}")]
    Unavailable(String),
    #[error("invalid Codex version output: {0}")]
    InvalidVersion(String),
    #[error("runtime command timed out: {0}")]
    CommandTimeout(String),
    #[error("runtime prompt must not be empty")]
    EmptyPrompt,
    #[error("runtime thread id must not be empty")]
    InvalidThreadId,
    #[error("invalid working directory: {0}")]
    InvalidWorkingDirectory(PathBuf),
    #[error("runtime process did not expose {0}")]
    MissingProcessPipe(&'static str),
    #[error("App Server handshake timed out")]
    HandshakeTimeout,
    #[error("App Server protocol error: {0}")]
    Protocol(String),
    #[error("unknown run: {0}")]
    UnknownRun(String),
    #[error("run control channel is closed: {0}")]
    RunClosed(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_supported_version() -> Result<(), RuntimeError> {
        assert_eq!(
            CodexVersion::parse("codex-cli 0.130.0\n")?,
            MINIMUM_CODEX_VERSION
        );
        assert_eq!(
            CodexVersion::parse("codex-cli v0.130.1-beta.2")?,
            CodexVersion::new(0, 130, 1)
        );
        assert!(CodexVersion::parse("codex-cli unknown").is_err());
        Ok(())
    }

    #[cfg(windows)]
    #[test]
    fn resolves_windows_shims_in_path_order() -> Result<(), Box<dyn std::error::Error>> {
        let root = tempfile::tempdir()?;
        let shim_directory = root.path().join("shim");
        let app_directory = root.path().join("app");
        std::fs::create_dir_all(&shim_directory)?;
        std::fs::create_dir_all(&app_directory)?;
        std::fs::write(shim_directory.join("codex.cmd"), "@echo off\r\n")?;
        std::fs::write(app_directory.join("codex.exe"), b"fixture")?;
        let search_path = std::env::join_paths([&shim_directory, &app_directory])?;

        let resolved = resolve_binary_on_path(
            Path::new("codex"),
            Some(search_path.as_os_str()),
            Some(OsStr::new(".COM;.EXE;.BAT;.CMD")),
        )
        .ok_or("Codex shim was not resolved")?;
        assert!(
            resolved
                .to_string_lossy()
                .eq_ignore_ascii_case(&shim_directory.join("codex.cmd").to_string_lossy())
        );
        Ok(())
    }

    #[test]
    fn app_server_fixture_matches_required_contract() -> Result<(), Box<dyn std::error::Error>> {
        let fixture = include_str!("../tests/fixtures/app_server_contract.jsonl");
        let messages = fixture
            .lines()
            .map(serde_json::from_str::<Value>)
            .collect::<Result<Vec<_>, _>>()?;
        let methods = messages
            .iter()
            .filter_map(|message| message.get("method").and_then(Value::as_str))
            .collect::<Vec<_>>();
        assert!(methods.contains(&"initialize"));
        assert!(methods.contains(&"initialized"));
        assert!(methods.contains(&"thread/start"));
        assert!(methods.contains(&"turn/start"));
        assert!(
            messages
                .iter()
                .all(|message| message.get("jsonrpc") == Some(&json!("2.0")))
        );
        Ok(())
    }

    #[test]
    fn approval_responses_match_current_app_server_contract() -> Result<(), RuntimeError> {
        let command = PendingApproval::from_message(&json!({
            "jsonrpc": "2.0",
            "id": "approval-a",
            "method": "item/commandExecution/requestApproval",
            "params": {}
        }))
        .ok_or_else(|| RuntimeError::Protocol("invalid test fixture".to_owned()))?;
        assert_eq!(
            command.response(ApprovalDecision::Accept),
            json!({
                "jsonrpc": "2.0",
                "id": "approval-a",
                "result": { "decision": "accept" }
            })
        );

        let permissions = PendingApproval::from_message(&json!({
            "jsonrpc": "2.0",
            "id": 41,
            "method": "item/permissions/requestApproval",
            "params": { "permissions": { "network": { "enabled": true } } }
        }))
        .ok_or_else(|| RuntimeError::Protocol("invalid test fixture".to_owned()))?;
        assert_eq!(
            permissions.response(ApprovalDecision::AcceptForSession),
            json!({
                "jsonrpc": "2.0",
                "id": 41,
                "result": {
                    "permissions": { "network": { "enabled": true } },
                    "scope": "session"
                }
            })
        );
        Ok(())
    }

    #[test]
    fn generated_schema_locks_app_server_0130_contract() -> Result<(), Box<dyn std::error::Error>> {
        let schema = serde_json::from_str::<Value>(include_str!(
            "../tests/fixtures/app_server_schema_0.130.0/codex_app_server_protocol.v2.schemas.json"
        ))?;

        let required = |definition: &str| -> Vec<&str> {
            schema
                .pointer(&format!("/definitions/{definition}/required"))
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .collect()
        };
        assert_eq!(required("InitializeParams"), vec!["clientInfo"]);
        assert_eq!(required("ThreadResumeParams"), vec!["threadId"]);
        assert_eq!(required("TurnStartParams"), vec!["input", "threadId"]);
        assert_eq!(
            required("TurnSteerParams"),
            vec!["expectedTurnId", "input", "threadId"]
        );
        assert_eq!(required("TurnInterruptParams"), vec!["threadId", "turnId"]);

        let start = turn_start_message(7, "thread-fixture", "inspect the task");
        assert_eq!(start.pointer("/method"), Some(&json!("turn/start")));
        assert_eq!(
            start.pointer("/params/input/0"),
            Some(&json!({ "type": "text", "text": "inspect the task" }))
        );

        let approval_schema = serde_json::from_str::<Value>(include_str!(
            "../tests/fixtures/app_server_schema_0.130.0/CommandExecutionRequestApprovalResponse.json"
        ))?;
        let decisions = approval_schema
            .pointer("/definitions/CommandExecutionApprovalDecision/oneOf")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|item| item.pointer("/enum/0").and_then(Value::as_str))
            .collect::<Vec<_>>();
        for value in ["accept", "acceptForSession", "decline", "cancel"] {
            assert!(decisions.contains(&value));
        }
        assert_eq!(ApprovalDecision::Accept.app_server_value(), "accept");
        assert_eq!(
            ApprovalDecision::AcceptForSession.app_server_value(),
            "acceptForSession"
        );
        assert_eq!(ApprovalDecision::Decline.app_server_value(), "decline");
        assert_eq!(ApprovalDecision::Cancel.app_server_value(), "cancel");
        Ok(())
    }

    #[test]
    fn terminal_events_are_normalized() {
        assert_eq!(
            app_server_completion_name(&json!({ "params": { "turn": { "status": "completed" } } })),
            "succeeded"
        );
        assert_eq!(
            app_server_completion_name(
                &json!({ "params": { "turn": { "status": "interrupted" } } })
            ),
            "cancelled"
        );
        assert_eq!(
            classify_exec_event("turn.failed"),
            (RuntimeEventKind::Lifecycle, "failed", true)
        );
    }

    #[test]
    fn handshake_notifications_are_replayed_without_losing_approvals_or_terminal_state() {
        let adapter = CodexAdapter::new("codex");
        let mut receiver = adapter.subscribe();
        let mut active_turn = Some("turn-fixture".into());
        let mut approvals = HashMap::new();
        assert!(!handle_app_server_message(
            &adapter,
            "run-fixture",
            json!({
                "jsonrpc": "2.0",
                "id": "approval-fixture",
                "method": "item/commandExecution/requestApproval",
                "params": {"command": ["cargo", "test"]}
            }),
            &mut active_turn,
            &mut approvals,
        ));
        assert!(approvals.contains_key("approval-fixture"));
        assert_eq!(
            receiver.try_recv().map(|event| event.kind),
            Ok(RuntimeEventKind::ApprovalRequested)
        );

        assert!(handle_app_server_message(
            &adapter,
            "run-fixture",
            json!({
                "method": "turn/completed",
                "params": {"turn": {"id": "turn-fixture", "status": "completed"}}
            }),
            &mut active_turn,
            &mut approvals,
        ));
        assert!(active_turn.is_none());
        assert_eq!(
            receiver.try_recv().map(|event| event.name),
            Ok("succeeded".into())
        );
    }
}
