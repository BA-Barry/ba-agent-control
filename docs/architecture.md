# Architecture

BA Agent is a local control plane with explicit boundaries between durable domain state, runtime execution, Git isolation, the HTTP/MCP surfaces, and the embedded console.

```text
CLI / embedded console / MCP clients
                 │
      localhost HTTP + SSE / stdio
                 │
        ba-server + ba-mcp
                 │
      ba-scheduler ─ ba-runtime
          │              │
       ba-git        Codex adapters
          │
       ba-store ───── ba-core
          │
    global SQLite + repository .ba-agent/
```

## Workspace boundaries

- `ba-core`: domain identifiers, entities, state machines, canonical JSON, policy types, and shared errors.
- `ba-store`: SQLite migrations, transactional repositories, event ordering, backups, integrity checks, retention, and receipt-chain persistence.
- `ba-git`: repository discovery, worktree lifecycle, path-scope validation, local branch/commit creation, and protected-operation classification.
- `ba-runtime`: runtime capability model and Codex App Server / `codex exec --json` adapters.
- `ba-scheduler`: dependency-aware dispatch, concurrency and write-scope conflict control, recovery, approval waits, and gate orchestration.
- `ba-server`: localhost REST `/api/v1`, SSE, session security, static console embedding, and application state.
- `ba-mcp`: structured MCP tools that call application services rather than writing SQLite directly.
- `ba-cli`: the `ba-agent-control` binary and stable command hierarchy.

## Persistence boundaries

The global database is stored under the operating system application-data directory. It uses WAL mode, foreign keys, a transaction per migration, a pre-migration backup, and startup integrity checking. Operational events are append-only with per-run monotonically increasing sequence numbers.

Repository-owned artifacts live under `.ba-agent/` and may be version controlled:

```text
.ba-agent/
  project.toml
  workflows/
  plans/
  knowledge/
  receipts/
```

Detailed logs, worktrees, lock files, session tokens, and transient adapter state never enter the project repository.

Project onboarding is a shared scheduler service used by CLI, REST, MCP, and the console. Import mode accepts only the exact root of an existing Git repository and preserves an existing project configuration. Create mode accepts only a missing or empty directory, initializes a local `main` repository, and creates the repository-owned directories. Duplicate canonical roots, nested repository paths, and non-empty create targets are rejected. Rollback removes only artifacts created by the failed create operation; onboarding never creates a remote or performs a push.

Workflow definitions are version-controlled TOML in `.ba-agent/workflows/`. Six built-in definitions are embedded in the binary; a valid project file with the same identifier is an explicit override. Instantiation resolves each narrow role to a durable AgentProfile, then inserts the Workstream and self-contained dependency graph in one transaction. `Task.agent_profile_id` is a real foreign key with restricted profile deletion, not an inferred UI label.

## Execution model

A Task is schedulable only when dependencies are satisfied, approvals are clear, and its declared path scope does not conflict with a running writer. All projects share a durable global pool whose default maximum is four. Each project may request a lower cap in `.ba-agent/project.toml`; the effective cap is the lesser of the project and global values. An eligible overflow dispatch remains durably queued instead of being rejected. Admission claims run inside an immediate SQLite transaction, so concurrent BA Agent control processes cannot overclaim a slot. The queue follows dependency satisfaction and user-controlled submission order, can be reordered or cancelled, and is drained again after terminal runs, cancellations, dependency completion, or policy changes.

Read tasks may run without a worktree; after every configured gate succeeds they complete with a permanent read-review receipt. Write tasks create their detached worktree only after admission, then finish with a local commit, `ba/<workstream>/<task-id>` branch, and commit receipt. CLI gate execution must match the project's configured program and argument vector exactly, so a same-name command cannot forge a required pass. Remote or destructive operations remain approval-gated.

Codex App Server is the preferred adapter because it exposes threads, tool calls, approvals, and lifecycle events. Capability negotiation verifies the CLI version, `codex login status`, App Server availability, and fallback support. An installed but unauthenticated CLI is reported as unavailable for dispatch with an explicit login action. If the negotiated protocol is unsupported, the adapter visibly degrades to `codex exec --json`; the CLI, HTTP API, and console receive the actual capabilities, never simulated parity.

## Security model

The service binds to loopback only. Every server start creates an in-memory random bearer/session token and a CSRF value. Mutating requests require both; Origin is restricted to the exact server origin. Responses set a restrictive CSP. Filesystem inputs are canonicalized and checked against registered roots before access.

Hooks are defense in depth and cannot replace the Codex sandbox. They reject dangerous Git commands, broad deletion, writes outside a declared scope, direct protected-state edits, and commits that bypass required gates.

## Recovery and evidence

On startup, active runs whose process cannot be reconciled become `orphaned`; resumable adapters may restore them, otherwise the user can retry from the retained task and event stream. A quality Receipt uses canonical JSON, SHA-256, and the previous receipt hash. Logs may expire after the configured retention period; receipt records and their essential evidence remain permanent.

Agent handoffs are Artifact records backed by canonical JSON under `.ba-agent/knowledge/handoffs/`. Creation accepts only source-to-direct-dependent destinations in the same project and validates every linked Artifact, Receipt, run, and repository-relative changed path. On every read, BA Agent checks the file digest, normalized Artifact linkage, and structured metadata before injecting the handoff into a downstream launch or recovery prompt. The digest has a narrower purpose than the permanent Receipt chain: it detects substitution while a repository-owned handoff exists before its next Git commit.
