---
name: ba-plan
description: Plan BA Agent work when a defined change needs dependency-aware tasks, declared write scopes, role assignments, gates, and completion evidence.
---

# Plan executable tasks

Break the approved definition and design into independently verifiable tasks. For each task record its prerequisites, recommended narrow role, read/write intent, declared paths, acceptance checks, and quality gates.

Parallelize only tasks with independent writes. Order overlapping write scopes serially. Keep push, merge, deploy, deletion, privilege elevation, migration risk, and new external permissions as explicit approvals.

Return the dependency graph and critical path. Do not queue or dispatch tasks.
