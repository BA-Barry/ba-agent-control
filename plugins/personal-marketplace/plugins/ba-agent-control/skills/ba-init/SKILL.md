---
name: ba-init
description: Initialize BA Agent metadata in an existing Git repository when a developer wants to register or prepare that project for controlled local runs.
---

# Initialize a project

Inspect the repository before changing it. Confirm it is a Git worktree, identify its root and default branch, and report any existing `.ba-agent/` content.

Create only project-owned configuration requested by the user. Keep runtime logs, worktrees, locks, tokens, and databases out of the repository. Preserve existing files and surface a conflict instead of replacing them.

Return the registered root, created files, detected adapters, and any health warning. Do not dispatch a run during initialization unless the user separately asks.
