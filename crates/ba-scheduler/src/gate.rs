use std::collections::BTreeMap;
use std::path::Path;
use std::process::Stdio;
use std::time::{Duration, Instant};

use ba_core::{EntityId, Gate, GateStatus, now};
use ba_store::SqliteStore;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::Command;

use crate::SchedulerError;

const MAX_CAPTURE_BYTES: usize = 64 * 1024;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct GateDefinition {
    pub name: String,
    pub program: String,
    #[serde(default)]
    pub args: Vec<String>,
    pub timeout_ms: u64,
}

impl GateDefinition {
    fn validate(&self) -> Result<(), SchedulerError> {
        if self.name.trim().is_empty() {
            return Err(SchedulerError::InvalidGate(
                "gate name must not be empty".to_owned(),
            ));
        }
        if self.program.trim().is_empty() {
            return Err(SchedulerError::InvalidGate(
                "gate program must not be empty".to_owned(),
            ));
        }
        if self.timeout_ms == 0 {
            return Err(SchedulerError::InvalidGate(
                "gate timeout must be greater than zero".to_owned(),
            ));
        }
        if self.program.contains('\0') || self.args.iter().any(|argument| argument.contains('\0')) {
            return Err(SchedulerError::InvalidGate(
                "gate command contains a NUL byte".to_owned(),
            ));
        }
        Ok(())
    }
}

/// Load a quality gate from `.ba-agent/project.toml` as an argv array.
///
/// # Errors
///
/// Returns an error when the configuration is missing, malformed, or contains a
/// non-string/empty command.
pub fn configured_gate(project_root: &Path, name: &str) -> Result<GateDefinition, SchedulerError> {
    if name.trim().is_empty() {
        return Err(SchedulerError::InvalidGate(
            "gate name must not be empty".into(),
        ));
    }
    let path = project_root.join(".ba-agent/project.toml");
    let source = std::fs::read_to_string(&path).map_err(|error| {
        SchedulerError::InvalidGate(format!(
            "unable to read gate configuration {}: {error}",
            path.display()
        ))
    })?;
    let document: toml::Value = toml::from_str(&source)
        .map_err(|error| SchedulerError::InvalidGate(format!("invalid project.toml: {error}")))?;
    let command = document
        .get("gates")
        .and_then(|value| value.get(name))
        .and_then(toml::Value::as_array)
        .ok_or_else(|| {
            SchedulerError::InvalidGate("gate is not configured as an argv array".into())
        })?;
    let command_parts = command
        .iter()
        .map(|value| {
            value.as_str().map(str::to_owned).ok_or_else(|| {
                SchedulerError::InvalidGate("gate argv entries must be strings".into())
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let (program, arguments) = command_parts
        .split_first()
        .ok_or_else(|| SchedulerError::InvalidGate("configured gate command is empty".into()))?;
    let definition = GateDefinition {
        name: name.to_owned(),
        program: program.clone(),
        args: arguments.to_vec(),
        timeout_ms: 600_000,
    };
    definition.validate()?;
    Ok(definition)
}

/// List configured non-empty quality gates that must pass before local commit.
///
/// # Errors
///
/// Returns an error when `.ba-agent/project.toml` cannot be read or parsed.
pub fn configured_gate_names(project_root: &Path) -> Result<Vec<String>, SchedulerError> {
    let path = project_root.join(".ba-agent/project.toml");
    let source = std::fs::read_to_string(&path).map_err(|error| {
        SchedulerError::InvalidGate(format!(
            "unable to read gate configuration {}: {error}",
            path.display()
        ))
    })?;
    let document: toml::Value = toml::from_str(&source)
        .map_err(|error| SchedulerError::InvalidGate(format!("invalid project.toml: {error}")))?;
    let gates = document
        .get("gates")
        .and_then(toml::Value::as_table)
        .ok_or_else(|| SchedulerError::InvalidGate("missing [gates] table".into()))?;
    let mut names = gates
        .iter()
        .filter_map(|(name, value)| {
            value
                .as_array()
                .filter(|command| !command.is_empty())
                .map(|_| name.clone())
        })
        .collect::<Vec<_>>();
    names.sort();
    Ok(names)
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct GateExecution {
    pub status: GateStatus,
    pub exit_code: Option<i32>,
    pub duration_ms: u64,
    pub stdout: String,
    pub stderr: String,
    pub output_truncated: bool,
    pub timed_out: bool,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct GateRunner;

impl GateRunner {
    /// Execute one configured gate as an argv vector with bounded output and time.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid definitions/directories, process failures, or
    /// output-capture failures.
    pub async fn run(
        &self,
        cwd: &Path,
        definition: &GateDefinition,
    ) -> Result<GateExecution, SchedulerError> {
        definition.validate()?;
        if !cwd.is_absolute() || !cwd.is_dir() {
            return Err(SchedulerError::InvalidGateWorkingDirectory(
                cwd.to_path_buf(),
            ));
        }
        let mut child = Command::new(&definition.program)
            .args(&definition.args)
            .current_dir(cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()?;
        let stdout = child
            .stdout
            .take()
            .ok_or(SchedulerError::MissingGatePipe("stdout"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or(SchedulerError::MissingGatePipe("stderr"))?;
        let stdout_task = tokio::spawn(capture_output(stdout));
        let stderr_task = tokio::spawn(capture_output(stderr));
        let started = Instant::now();
        let timeout = Duration::from_millis(definition.timeout_ms);
        let (status, timed_out) =
            if let Ok(status) = tokio::time::timeout(timeout, child.wait()).await {
                (status?, false)
            } else {
                child.kill().await?;
                (child.wait().await?, true)
            };
        let stdout = stdout_task
            .await
            .map_err(|error| SchedulerError::GateCapture(error.to_string()))??;
        let stderr = stderr_task
            .await
            .map_err(|error| SchedulerError::GateCapture(error.to_string()))??;
        let duration_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
        Ok(GateExecution {
            status: if !timed_out && status.success() {
                GateStatus::Passed
            } else {
                GateStatus::Failed
            },
            exit_code: status.code(),
            duration_ms,
            stdout: stdout.text,
            stderr: stderr.text,
            output_truncated: stdout.truncated || stderr.truncated,
            timed_out,
        })
    }
}

#[derive(Debug)]
struct CapturedOutput {
    text: String,
    truncated: bool,
}

async fn capture_output(
    mut reader: impl AsyncRead + Unpin,
) -> Result<CapturedOutput, std::io::Error> {
    let mut buffer = [0_u8; 8 * 1024];
    let mut captured = Vec::new();
    let mut truncated = false;
    loop {
        let count = reader.read(&mut buffer).await?;
        if count == 0 {
            break;
        }
        let remaining = MAX_CAPTURE_BYTES.saturating_sub(captured.len());
        let copied = remaining.min(count);
        captured.extend_from_slice(&buffer[..copied]);
        truncated |= copied < count;
    }
    Ok(CapturedOutput {
        text: String::from_utf8_lossy(&captured).into_owned(),
        truncated,
    })
}

#[derive(Clone, Debug)]
pub struct GateCoordinator {
    store: SqliteStore,
    runner: GateRunner,
}

impl GateCoordinator {
    #[must_use]
    pub fn new(store: SqliteStore) -> Self {
        Self {
            store,
            runner: GateRunner,
        }
    }

    /// Persist and execute a gate, including failed spawn/capture evidence.
    ///
    /// # Errors
    ///
    /// Returns the gate execution or persistence error after recording a failed gate
    /// whenever persistence remains available.
    pub async fn run(
        &self,
        task_id: &EntityId,
        run_id: Option<&EntityId>,
        cwd: &Path,
        definition: &GateDefinition,
    ) -> Result<Gate, SchedulerError> {
        definition.validate()?;
        let mut gate = Gate {
            id: EntityId::new(),
            task_id: task_id.clone(),
            run_id: run_id.cloned(),
            name: definition.name.clone(),
            status: GateStatus::Pending,
            command: Some(serde_json::to_string(&json!({
                "program": definition.program,
                "args": definition.args,
            }))?),
            result: None,
            created_at: now(),
            completed_at: None,
        };
        self.store.upsert_gate(&gate)?;
        gate.status = GateStatus::Running;
        self.store.upsert_gate(&gate)?;

        match self.runner.run(cwd, definition).await {
            Ok(execution) => {
                gate.status = execution.status;
                gate.result = Some(serde_json::to_value(&execution)?);
                gate.completed_at = Some(now());
                self.store.upsert_gate(&gate)?;
                Ok(gate)
            }
            Err(error) => {
                gate.status = GateStatus::Failed;
                gate.result = Some(json!({ "error": error.to_string() }));
                gate.completed_at = Some(now());
                self.store.upsert_gate(&gate)?;
                Err(error)
            }
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", content = "gates", rename_all = "snake_case")]
pub enum GateEligibility {
    Eligible,
    Missing(Vec<String>),
    Pending(Vec<String>),
    Failed(Vec<String>),
}

#[must_use]
pub fn evaluate_required_gates(gates: &[Gate], required: &[String]) -> GateEligibility {
    let mut newest = BTreeMap::<&str, &Gate>::new();
    for gate in gates {
        match newest.get(gate.name.as_str()) {
            Some(existing) if existing.created_at >= gate.created_at => {}
            _ => {
                newest.insert(gate.name.as_str(), gate);
            }
        }
    }
    let mut missing = Vec::new();
    let mut pending = Vec::new();
    let mut failed = Vec::new();
    for name in required {
        match newest.get(name.as_str()).map(|gate| gate.status) {
            None => missing.push(name.clone()),
            Some(GateStatus::Passed) => {}
            Some(GateStatus::Pending | GateStatus::Running) => pending.push(name.clone()),
            Some(GateStatus::Failed | GateStatus::Skipped) => failed.push(name.clone()),
        }
    }
    if !failed.is_empty() {
        GateEligibility::Failed(failed)
    } else if !missing.is_empty() {
        GateEligibility::Missing(missing)
    } else if !pending.is_empty() {
        GateEligibility::Pending(pending)
    } else {
        GateEligibility::Eligible
    }
}
