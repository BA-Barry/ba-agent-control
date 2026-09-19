# Product backlog

This backlog records product gaps observed during the personal Alpha acceptance and first-use walkthrough. It is intentionally outcome-oriented: entries describe a user-visible failure or missing capability rather than creating new release gates.

## Iteration progress: project onboarding

**Progress (2026-08-30): complete locally.** The Projects surface now distinguishes importing an existing Git root from creating a new local repository. Both paths use one application service across the console, CLI, REST API, and MCP. Existing project configuration is preserved during import; create mode refuses non-empty directories and rolls back only its own artifacts on failure. The flow explicitly stops at local registration and does not create a GitHub repository, remote, push, tag, or release.

## Next iteration: complete the daily task loop

### 1. Make roles usable from the console

**Progress (2026-08-30): complete.** Task creation and workflow templates persist an Agent profile assignment, the console shows it before launch, and dispatch reuses it by default while allowing a compatible human override. Write tasks offer only write-capable profiles, all eight built-in profiles are available through `adapter profiles`, and assignments survive restart through SQLite.

**Target outcome:**

- Add a role selector to task creation and task detail.
- Recommend a role from read/write intent and task purpose while leaving the final choice to the user.
- Display the assigned role in task and run views.
- Add a supported CLI command for listing Agent profiles.

### 2. Finish the console-only execution path

**Progress (2026-08-29):** complete for the personal Alpha. Eligible task rows can open a role-aware dispatch panel, and the quality surface can execute or retry the selected project's configured gates with live feedback. Write-task gates now execute in the originating run worktree instead of the project checkout.

**Target outcome:**

- Add `派发` / `Dispatch` to eligible task rows and task detail.
- Add configured gate execution, progress, retry, and evidence controls to the quality surface.
- Clearly explain why a task is not dispatchable: dependency, concurrency, write scope, runtime, or status.
- Keep protected remote actions in the existing human approval queue.

### 3. Wire and expose concurrency policy

**Progress (2026-08-30):** complete. The live scheduler reads a SQLite-backed global maximum (default four), resolves each project's `.ba-agent/project.toml` maximum, applies the lower effective value, and exposes both through CLI, REST, MCP, and Settings. Atomic admission tests with independent SQLite connections prove two control processes cannot exceed the shared limit.

**Target outcome:**

- Load and validate the configured concurrency value in the live dispatch path.
- Define whether the limit is global, per project, or both, and show the effective value in Settings.
- Preserve the default of four for the personal Alpha.
- Add real runtime tests proving a non-default value is honored.

### 4. Queue capacity overflow instead of rejecting it

**Progress (2026-08-30):** complete. Overflow work persists as ordered Dispatch/Run records, starts automatically when dependencies and capacity allow, creates write worktrees only after admission, and can be reordered or cancelled from CLI, REST, MCP, and the console. Terminal transitions, cancellations, dependency completion, and policy changes all refill available slots.

**Target outcome:**

- Allow an eligible fifth task to wait durably for capacity.
- Start it in dependency and submission order when a slot becomes available.
- Allow the user to reorder or cancel waiting work.
- Continue to serialize overlapping write scopes.

## Next iteration: make multi-Agent work materially better than a single chat

### 5. Add workflow templates and explicit handoffs

**Progress (2026-08-30):** complete. Six built-in and project-overridable templates create an atomic assigned task graph through CLI, REST, MCP, and the console. Task-level role assignments survive restarts and become the dispatch default. Structured repository-owned handoffs are limited to direct downstream dependencies, verified when read, visible in the console, and injected on launch and recovery.

**Target outcome:** provide editable templates for feature delivery, defect investigation, security review, migration, release preparation, and documentation verification. Each template should create narrow tasks, role assignments, dependencies, acceptance criteria, and declared write scopes. Persist structured handoffs so downstream Agents receive upstream findings without copying chat text manually.

### 6. Add a controlled review-and-repair loop

**Progress (2026-08-31): complete locally.** A SQLite-backed loop now creates assigned Implementer → Test Engineer → Reviewer tasks atomically, refreshes its state from durable Task records, and waits for a recorded local-user decision after every review. Accept creates a separate confirmation task; repair creates a fresh three-task stage up to the configured one-to-three-round limit; block, failed work, and an exhausted limit stop the loop without rewriting evidence. CLI, REST, MCP, and the bilingual console share the service, and desktop/mobile Playwright coverage exercises creation and an explicit repair decision.

**Target outcome:** let the user approve a finite loop such as implement → test → review → repair → confirm. Every iteration must remain a distinct Run with preserved evidence; the loop must stop on success, iteration limit, blocked decision, or failed gate.

### 7. Reduce safe read-only approval noise and recover after one rejected tool call

**Progress (2026-09-16): activated on the real local service.** Runtime approvals resolve independently from protected Git approvals. Rejecting or cancelling one App Server command or MCP tool call returns the Run to `running` when no other approval remains, so the Agent can report a safe alternative and the runtime decides the eventual terminal state. The console now exposes both one-turn and current-session decisions for live runtime requests, labels command, MCP, and file-change requests distinctly, and explains the scope beside the decision. Rejected push, merge, deploy, delete, elevation, and other protected operations still terminate the Run and cancel sibling approvals; the scheduler rejects `approved_for_session` for every protected request even if a client bypasses the console. Duplicate responses are rejected before reaching the runtime.

Scheduler tests cover protected rejection, protected session-scope refusal, runtime rejection recovery, session-decision forwarding, and duplicate-response suppression. Desktop and mobile browser tests confirm that the session action appears only for runtime requests, posts the explicit decision, remains operable with keyboard and assistive technology, and does not appear for protected Git requests. Durable task `01M0SKWJKQHM8MVWC7H3PAP1YZ` remains the historical planning record and is now archived without manufacturing a completed lifecycle.

**Protocol correction (2026-09-17):** session reuse is limited to command/file-change requests supported by the runtime and their explicit `availableDecisions`. MCP elicitation has no session decision and permission elevation remains per-call. The server computes the capability used by the console and records the precise runtime decision in durable evidence. The earlier request-ID-only check was insufficient; mocked forwarding alone did not establish MCP session reuse.

**Target outcome:** group narrowly equivalent read-only approvals where the runtime supports it, preserve per-command evidence, and allow the Agent to continue after a declined tool call unless the declined operation is essential.

## Operational clarity

### 8. Reconcile obsolete durable tasks without rewriting history

**Progress (2026-09-16): activated on the real schema-v5 database.** Archive, supersede, and restore are separate append-only task disposition records rather than lifecycle states. The active console, REST, CLI, and MCP lists hide archived or superseded tasks by default while scheduler dependency checks retain the complete graph. Every decision requires a reason; supersede also requires a same-project replacement. Queued, running, review, actively dispatched, and still-required upstream tasks are rejected. Restore makes the original Task visible again without changing its status, Runs, events, gates, receipts, or dependencies. SQLite immediate transactions prevent concurrent disposition heads. Desktop/mobile browser tests exercise supersede, history inspection, and restore; a real reversible archive/restore round trip preserved the original draft state, database integrity, and Receipt chain after the v4→v5 migration.

**Current gap:** several early Alpha tasks remain draft or blocked even though their work was completed outside those original Runs. Directly marking them completed would manufacture lifecycle evidence, while leaving them mixed with live work confuses the backlog.

**Target outcome:** provide an explicit archive/supersede action with a reason and replacement reference. Preserve original states and events while hiding superseded entries from the default active view.

### 9. Improve service restart and session recovery messaging

**Progress (2026-08-29):** complete within the technical boundary of a loopback process. Running `ui` again authenticates and reopens a live service or starts a replacement; `doctor` distinguishes `running`, `stopped`, and `stale`; an invalid token receives a bilingual no-store recovery page; an unrelated listener is not trusted; and an explicitly occupied port has a precise error. A page whose old port has no listener cannot execute recovery code, so its documented recovery action is to rerun `ui`, which opens the new authenticated URL.

**Current gap:** an expired browser session can remain open after the local service stops, and a briefly stale control descriptor can make a requested port appear occupied even when no listener remains.

**Target outcome:** distinguish expired session, stopped service, stale descriptor, and occupied port; offer a safe restart action and redirect the browser to the newly authenticated local session.

### 10. Make terminal workflow nodes conclude without an empty handoff

**Progress (2026-08-30):** addressed in the managed Codex prompt. Before each launch or recovery, BA Agent derives whether the Task has a direct downstream dependency. A terminal Task is explicitly told to report its result and not call `handoff.create` or create a placeholder Task. Tasks with a direct downstream retain the existing structured-handoff instruction.

**Observed evidence:** during the live MCP approval bridge acceptance, a Planner successfully verified the incoming handoff but then attempted an outgoing `handoff.create` with an empty destination list. BA Agent rejected that invalid request and preserved the resulting failed Run instead of manufacturing success. The prompt refinement prevents the same terminal-node mistake in future managed launches.

## Next correctness priority: downstream code visibility

**Observed (2026-09-17):** `WorktreeManager::create_detached` always resolves the project checkout's `HEAD`; dispatch does not select a completed dependency's commit. Read-only runs default to the project checkout. An Implementer can therefore produce a successful isolated commit while the dependent Test Engineer or Reviewer starts on the earlier checkout. Structured handoff text conveys findings but does not itself select the code being tested.

**Required outcome:** downstream testing and review must inspect the exact upstream implementation, and a repair round must begin with the prior round's changes. Resolve and persist the selected existing Git revision, use isolated task worktrees, and surface conflicting dependency branches for an explicit integration decision. Preserve the current human approval boundary for merging into the user's branch. Prove the behavior with a real temporary repository where only the upstream commit contains the changed file, then run the live review/repair acceptance. This is a correctness gap to address before claiming the multi-Agent delivery loop is complete.

**Progress (2026-09-19): implemented and proven locally.** Dispatch now traverses completed dependencies, accepts only quality-gated immutable commit receipts, and selects a commit that contains every upstream result. Dependent write and read tasks both receive isolated attempt worktrees; readers use an empty write scope plus the runtime read-only sandbox. The Run records `workspace.selected`, gates and finalization are bound to the originating Run, and review receipts preserve the inspected commit. Divergent or missing write evidence blocks dispatch unless the repository checkout already contains every input through an explicit integration. A real temporary-Git integration test executes implement → test → review → repair → test → review → confirmation, reopens SQLite before confirmation, and proves each stage sees the prior commit while the project checkout remains unchanged. The remaining acceptance item is a real-model execution of this already deterministic path, not another scheduler design change.

## Role-extension rule

Do not add a new Agent profile merely for a new topic. Prefer a Skill or workflow template when the existing role, permissions, and review perspective remain correct. Add a role only after repeated tasks demonstrate a distinct durable responsibility, permission boundary, or evaluation standard. Increasing the number of roles must not be presented as increasing runtime concurrency.
