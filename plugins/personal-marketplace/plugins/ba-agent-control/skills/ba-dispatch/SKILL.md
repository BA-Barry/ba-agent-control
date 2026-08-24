---
name: ba-dispatch
description: Dispatch ready BA Agent tasks when prerequisites, approvals, runtime capabilities, concurrency, and declared write scopes must be checked before execution.
---

# Dispatch ready tasks

Read current task, run, approval, adapter, and worktree state through BA Agent tools. Dispatch only `ready` tasks whose dependencies are complete and whose path scope does not conflict with an active writer.

Use the requested role when it is compatible; otherwise explain the safer role. Write tasks require an isolated worktree. Never simulate unsupported adapter capabilities or bypass a pending approval.

Return dispatch identifiers, actual adapter capabilities, isolation details, and immediate blockers. Stop after the requested dispatch set is accepted or rejected.
