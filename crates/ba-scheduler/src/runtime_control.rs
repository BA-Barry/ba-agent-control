use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write as _,
    path::{Path, PathBuf},
    sync::Arc,
};

use ba_core::{
    AgentProfile, Approval, ApprovalStatus, EntityId, Run, RunEvent, RunStatus, Task, TaskStatus,
    now,
};
use ba_runtime::{
    ApprovalDecision, ResumeRequest, RunRequest, RuntimeAdapter, RuntimeEvent, RuntimeEventKind,
    RuntimeTransport, SandboxMode, StdioMcpServer,
};
use ba_store::{SqliteStore, TerminalRunUpdate};
use serde_json::{Value, json};
use tokio::sync::{Mutex, broadcast};

use crate::{
    ApprovalCoordinator, ApprovalResolution, DispatchBundle, DispatchCoordinator, HandoffArtifact,
    HandoffService, Scheduler, SchedulerConfig, SchedulerError, SchedulerPolicyService,
    handoff::handoff_prompt_section,
};

/// Shared runtime lifecycle used by HTTP, MCP, and CLI control surfaces.
#[derive(Clone, Debug)]
pub struct RuntimeCoordinator {
    store: SqliteStore,
    runtime: Arc<dyn RuntimeAdapter>,
    events: broadcast::Sender<RunEvent>,
    worktree_root: Option<PathBuf>,
    managed_mcp_server: Option<StdioMcpServer>,
    drain_lock: Arc<Mutex<()>>,
    approval_lock: Arc<Mutex<()>>,
}

#[derive(Debug)]
struct RecoveryLaunch {
    task: Task,
    request: ResumeRequest,
}

impl RuntimeCoordinator {
    #[must_use]
    pub fn new(store: SqliteStore, runtime: Arc<dyn RuntimeAdapter>) -> Self {
        let (events, _receiver) = broadcast::channel(2_048);
        Self::with_event_sender(store, runtime, events)
    }

    #[must_use]
    pub fn with_event_sender(
        store: SqliteStore,
        runtime: Arc<dyn RuntimeAdapter>,
        events: broadcast::Sender<RunEvent>,
    ) -> Self {
        Self {
            store,
            runtime,
            events,
            worktree_root: None,
            managed_mcp_server: None,
            drain_lock: Arc::new(Mutex::new(())),
            approval_lock: Arc::new(Mutex::new(())),
        }
    }

    /// Enable durable queue draining and just-in-time worktree creation.
    #[must_use]
    pub fn with_queue(mut self, worktree_root: PathBuf) -> Self {
        self.worktree_root = Some(worktree_root);
        self
    }

    /// Inject the trusted local BA Agent MCP server into every managed runtime.
    #[must_use]
    pub fn with_managed_mcp_server(mut self, binary: PathBuf, data_directory: &Path) -> Self {
        self.managed_mcp_server = Some(StdioMcpServer {
            name: "ba_agent".into(),
            command: binary,
            args: vec![
                "--data-dir".into(),
                data_directory.to_string_lossy().into_owned(),
                "mcp".into(),
            ],
        });
        self
    }

    #[must_use]
    pub fn subscribe_events(&self) -> broadcast::Receiver<RunEvent> {
        self.events.subscribe()
    }

    /// Persist and publish one ordered domain event.
    ///
    /// # Errors
    ///
    /// Returns an error when the event cannot be persisted.
    pub fn append_event(
        &self,
        run_id: &EntityId,
        kind: impl Into<String>,
        payload: Value,
    ) -> Result<RunEvent, SchedulerError> {
        let event = self.store.append_run_event(run_id, kind, payload)?;
        let _receiver_count = self.events.send(event.clone());
        Ok(event)
    }

    /// Start a queued dispatch in the runtime and bridge all events into durable state.
    ///
    /// # Errors
    ///
    /// Returns an error when entities are missing, the working directory is invalid,
    /// or the runtime launch fails.
    pub async fn launch(
        &self,
        bundle: DispatchBundle,
        prompt_override: Option<String>,
    ) -> Result<DispatchBundle, SchedulerError> {
        let claimed = if self.worktree_root.is_some() {
            let task = self.store.get_task(&bundle.run.task_id)?.ok_or_else(|| {
                SchedulerError::NotSchedulable("task disappeared before admission".into())
            })?;
            let project = self.store.get_project(&task.project_id)?.ok_or_else(|| {
                SchedulerError::NotSchedulable("project disappeared before admission".into())
            })?;
            let policy = SchedulerPolicyService::new(self.store.clone());
            let global = policy.global()?.global_max_concurrency;
            let project = policy.project(&project)?.effective_max_concurrency;
            self.claim_bundle_with_policy(bundle, global, project)?
        } else {
            self.claim_bundle(bundle, |_run, _task, _active_runs, _tasks| true)?
        };
        let bundle = claimed
            .ok_or_else(|| SchedulerError::AlreadyActive("run was already claimed".into()))?;
        self.launch_claimed(bundle, prompt_override).await
    }

    #[allow(
        clippy::too_many_lines,
        reason = "runtime launch keeps the claimed run, worktree, adapter, and event transition sequence visible in one lifecycle"
    )]
    async fn launch_claimed(
        &self,
        mut bundle: DispatchBundle,
        prompt_override: Option<String>,
    ) -> Result<DispatchBundle, SchedulerError> {
        let task = self.store.get_task(&bundle.run.task_id)?.ok_or_else(|| {
            SchedulerError::NotSchedulable("task disappeared after dispatch".into())
        })?;
        let project = self.store.get_project(&task.project_id)?.ok_or_else(|| {
            SchedulerError::NotSchedulable("project disappeared after dispatch".into())
        })?;
        let profile = bundle
            .run
            .agent_profile_id
            .as_ref()
            .map(|id| self.store.get_agent_profile(id))
            .transpose()?
            .flatten();
        if bundle.worktree.is_none()
            && let Some(worktree_root) = &self.worktree_root
        {
            bundle.worktree =
                match DispatchCoordinator::new(self.store.clone(), worktree_root.clone())
                    .activate_worktree(&task, &bundle.run.id)
                {
                    Ok(worktree) => worktree,
                    Err(error) => {
                        self.finish_run(
                            &bundle.run.id,
                            RunStatus::Failed,
                            "worktree.create_failed",
                            json!({"error": error.to_string()}),
                        )?;
                        return Err(error);
                    }
                };
        }
        let cwd = bundle.worktree.as_ref().map_or_else(
            || PathBuf::from(&project.root_path),
            |worktree| PathBuf::from(&worktree.path),
        );
        let cwd = cwd.canonicalize()?;
        if let Some(worktree) = &bundle.worktree {
            self.append_event(
                &bundle.run.id,
                "workspace.selected",
                json!({
                    "worktreeId": worktree.id,
                    "baseRevision": worktree.base_revision,
                    "dependsOn": task.depends_on,
                    "readOnly": !task.write,
                }),
            )?;
        }
        let handoffs = HandoffService::new(self.store.clone()).list_for_task(&task.id)?;
        let has_direct_downstream = self
            .store
            .list_tasks(&task.project_id)?
            .iter()
            .any(|candidate| candidate.depends_on.contains(&task.id));
        let prompt = compose_runtime_prompt(
            &task,
            &bundle.run,
            profile.as_ref(),
            &handoffs,
            prompt_override.as_deref(),
            self.managed_mcp_server.is_some(),
            has_direct_downstream,
        );
        let environment = managed_run_environment(&task, &bundle.run)?;
        let mut runtime_events = self.runtime.subscribe();

        let launched = match self
            .runtime
            .launch(RunRequest {
                prompt,
                cwd,
                sandbox: if task.write {
                    SandboxMode::WorkspaceWrite
                } else {
                    SandboxMode::ReadOnly
                },
                environment,
                mcp_servers: self.managed_mcp_server.iter().cloned().collect(),
            })
            .await
        {
            Ok(handle) => handle,
            Err(error) => {
                self.finish_run(
                    &bundle.run.id,
                    RunStatus::Failed,
                    "runtime.launch_failed",
                    json!({"error": error.to_string()}),
                )?;
                return Err(error.into());
            }
        };

        self.store.transition_task(&task.id, TaskStatus::Running)?;
        let mut run = self
            .store
            .transition_run(&bundle.run.id, RunStatus::Running)?;
        run.runtime_run_id = Some(launched.run_id.clone());
        run.runtime_thread_id
            .clone_from(&launched.runtime_thread_id);
        run.started_at = Some(now());
        run.adapter = match launched.transport {
            RuntimeTransport::AppServer => "codex-app-server",
            RuntimeTransport::ExecJson => "codex-exec-json",
        }
        .into();
        self.store.upsert_run(&run)?;
        bundle.run = run;
        bundle.dispatch.status = "running".into();
        bundle.dispatch.updated_at = now();
        self.store.upsert_dispatch(&bundle.dispatch)?;
        self.append_event(
            &bundle.run.id,
            "runtime.started",
            json!({
                "runtimeRunId": launched.run_id,
                "runtimeThreadId": launched.runtime_thread_id,
                "transport": launched.transport,
                "degradedReason": launched.degraded_reason
            }),
        )?;

        let coordinator = self.clone();
        let domain_run_id = bundle.run.id.clone();
        let runtime_run_id = launched.run_id;
        tokio::spawn(async move {
            coordinator
                .bridge_events(domain_run_id, runtime_run_id, &mut runtime_events)
                .await;
        });
        Ok(bundle)
    }

    fn claim_bundle<F>(
        &self,
        mut bundle: DispatchBundle,
        predicate: F,
    ) -> Result<Option<DispatchBundle>, SchedulerError>
    where
        F: FnOnce(&Run, &Task, &[Run], &[Task]) -> bool,
    {
        let Some(run) = self.store.claim_queued_run_if(&bundle.run.id, predicate)? else {
            return Ok(None);
        };
        bundle.run = run;
        Ok(Some(bundle))
    }

    fn claim_bundle_with_policy(
        &self,
        bundle: DispatchBundle,
        global_limit: usize,
        project_limit: usize,
    ) -> Result<Option<DispatchBundle>, SchedulerError> {
        self.claim_bundle(bundle, move |_run, task, active_runs, tasks| {
            if active_runs.len() >= global_limit {
                return false;
            }
            let tasks_by_id = tasks
                .iter()
                .map(|item| (item.id.clone(), item))
                .collect::<BTreeMap<_, _>>();
            let project_active = active_runs
                .iter()
                .filter_map(|run| tasks_by_id.get(&run.task_id))
                .filter(|active| active.project_id == task.project_id)
                .count();
            if project_active >= project_limit
                || task.depends_on.iter().any(|dependency_id| {
                    tasks_by_id
                        .get(dependency_id)
                        .is_none_or(|dependency| dependency.status != TaskStatus::Completed)
                })
            {
                return false;
            }
            let Ok(mut scheduler) = Scheduler::new(SchedulerConfig {
                max_concurrency: global_limit,
            }) else {
                return false;
            };
            let mut restored = BTreeSet::new();
            for active_run in active_runs {
                if !restored.insert(active_run.task_id.clone()) {
                    continue;
                }
                let Some(active_task) = tasks_by_id.get(&active_run.task_id) else {
                    return false;
                };
                if scheduler.restore(active_task).is_err() {
                    return false;
                }
            }
            let mut candidate = task.clone();
            candidate.status = TaskStatus::Ready;
            let project_tasks = tasks
                .iter()
                .filter(|item| item.project_id == task.project_id)
                .cloned()
                .collect::<Vec<_>>();
            scheduler.admit(&candidate, &project_tasks).is_ok()
        })
    }

    /// Claim and start every queued dispatch currently allowed by durable policy.
    ///
    /// # Errors
    ///
    /// Returns an error when policy/configuration reads, SQLite claims, worktree
    /// creation, or runtime launch coordination fail.
    pub async fn drain_queue(&self) -> Result<Vec<DispatchBundle>, SchedulerError> {
        if self.worktree_root.is_none() {
            return Ok(Vec::new());
        }
        let _guard = self.drain_lock.lock().await;
        let mut launched = Vec::new();
        loop {
            let policy = SchedulerPolicyService::new(self.store.clone()).global()?;
            let mut active_runs = Vec::new();
            for status in [
                RunStatus::Starting,
                RunStatus::Running,
                RunStatus::WaitingApproval,
                RunStatus::Orphaned,
            ] {
                active_runs.extend(self.store.list_runs_by_status(status)?);
            }
            if active_runs.len() >= policy.global_max_concurrency {
                break;
            }

            let mut selected = None;
            for dispatch in self.store.list_dispatches_by_status("queued")? {
                let Some(run) = self.store.get_run(&dispatch.run_id)? else {
                    continue;
                };
                if run.status != RunStatus::Queued {
                    continue;
                }
                let Some(task) = self.store.get_task(&dispatch.task_id)? else {
                    continue;
                };
                let Some(project) = self.store.get_project(&task.project_id)? else {
                    continue;
                };
                let project_policy =
                    SchedulerPolicyService::new(self.store.clone()).project(&project)?;
                let bundle = DispatchBundle {
                    dispatch,
                    run,
                    worktree: None,
                };
                if let Some(bundle) = self.claim_bundle_with_policy(
                    bundle,
                    policy.global_max_concurrency,
                    project_policy.effective_max_concurrency,
                )? {
                    selected = Some(bundle);
                    break;
                }
            }
            let Some(bundle) = selected else {
                break;
            };
            let prompt = bundle.dispatch.prompt.clone();
            match self.launch_claimed(bundle, prompt).await {
                Ok(bundle) => launched.push(bundle),
                Err(error) => {
                    tracing::error!(error = %error, "queued dispatch failed to launch");
                }
            }
        }
        Ok(launched)
    }

    /// Resume one orphaned domain run from its persisted Codex thread.
    ///
    /// # Errors
    ///
    /// Returns an error when recovery metadata is missing or Codex cannot resume
    /// the thread. Failed attempts remain orphaned and blocked.
    pub async fn resume(
        &self,
        run_id: &EntityId,
        prompt_override: Option<String>,
    ) -> Result<Run, SchedulerError> {
        let RecoveryLaunch { task, request } =
            self.prepare_recovery(run_id, prompt_override.as_deref())?;
        let mut runtime_events = self.runtime.subscribe();
        let resumed = match self.runtime.resume(request).await {
            Ok(handle) => handle,
            Err(error) => {
                self.append_event(
                    run_id,
                    "recovery.resume_failed",
                    json!({"error": error.to_string()}),
                )?;
                return Err(error.into());
            }
        };

        let mut run = self.store.transition_run(run_id, RunStatus::Running)?;
        if task.status == TaskStatus::Blocked {
            self.store.transition_task(&task.id, TaskStatus::Running)?;
        }
        run.runtime_run_id = Some(resumed.run_id.clone());
        run.runtime_thread_id.clone_from(&resumed.runtime_thread_id);
        run.adapter = match resumed.transport {
            RuntimeTransport::AppServer => "codex-app-server",
            RuntimeTransport::ExecJson => "codex-exec-json",
        }
        .into();
        run.updated_at = now();
        self.store.upsert_run(&run)?;
        self.store.update_dispatch_status(run_id, "running")?;
        self.append_event(
            run_id,
            "recovery.resumed",
            json!({
                "runtimeRunId": resumed.run_id,
                "runtimeThreadId": resumed.runtime_thread_id,
                "transport": resumed.transport,
                "degradedReason": resumed.degraded_reason
            }),
        )?;

        let coordinator = self.clone();
        let domain_run_id = run.id.clone();
        let runtime_run_id = resumed.run_id;
        tokio::spawn(async move {
            coordinator
                .bridge_events(domain_run_id, runtime_run_id, &mut runtime_events)
                .await;
        });
        Ok(run)
    }

    fn prepare_recovery(
        &self,
        run_id: &EntityId,
        prompt_override: Option<&str>,
    ) -> Result<RecoveryLaunch, SchedulerError> {
        let run = self
            .store
            .get_run(run_id)?
            .ok_or_else(|| SchedulerError::NotActive(run_id.to_string()))?;
        if run.status != RunStatus::Orphaned {
            return Err(SchedulerError::NotActive(format!(
                "run {run_id} is not orphaned"
            )));
        }
        let thread_id = run
            .runtime_thread_id
            .clone()
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| SchedulerError::NotActive("run has no resumable thread id".into()))?;
        let task = self
            .store
            .get_task(&run.task_id)?
            .ok_or_else(|| SchedulerError::NotActive("recovery task not found".into()))?;
        let project = self
            .store
            .get_project(&task.project_id)?
            .ok_or_else(|| SchedulerError::NotActive("recovery project not found".into()))?;
        let profile = run
            .agent_profile_id
            .as_ref()
            .map(|id| self.store.get_agent_profile(id))
            .transpose()?
            .flatten();
        let worktree = self
            .store
            .list_worktrees(&task.project_id)?
            .into_iter()
            .find(|candidate| candidate.run_id.as_ref() == Some(run_id));
        let cwd = worktree.map_or_else(
            || PathBuf::from(project.root_path),
            |worktree| PathBuf::from(worktree.path),
        );
        let handoffs = HandoffService::new(self.store.clone()).list_for_task(&task.id)?;
        let has_direct_downstream = self
            .store
            .list_tasks(&task.project_id)?
            .iter()
            .any(|candidate| candidate.depends_on.contains(&task.id));
        let prompt = format!(
            "恢复先前中断的 BA Agent 任务。\n\n{}",
            compose_runtime_prompt(
                &task,
                &run,
                profile.as_ref(),
                &handoffs,
                prompt_override,
                self.managed_mcp_server.is_some(),
                has_direct_downstream,
            )
        );
        let request = ResumeRequest {
            thread_id,
            prompt,
            cwd: cwd.canonicalize()?,
            sandbox: if task.write {
                SandboxMode::WorkspaceWrite
            } else {
                SandboxMode::ReadOnly
            },
            environment: managed_run_environment(&task, &run)?,
            mcp_servers: self.managed_mcp_server.iter().cloned().collect(),
        };
        Ok(RecoveryLaunch { task, request })
    }

    /// Deliver guidance to an active runtime and record the user action.
    ///
    /// # Errors
    ///
    /// Returns an error when the run is inactive or has no live runtime handle.
    pub async fn message(
        &self,
        run_id: &EntityId,
        message: &str,
    ) -> Result<RunEvent, SchedulerError> {
        let message = message.trim();
        if message.is_empty() {
            return Err(SchedulerError::NotActive("guidance is empty".into()));
        }
        let run = self.active_run(run_id)?;
        let runtime_run_id = run
            .runtime_run_id
            .as_deref()
            .ok_or_else(|| SchedulerError::NotActive("run has no live runtime handle".into()))?;
        self.runtime
            .guide(runtime_run_id, message.to_owned())
            .await?;
        self.append_event(
            run_id,
            "user.guidance",
            json!({"message": message, "delivery": "runtime"}),
        )
    }

    /// Interrupt an active runtime and persist cancellation immediately.
    ///
    /// # Errors
    ///
    /// Returns an error when the run is inactive or has no live runtime handle.
    pub async fn cancel(
        &self,
        run_id: &EntityId,
        actor: &str,
    ) -> Result<(Run, RunEvent), SchedulerError> {
        let current = self
            .store
            .get_run(run_id)?
            .ok_or_else(|| SchedulerError::NotActive(run_id.to_string()))?;
        if current.status == RunStatus::Queued {
            let update = self.finish_run(
                run_id,
                RunStatus::Cancelled,
                "run.cancel_requested",
                json!({"actor": actor, "queued": true}),
            )?;
            return Ok((update.run, update.event));
        }
        let current = self.active_run(run_id)?;
        let runtime_run_id = current
            .runtime_run_id
            .as_deref()
            .ok_or_else(|| SchedulerError::NotActive("run has no live runtime handle".into()))?;
        self.runtime.cancel(runtime_run_id).await?;
        let update = self.finish_run(
            run_id,
            RunStatus::Cancelled,
            "run.cancel_requested",
            json!({"actor": actor}),
        )?;
        Ok((update.run, update.event))
    }

    /// Respond to a runtime or policy approval and resolve its durable record.
    ///
    /// # Errors
    ///
    /// Returns an error when the approval or runtime session is unavailable.
    pub async fn respond_approval(
        &self,
        approval_id: &EntityId,
        resolution: ApprovalResolution,
        runtime_decision: ApprovalDecision,
        evidence: &Value,
    ) -> Result<(Approval, RunEvent), SchedulerError> {
        // Keep the runtime side effect and its durable CAS in one process-wide
        // critical section. The control descriptor already enforces one
        // service owner, while the store CAS remains the cross-process safety
        // boundary for recovery and defensive tests.
        let _approval_guard = self.approval_lock.lock().await;
        let pending = self
            .store
            .get_approval(approval_id)?
            .ok_or_else(|| SchedulerError::ApprovalAlreadyResolved(approval_id.to_string()))?;
        if pending.status != ApprovalStatus::Pending {
            return Err(SchedulerError::ApprovalAlreadyResolved(
                approval_id.to_string(),
            ));
        }
        if runtime_decision == ApprovalDecision::AcceptForSession
            && !crate::approval_supports_session(&pending)
        {
            return Err(SchedulerError::InvalidApprovalDecision(
                "this request does not support session approval; MCP elicitation, permission elevation and protected operations require an individual decision".into(),
            ));
        }
        let request_id = pending.request.get("request_id").and_then(Value::as_str);
        if let Some(request_id) = request_id {
            let run = self.active_run(&pending.run_id)?;
            let runtime_run_id = run.runtime_run_id.as_deref().ok_or_else(|| {
                SchedulerError::NotActive("approval run has no live runtime handle".into())
            })?;
            self.runtime
                .respond_approval(runtime_run_id, request_id, runtime_decision)
                .await?;
        }
        let approvals = ApprovalCoordinator::new(self.store.clone());
        let evidence = json!({"runtime_decision": runtime_decision, "user_evidence": evidence});
        let approval = if request_id.is_some() {
            approvals.resolve_runtime(approval_id, resolution, &evidence)?
        } else {
            approvals.resolve_managed(approval_id, resolution, &evidence)?
        };
        let terminal = if request_id.is_none() {
            match resolution {
                ApprovalResolution::Reject => Some(RunStatus::Failed),
                ApprovalResolution::Cancel => Some(RunStatus::Cancelled),
                ApprovalResolution::Approve => None,
            }
        } else {
            None
        };
        let event = if let Some(status) = terminal {
            self.finish_run(
                &approval.run_id,
                status,
                "approval.resolved",
                json!({"approval": &approval}),
            )?
            .event
        } else {
            self.append_event(
                &approval.run_id,
                "approval.resolved",
                json!({"approval": &approval}),
            )?
        };
        Ok((approval, event))
    }

    fn active_run(&self, run_id: &EntityId) -> Result<Run, SchedulerError> {
        let run = self
            .store
            .get_run(run_id)?
            .ok_or_else(|| SchedulerError::NotActive(run_id.to_string()))?;
        if !matches!(run.status, RunStatus::Running | RunStatus::WaitingApproval) {
            return Err(SchedulerError::NotActive(run_id.to_string()));
        }
        Ok(run)
    }

    async fn bridge_events(
        &self,
        domain_run_id: EntityId,
        runtime_run_id: String,
        receiver: &mut broadcast::Receiver<RuntimeEvent>,
    ) {
        loop {
            match receiver.recv().await {
                Ok(event) if event.run_id == runtime_run_id => {
                    let terminal =
                        matches!(event.name.as_str(), "succeeded" | "failed" | "cancelled");
                    if let Err(error) = self.persist_runtime_event(&domain_run_id, &event) {
                        tracing::error!(error = %error, run_id = %domain_run_id, "runtime event persistence failed");
                    }
                    if terminal {
                        break;
                    }
                }
                Ok(_) => {}
                Err(broadcast::error::RecvError::Lagged(skipped)) => {
                    let _ignored = self.append_event(
                        &domain_run_id,
                        "runtime.events_lagged",
                        json!({"skipped": skipped}),
                    );
                }
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    }

    fn persist_runtime_event(
        &self,
        run_id: &EntityId,
        event: &RuntimeEvent,
    ) -> Result<(), SchedulerError> {
        let kind = format!("runtime.{}", event.name);
        let payload = serde_json::to_value(event)?;
        let terminal = match event.name.as_str() {
            "succeeded" => Some(RunStatus::Succeeded),
            "failed" => Some(RunStatus::Failed),
            "cancelled" => Some(RunStatus::Cancelled),
            _ => None,
        };
        if let Some(status) = terminal {
            self.finish_run(run_id, status, kind, payload)?;
        } else {
            self.append_event(run_id, kind, payload)?;
        }
        if event.kind == RuntimeEventKind::ApprovalRequested {
            let request_id = event
                .payload
                .get("id")
                .map(|value| match value {
                    Value::String(value) => value.clone(),
                    other => other.to_string(),
                })
                .ok_or_else(|| {
                    SchedulerError::NotActive("approval event did not contain an id".into())
                })?;
            ApprovalCoordinator::new(self.store.clone()).request_runtime_operation(
                run_id,
                &event.name,
                &request_id,
                &event.payload,
            )?;
        }
        Ok(())
    }

    fn finish_run(
        &self,
        run_id: &EntityId,
        status: RunStatus,
        event_kind: impl Into<String>,
        event_payload: Value,
    ) -> Result<TerminalRunUpdate, SchedulerError> {
        let update = self
            .store
            .finish_run(run_id, status, event_kind, event_payload)?;
        let _receiver_count = self.events.send(update.event.clone());
        if self.worktree_root.is_some() {
            let coordinator = self.clone();
            tokio::spawn(async move {
                if let Err(error) = coordinator.drain_queue().await {
                    tracing::error!(error = %error, "durable queue drain failed");
                }
            });
        }
        Ok(update)
    }
}

fn compose_runtime_prompt(
    task: &Task,
    run: &Run,
    profile: Option<&AgentProfile>,
    handoffs: &[HandoffArtifact],
    prompt_override: Option<&str>,
    managed_mcp_available: bool,
    has_direct_downstream: bool,
) -> String {
    let mut prompt = prompt_override
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map_or_else(|| compose_task_prompt(task, profile), str::to_owned);
    if let Some(section) = handoff_prompt_section(handoffs) {
        prompt.push_str("\n\n");
        prompt.push_str(&section);
    }
    prompt.push_str("\n\n");
    let _write_result = write!(
        prompt,
        "BA Agent 托管上下文：project_id={}，task_id={}，run_id={}。",
        task.project_id, task.id, run.id
    );
    if managed_mcp_available {
        prompt.push_str(
            " 已注入名为 ba_agent 的 BA Agent MCP；读取或更新项目控制面时必须优先使用其结构化工具。",
        );
        if has_direct_downstream {
            prompt.push_str(
                " 当前任务存在直接下游任务；若任务要求交接，必须在结束前调用 handoff.create，并使用当前 task_id 作为 source_task_id、当前 run_id 作为 source_run_id。",
            );
        } else {
            prompt.push_str(
                " 当前任务没有直接下游任务；不得调用 handoff.create，也不得创建占位任务。完成本任务后直接报告结果。",
            );
        }
    }
    prompt
}

fn compose_task_prompt(task: &Task, profile: Option<&AgentProfile>) -> String {
    let mut sections = vec![format!("任务：{}", task.title)];
    if !task.description.trim().is_empty() {
        sections.push(format!("验收与说明：\n{}", task.description.trim()));
    }
    if task.write {
        sections.push(format!("允许写入范围：{}", task.path_scope.join(", ")));
    } else {
        sections.push("本任务为只读任务，不得修改仓库文件。".into());
    }
    if let Some(profile) = profile
        && !profile.instructions.trim().is_empty()
    {
        sections.push(format!(
            "角色：{} / {}\n{}",
            profile.name,
            profile.role,
            profile.instructions.trim()
        ));
    }
    sections.push("完成任务并提供可验证证据；遇到需要人工许可的操作时发起审批。".into());
    sections.join("\n\n")
}

fn managed_run_environment(
    task: &Task,
    run: &Run,
) -> Result<BTreeMap<String, String>, SchedulerError> {
    let allowed_paths = std::env::join_paths(task.path_scope.iter().map(PathBuf::from))?
        .to_string_lossy()
        .into_owned();
    Ok(BTreeMap::from([
        ("BA_AGENT_MANAGED_RUN".into(), "1".into()),
        ("BA_AGENT_PROJECT_ID".into(), task.project_id.to_string()),
        ("BA_AGENT_TASK_ID".into(), task.id.to_string()),
        ("BA_AGENT_RUN_ID".into(), run.id.to_string()),
        ("BA_AGENT_ALLOWED_PATHS".into(), allowed_paths),
        ("BA_AGENT_GATE_STATUS".into(), "pending".into()),
        ("BA_AGENT_OPERATION_APPROVED".into(), "0".into()),
    ]))
}
