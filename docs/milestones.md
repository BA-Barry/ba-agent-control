# Milestones

## M1 — Foundation

Single-binary server, migrations, empty console, plugin skeleton, validation, and three-platform CI.

Status: **complete locally (2026-08-24)**.

- Latest release Windows x64 binary: 15,876,608 bytes (15.14 MiB), embedded hashed console assets, authenticated runtime status, quality-gated read receipts, `doctor=ready`.
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

Status: **personal Alpha acceptance proven locally (2026-08-25)**.

- The user approved the comp-first “测量员外业簿” direction and retained it unchanged as the default. All seven console surfaces now also provide structurally distinct Chart Room and Signal Room themes, with Chinese/English, system/light/dark appearance, responsive layouts, keyboard focus, and screen-reader semantics.
- Impeccable critique, audit, harden, and polish passes completed. Forty functional/accessibility Playwright scenarios plus two isolated performance scenarios cover all seven routes across every interface-theme/appearance/language combination on desktop and Pixel 7 with zero axe violations, plus real milestone/task mutations, truthful runtime authentication/degradation, keyboard operation, loading, empty, disconnected, extreme-content, forced-color, 200% text and production Web Vitals checks.
- The embedded console now authenticates through the random local session, reads real `/api/v1` entities and worktree identity, consumes a forward-compatible `run-event` SSE envelope, and performs real project/task creation, approval, cancellation, guidance, retry/resume, and receipt-verification mutations with CSRF protection.
- Project milestones are now first-class REST and MCP resources. The console registers a real active milestone, replaces the previous active milestone atomically, derives progress from milestone-linked tasks, and attaches new tasks to the current milestone instead of displaying the project description as synthetic milestone state.
- Dependency-aware task creation is now consistent across the console, CLI, REST API and MCP: each entry point validates project ownership, optional milestone/workstream alignment, read/write intent and repository-relative write scopes. The console dependency diagram is derived from persisted relationships rather than sample ordering.
- When the local service is unavailable, the console visibly enters demo fallback mode and reports the cause instead of silently presenting sample records as live data.
- Production single-binary browser runs loaded real data on all seven routes, registered the BA Agent repository, created a workstream and scoped write task, completed receipt-chain verification, logged no browser errors, and showed no horizontal overflow at 390 × 844.
- Failed retries preserve their earlier detached worktree and allocate a new attempt-specific worktree. Orphaned runs resume their persisted thread; live runs with a Codex thread use the desktop app's registered `codex://threads/<id>` handoff.
- One continuous non-model Alpha acceptance test now registers three real Git repositories, admits four agent profiles while rejecting a fifth, isolates three writers, resolves an approval, passes gates, commits three workstream branches, reopens SQLite for orphan/thread recovery, prunes future-expired runtime data, and verifies every permanent receipt chain after another reopen.
- Self-hosting remains evidence-preserving: the earlier failed Reviewer run and its approval ergonomics issue remain in the backlog rather than being rewritten into a synthetic success.
- Three consecutive real Codex-backed self-hosted workflows each completed six configured quality gates, produced a permanent receipt, and survived a service restart before the next workflow. The final restart retained all three completed Tasks, succeeded Runs, 84 ordered events, 18 passed gates, and a valid three-receipt forward chain.

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

Status: **private four-platform candidate refreshed (2026-08-27 CST); final human and publication gates remain**. The verification repository is private and no tag, GitHub Release or public archive has been published.

- The manual-only candidate workflow certified Windows x64, Linux x64, macOS Intel and macOS ARM at commit `308eb7cd2967cbdc21344c3be995471cc3e84685` in [run 33011040260](https://github.com/BA-Lau/ba-agent-control/actions/runs/33011040260), uploading private Actions artifacts without publishing a Release.
- The candidate workflow now measures and enforces core startup, local API p95 and idle-memory budgets on every native runner before packaging.
- Platform packaging emits a portable archive plus mandatory SHA-256 sidecar; install scripts verify the digest before reading package entries and exercise install, upgrade, rollback, uninstall, and explicit runtime-data retention.
- A deterministic generator inventories the four release targets' union of 273 non-workspace Rust packages and 39 embedded-console runtime packages, embeds 196 deduplicated full license/notice documents with package mappings, and fails closed when installed notice text is missing. The sole upstream npm omission has a provenance-locked override; the release owner completed the technical license sign-off on 2026-08-27 CST.
- All four hosted candidates contain exactly eight expected files and pass install, upgrade, rollback, uninstall, database retention, native checksum/traversal/link or reparse-point rejection, startup/API/memory budgets, 40 functional/accessibility browser scenarios and two isolated Web Vitals scenarios. Downloaded inner archive hashes and entries were independently reverified after upload.
- Remaining gates are the Signal Room forced-colors active-navigation contrast fix and three-theme regression, a manual screen-reader pass, native Windows High Contrast and actual browser 200% zoom checks, and separate authorization for public visibility, tagging and publication.
