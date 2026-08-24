# Milestones

## M1 — Foundation

Single-binary server, migrations, empty console, plugin skeleton, validation, and three-platform CI.

Status: **complete locally (2026-08-24)**.

- Release Windows x64 binary: 8.79 MB, embedded hashed console assets, `doctor=ready`.
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

Status: **waiting at the comp-first decision gate**. No designed console surface has been implemented.

- Self-hosting has begun with the repository registered in BA Agent and a `personal-alpha` workstream containing the visual gate, console, release, and hardening tasks.
- The first real Reviewer run used Codex App Server and produced 470 structured events. Fourteen read-only Windows commands were approved individually; rejecting an unsafe destructive reproduction command correctly preserved the repository but currently failed the whole run. The approval-noise/recovery issue is tracked in the self-hosted backlog.

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
- Platform packaging emits a portable archive plus SHA-256; install scripts exercise install, upgrade, rollback, uninstall, and explicit runtime-data retention.
- A deterministic generator inventories the four release targets' union of 273 non-workspace Rust packages and 63 embedded-console runtime packages from locked dependency metadata. License-expression and notice-text review remains required before Public Beta.
- The Windows lifecycle test passes locally. All four hosted-runner jobs, clean-machine startup, console WCAG 2.2 AA, and public-release authorization remain open.
