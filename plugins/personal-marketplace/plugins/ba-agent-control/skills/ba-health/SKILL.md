---
name: ba-health
description: Diagnose BA Agent health when a project, database, runtime adapter, worktree, run, receipt chain, or local service appears unavailable or degraded.
---

# Diagnose health

Inspect read-only health endpoints and local diagnostics first. Check repository registration, database integrity, migrations, adapter version and negotiated capabilities, active process reconciliation, worktree state, receipt-chain verification, and disk or path errors.

Distinguish healthy, degraded, and blocked conditions. Preserve state while diagnosing; do not delete worktrees, rewrite receipts, migrate data, or reinstall tools without explicit authorization.

Return the likely cause, supporting evidence, safe remediation order, and actions that need approval.
