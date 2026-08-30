# BA Agent

BA Agent (`ba-agent-control`) is a local-first control plane for coordinating auditable AI-assisted development across multiple Git projects. It reuses your authenticated Codex CLI, isolates write tasks in worktrees, pauses risky operations for approval, and stores permanent hash-chained quality receipts.

> Status: the personal Alpha feature set, three-workflow self-hosting acceptance, hosted CI, private four-platform release-candidate certification, release-owner technical license sign-off, and manual accessibility acceptance are complete. No tag, GitHub Release, or public archive has been published.

## What works in this Alpha

- Import an existing local Git repository or safely create a new local repository, then organize workstreams, milestones, and dependent tasks.
- Dispatch Codex-backed runs with capability detection and structured event capture.
- Share a durable global run pool across projects (default four), apply project-specific caps, and automatically start queued work in dependency/submission order while serializing overlapping writers.
- Isolate writes in Git worktrees and protect path scopes.
- Run configurable quality gates, create local commits, and preserve receipts.
- Control runs, approvals, gates, and receipts through the loopback HTTP/SSE API and MCP tools.
- Probe GitHub credentials safely and inspect repository metadata without enabling remote writes.
- Generate Codex and Claude-compatible project configuration without overwriting user-owned content.
- Import and safely restore runtime state snapshots as immutable, receipt-backed evidence without bypassing BA Agent state machines.
- Operate all seven bilingual console surfaces with real local API/SSE data, system/light/dark appearance, three materially different interface themes, keyboard navigation, project/task creation, role-aware dispatch, configured gate execution, run control, approvals, and receipt verification.

The console follows the approved comp-first “测量员外业簿” direction and is embedded into the Rust binary. When the local service is unavailable it enters an explicitly labeled demonstration mode instead of presenting sample data as live state.

## Quick start

Prerequisites: Rust stable, Node.js 24, pnpm 11.19.0, Git, and Codex CLI 0.130.0+ already authenticated.

```powershell
pnpm install --frozen-lockfile
pnpm build
cargo run -p ba-agent-control -- doctor
cargo run -p ba-agent-control -- adapter profiles
cargo run -p ba-agent-control -- ui
```

The UI server binds to `127.0.0.1` only and prints a one-time session URL. Runtime data is stored in the operating system's application-data directory; repository-owned configuration is under `.ba-agent/`.
Running `ui` again while the default local service is alive safely reuses and reopens that authenticated service instead of starting a duplicate process.
`doctor --json` reports `controlService.status` as `running`, `stopped`, or `stale`. An invalid one-time launch link shows a local recovery page with the exact command to reopen the console; it never guesses or persists the rejected credential.

On the **项目 / Projects** page, choose **导入现有仓库** to register an existing Git root without replacing its `.ba-agent/project.toml`, or **创建本地项目** to initialize a missing or empty directory on `main` and create the `.ba-agent/` structure. Neither path creates a GitHub repository, remote, push, tag, or release. The same operations are available as `project import` and `project create`.

Queue and concurrency controls are available in the console under **工作流与任务** and **设置与适配器**, or from the CLI:

```powershell
ba-agent-control dispatch queue
ba-agent-control dispatch reorder <DISPATCH_ID> <DISPATCH_ID>
ba-agent-control dispatch cancel <DISPATCH_ID>
ba-agent-control dispatch global 4
ba-agent-control dispatch project-limit <PROJECT_ID> 2
```

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

See [中文使用说明](docs/usage.zh-CN.md), [Architecture](docs/architecture.md), [Product backlog](docs/product-backlog.md), [Completion audit](docs/completion-audit.md), [Runtime compatibility](docs/runtime-compatibility.md), [Self-hosting evidence](docs/self-hosting.md), [Install and rollback](docs/installation.md), [Release candidates](docs/release.md), [GitHub adapter](docs/github-adapter.md), [Performance evidence](docs/performance.md), [Contributing](CONTRIBUTING.md), and [Security](SECURITY.md).

## License

Apache-2.0. See [NOTICE](NOTICE) for clean-room attribution.
