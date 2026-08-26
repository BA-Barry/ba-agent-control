# BA Agent

BA Agent (`ba-agent-control`) is a local-first control plane for coordinating auditable AI-assisted development across multiple Git projects. It reuses your authenticated Codex CLI, isolates write tasks in worktrees, pauses risky operations for approval, and stores permanent hash-chained quality receipts.

> Status: the personal Alpha feature set, three-workflow self-hosting acceptance, hosted CI, and the private four-platform release-candidate certification are complete. Manual accessibility and release-owner legal review remain before Public Beta; no tag, GitHub Release, or public archive has been published.

## What works in this Alpha

- Register local projects and organize workstreams, milestones, and dependent tasks.
- Dispatch Codex-backed runs with capability detection and structured event capture.
- Isolate writes in Git worktrees and protect path scopes.
- Run configurable quality gates, create local commits, and preserve receipts.
- Control runs, approvals, gates, and receipts through the loopback HTTP/SSE API and MCP tools.
- Probe GitHub credentials safely and inspect repository metadata without enabling remote writes.
- Generate Codex and Claude-compatible project configuration without overwriting user-owned content.
- Import and safely restore runtime state snapshots as immutable, receipt-backed evidence without bypassing BA Agent state machines.
- Operate all seven bilingual console surfaces with real local API/SSE data, responsive themes, keyboard navigation, project/task creation, run control, approvals, and receipt verification.

The console follows the approved comp-first “测量员外业簿” direction and is embedded into the Rust binary. When the local service is unavailable it enters an explicitly labeled demonstration mode instead of presenting sample data as live state.

## Quick start

Prerequisites: Rust stable, Node.js 24, pnpm 11.19.0, Git, and Codex CLI 0.130.0+ already authenticated.

```powershell
pnpm install --frozen-lockfile
pnpm build
cargo run -p ba-agent-control -- doctor
cargo run -p ba-agent-control -- ui
```

The UI server binds to `127.0.0.1` only and prints a one-time session URL. Runtime data is stored in the operating system's application-data directory; repository-owned configuration is under `.ba-agent/`.

## 中文简介

BA Agent 是面向高级个人开发者的本地研发控制面。它复用已登录的 Codex CLI，把写任务隔离在 Git worktree 中，对 push、merge、deploy、删除和提权操作保留人工审批，并用可校验的哈希链永久保存质量收据。默认界面为中文，完整支持英文，默认不启用遥测、云同步或模型 API 直连。

## Development

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
pnpm validate
pnpm test:e2e
python plugins/personal-marketplace/plugins/ba-agent-control/scripts/validate_bundle.py
python plugins/personal-marketplace/plugins/ba-agent-control/scripts/test_hook_guard.py
```

See [Architecture](docs/architecture.md), [Completion audit](docs/completion-audit.md), [Runtime compatibility](docs/runtime-compatibility.md), [Self-hosting evidence](docs/self-hosting.md), [Install and rollback](docs/installation.md), [Release candidates](docs/release.md), [GitHub adapter](docs/github-adapter.md), [Performance evidence](docs/performance.md), [Contributing](CONTRIBUTING.md), and [Security](SECURITY.md).

## License

Apache-2.0. See [NOTICE](NOTICE) for clean-room attribution.
