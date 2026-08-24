# BA Agent

BA Agent (`ba-agent-control`) is a local-first control plane for coordinating auditable AI-assisted development across multiple Git projects. It reuses your authenticated Codex CLI, isolates write tasks in worktrees, pauses risky operations for approval, and stores permanent hash-chained quality receipts.

> Status: foundation, core vertical slice, complete Codex control, and configuration compatibility are complete locally. The seven-surface console remains at its comp-first visual gate. This repository is intentionally local-only; no remote repository or release has been created.

## What works in this Alpha

- Register local projects and organize workstreams, milestones, and dependent tasks.
- Dispatch Codex-backed runs with capability detection and structured event capture.
- Isolate writes in Git worktrees and protect path scopes.
- Run configurable quality gates, create local commits, and preserve receipts.
- Control runs, approvals, gates, and receipts through the loopback HTTP/SSE API and MCP tools.
- Probe GitHub credentials safely and inspect repository metadata without enabling remote writes.
- Generate Codex and Claude-compatible project configuration without overwriting user-owned content.
- Import and safely restore runtime state snapshots as immutable, receipt-backed evidence without bypassing BA Agent state machines.

The seven-surface bilingual console is not implemented yet. Its visual direction is fixed to “测量员外业簿”, but implementation remains behind the approved comp-first gate: three comps must be generated and one selected first. The current embedded console is only an unstyled bootstrap notice.

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
```

See [Architecture](docs/architecture.md), [Runtime compatibility](docs/runtime-compatibility.md), [Self-hosting evidence](docs/self-hosting.md), [Install and rollback](docs/installation.md), [Release candidates](docs/release.md), [GitHub adapter](docs/github-adapter.md), [Performance evidence](docs/performance.md), [Contributing](CONTRIBUTING.md), and [Security](SECURITY.md).

## License

Apache-2.0. See [NOTICE](NOTICE) for clean-room attribution.
