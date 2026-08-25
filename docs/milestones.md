# Milestones

## M1 — Foundation

Single-binary server, migrations, empty console, plugin skeleton, validation, and three-platform CI.

Status: **complete locally (2026-08-24)**.

- Latest release Windows x64 binary: 15,843,328 bytes (15.11 MiB), embedded hashed console assets, `doctor=ready`.
- SQLite migration/integrity checks and loopback security tests pass.
- CI matrix covers Windows, Ubuntu, and macOS and builds the console before Rust embedding.
- Plugin bundle validates with 8 roles and 11 Skills; Hooks pass 15/15 tests.

## M2 — Core vertical slice

Register project → create task → dispatch Codex run → isolate worktree → stream events → run gates → issue receipt → create local commit → recover after restart.

Status: **complete locally (2026-08-24)**.

- A live ignored integration test completed a logged-in Codex App Server turn.
- Runtime lifecycle is shared by HTTP and MCP, including guidance, cancellation, approvals, SSE events, fallback metadata, and automatic thread resume.
- Write tasks persist detached worktree identity, base revision, and path scope.
- Configured argv gates finalize only eligible worktrees to `ba/<workstream>/<task-id>`, then append a permanent hash-chained receipt.
- Run, Task, Dispatch, and terminal Event updates are atomic and rollback-tested.

## M3 — Complete Codex control

App Server integration, four-way scheduling, roles, messages, approvals, hooks, optional GitHub adapter, and fallback/recovery behavior.

Status: **complete locally (2026-08-24)**.

- App Server control, Windows shim handling, protocol fixtures, exec fallback, messaging, approvals, cancellation, and restart recovery are implemented.
- Eight durable built-in roles and the complete Hook guardrail set are installed and validated.
- A live four-agent demonstration passed and blocked the fifth task at the configured concurrency limit.
- The optional GitHub adapter enforces `gh` → OS credential store → current-process `GITHUB_TOKEN` precedence, exposes read-only repository metadata, never serializes credentials, and has no remote mutation entry point.

## M4 — Personal Alpha console

All seven surfaces, Chinese/English, light/dark/system themes, responsive and accessible operation, followed by Impeccable and scoped Taste audits. BA Agent begins self-hosting its own backlog.

Status: **feature-complete locally; final self-hosting acceptance in progress (2026-08-25)**.

- The user approved the comp-first “测量员外业簿” direction. All seven console surfaces now use the shared field-book design system with Chinese/English, light/dark/system themes, responsive layouts, keyboard focus, and screen-reader semantics.
- Impeccable critique, audit, harden, and polish passes completed. Twenty Playwright scenarios cover all seven routes on desktop and Pixel 7 with zero axe violations, plus real milestone/task mutations, keyboard, loading, empty, disconnected, extreme-content, forced-color, 200% text and production Web Vitals checks.
- The embedded console now authenticates through the random local session, reads real `/api/v1` entities and worktree identity, consumes a forward-compatible `run-event` SSE envelope, and performs real project/task creation, approval, cancellation, guidance, retry/resume, and receipt-verification mutations with CSRF protection.
- Project milestones are now first-class REST and MCP resources. The console registers a real active milestone, replaces the previous active milestone atomically, derives progress from milestone-linked tasks, and attaches new tasks to the current milestone instead of displaying the project description as synthetic milestone state.
- Dependency-aware task creation is now consistent across the console, CLI, REST API and MCP: each entry point validates project ownership, optional milestone/workstream alignment, read/write intent and repository-relative write scopes. The console dependency diagram is derived from persisted relationships rather than sample ordering.
- When the local service is unavailable, the console visibly enters demo fallback mode and reports the cause instead of silently presenting sample records as live data.
- Production single-binary browser runs loaded real data on all seven routes, registered the BA Agent repository, created a workstream and scoped write task, completed receipt-chain verification, logged no browser errors, and showed no horizontal overflow at 390 × 844.
- Failed retries preserve their earlier detached worktree and allocate a new attempt-specific worktree. Orphaned runs resume their persisted thread; live runs with a Codex thread use the desktop app's registered `codex://threads/<id>` handoff.
- One continuous non-model Alpha acceptance test now registers three real Git repositories, admits four agent profiles while rejecting a fifth, isolates three writers, resolves an approval, passes gates, commits three workstream branches, reopens SQLite for orphan/thread recovery, prunes future-expired runtime data, and verifies every permanent receipt chain after another reopen.
- Self-hosting remains evidence-preserving: the earlier failed Reviewer run and its approval ergonomics issue remain in the backlog rather than being rewritten into a synthetic success.
- Before closing the milestone, BA Agent still needs three consecutive lossless self-hosted workflows. The native console operations required for the personal Alpha are now implemented and locally verified.

## M5 — Claude compatibility and migration

Canonical config generation, Claude compatibility report, TheAgency dry-run/import, and runtime-neutral receipt validation.

Status: **complete locally (2026-08-24)**.

- One canonical plugin source renders Codex and Claude instructions, eight roles, 11 Skills, Hooks, and the shared guard without overwriting user-owned content.
- A structured compatibility report explicitly distinguishes Claude configuration support from unsupported live dispatch, realtime events, and interactive approvals.
- Runtime state snapshots default to dry-run and import as canonical, digest-addressed evidence; Artifact and Receipt records are atomic and the authoritative Task/Run state machines are not bypassed.
- Missing digest-addressed evidence is now detected: dry-run reports `would-restore`, apply restores canonical content with a new repair Receipt, and guarded path traversal rejects unsafe parents or content conflicts.
- The end-to-end CLI demonstration produced `configuration_only`, preserved 8 roles and 11 Skills, reported `would-create` before apply, created one Artifact and one Receipt, and verified the permanent receipt chain.

## M6 — Public Beta

Windows x64, Linux x64, macOS x64/arm64 archives; checksums; install/upgrade/rollback; third-party license inventory; reproducible docs; WCAG 2.2 AA and release quality gates. Remote publication remains separately approved.

Status: **local release engineering in progress**. No remote repository or release has been created.

- A manual-only candidate workflow targets Windows x64, Linux x64, macOS Intel, and macOS arm64 and uploads Actions artifacts without publishing a Release.
- Platform packaging emits a portable archive plus mandatory SHA-256 sidecar; install scripts verify the digest before reading package entries and exercise install, upgrade, rollback, uninstall, and explicit runtime-data retention.
- A deterministic generator inventories the four release targets' union of 273 non-workspace Rust packages and 63 embedded-console runtime packages, embeds 202 deduplicated full license/notice documents with package mappings, and fails closed when installed notice text is missing. The sole upstream npm omission has a provenance-locked override; release-owner legal review of the expressions remains required.
- The latest notice-complete Windows candidate contains exactly eight expected files and passes install, upgrade, rollback, uninstall, database retention, checksum-mismatch/traversal/reparse-point rejection, startup/API/memory budgets, production-console Web Vitals, forced-color/200% text regression, and the console WCAG audit. After moving runtime handshake/recovery off the listening critical path, three cold starts measured 624.765 ms, 126.408 ms and 134.076 ms. All four hosted-runner jobs, cross-platform manual assistive-tech confirmation, and public-release authorization remain open.
