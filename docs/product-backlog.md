# Product backlog

This backlog records product gaps observed during the personal Alpha acceptance and first-use walkthrough. It is intentionally outcome-oriented: entries describe a user-visible failure or missing capability rather than creating new release gates.

## Next iteration: complete the daily task loop

### 1. Make roles usable from the console

**Current gap:** BA Agent persists eight built-in Agent profiles, but the task form does not let the user choose one, the console does not recommend a profile, and profile identifiers are not discoverable through the normal CLI. Dispatch without `--agent-profile-id` launches a generic Codex run.

**Target outcome:**

- Add a role selector to task creation and task detail.
- Recommend a role from read/write intent and task purpose while leaving the final choice to the user.
- Display the assigned role in task and run views.
- Add a supported CLI command for listing Agent profiles.

### 2. Finish the console-only execution path

**Current gap:** the console can create tasks and inspect runs, approvals, gates, and receipts, but starting a dispatch and running configured quality gates still require CLI commands. This makes the Alpha unnecessarily expensive to operate for non-CLI users.

**Target outcome:**

- Add `派发` / `Dispatch` to eligible task rows and task detail.
- Add configured gate execution, progress, retry, and evidence controls to the quality surface.
- Clearly explain why a task is not dispatchable: dependency, concurrency, write scope, runtime, or status.
- Keep protected remote actions in the existing human approval queue.

### 3. Wire and expose concurrency policy

**Current gap:** `.ba-agent/project.toml` declares `max_concurrency`, and the scheduler supports an override type, but the live dispatch path currently constructs the default four-run policy directly. Editing project configuration therefore does not change the effective limit.

**Target outcome:**

- Load and validate the configured concurrency value in the live dispatch path.
- Define whether the limit is global, per project, or both, and show the effective value in Settings.
- Preserve the default of four for the personal Alpha.
- Add real runtime tests proving a non-default value is honored.

### 4. Queue capacity overflow instead of rejecting it

**Current gap:** when four runs are active, a fifth dispatch is rejected and must be manually retried. The Task state model already has `queued`, but there is no durable waiting queue for capacity overflow.

**Target outcome:**

- Allow an eligible fifth task to wait durably for capacity.
- Start it in dependency and submission order when a slot becomes available.
- Allow the user to reorder or cancel waiting work.
- Continue to serialize overlapping write scopes.

## Next iteration: make multi-Agent work materially better than a single chat

### 5. Add workflow templates and explicit handoffs

**Current gap:** users can model dependencies, but must manually decide which roles to use, dispatch each task, and carry findings between runs. The Coordinator profile does not yet orchestrate the other profiles automatically.

**Target outcome:** provide editable templates for feature delivery, defect investigation, security review, migration, release preparation, and documentation verification. Each template should create narrow tasks, role assignments, dependencies, acceptance criteria, and declared write scopes. Persist structured handoffs so downstream Agents receive upstream findings without copying chat text manually.

### 6. Add a controlled review-and-repair loop

**Current gap:** an Implementer, Reviewer, and Test Engineer can run separately, but BA Agent does not yet turn findings into a bounded repair task and re-review cycle.

**Target outcome:** let the user approve a finite loop such as implement → test → review → repair → confirm. Every iteration must remain a distinct Run with preserved evidence; the loop must stop on success, iteration limit, blocked decision, or failed gate.

### 7. Reduce safe read-only approval noise and recover after one rejected tool call

**Current gap:** the first self-hosted Reviewer run required repeated Windows read-only command approvals, and one rejected tool call terminated the whole run instead of allowing a safe alternative. Durable task `01M0SKWJKQHM8MVWC7H3PAP1YZ` already represents this defect.

**Target outcome:** group narrowly equivalent read-only approvals where the runtime supports it, preserve per-command evidence, and allow the Agent to continue after a declined tool call unless the declined operation is essential.

## Operational clarity

### 8. Reconcile obsolete durable tasks without rewriting history

**Current gap:** several early Alpha tasks remain draft or blocked even though their work was completed outside those original Runs. Directly marking them completed would manufacture lifecycle evidence, while leaving them mixed with live work confuses the backlog.

**Target outcome:** provide an explicit archive/supersede action with a reason and replacement reference. Preserve original states and events while hiding superseded entries from the default active view.

### 9. Improve service restart and session recovery messaging

**Current gap:** an expired browser session can remain open after the local service stops, and a briefly stale control descriptor can make a requested port appear occupied even when no listener remains.

**Target outcome:** distinguish expired session, stopped service, stale descriptor, and occupied port; offer a safe restart action and redirect the browser to the newly authenticated local session.

## Role-extension rule

Do not add a new Agent profile merely for a new topic. Prefer a Skill or workflow template when the existing role, permissions, and review perspective remain correct. Add a role only after repeated tasks demonstrate a distinct durable responsibility, permission boundary, or evaluation standard. Increasing the number of roles must not be presented as increasing runtime concurrency.
