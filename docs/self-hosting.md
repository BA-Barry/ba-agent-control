# Self-hosting evidence

BA Agent registered its own repository as project `01M0SJAAPZMHYQNC6GN3HY4Q1H` and created the `personal-alpha` workstream on 2026-08-24. On 2026-08-25 the user resolved the visual gate in favor of the comp-first “测量员外业簿” direction, and the seven-surface console plus live local API/SSE integration passed local verification. Existing durable Task records must still be reconciled through explicit lifecycle transitions; this document does not retroactively rewrite their state.

## Reviewer run 01M0SK3R8GRGFNSSB1P0A28X99

The first real self-hosted task asked the built-in read-only Reviewer to inspect the M5 compatibility and runtime-state evidence import. It ran through Codex App Server without a worktree and recorded 470 ordered events from dispatch through terminal failure.

- Fourteen repository-read commands were approved one at a time.
- One command attempted an isolated destructive reproduction under the system temporary directory. It was rejected because the command did not resolve and assert that its deletion target remained within the newly created temporary root.
- The rejection protected both repository and external files, but the adapter treated one declined tool call as a terminal run failure instead of allowing the Reviewer to continue with a non-executed finding.
- The run exposed a real M5 defect before failure: an existing Artifact caused reimport to return `unchanged` even if the corresponding evidence file was missing.

The defect was reproduced safely in a Rust `tempfile` test and fixed. Missing evidence now reports `would-restore` in dry-run, restores only with `--apply`, appends a repair Receipt, and revalidates the hash chain. The approval ergonomics and rejected-command recovery behavior is tracked as task `01M0SKWJKQHM8MVWC7H3PAP1YZ`.

This run is intentionally retained as failed evidence. It demonstrates that self-hosting captures approval decisions and failure paths rather than rewriting them into a synthetic success.

## Reviewer run 01M0VPSZM9FQR2HPHNV30V0GS4

On 2026-08-25 a second read-only Reviewer run inspected the native project/task creation, retry, and worktree-display changes. It launched through Codex App Server, persisted its desktop thread identity, crossed two explicit read-only command approvals, and recorded 165 ordered events without state loss.

The run terminated with the runtime's structured `usageLimitExceeded` error after the account reported 99% of its weekly limit. BA Agent atomically marked the Run and Task failed and left no pending approval. It is not counted toward the three consecutive successful self-hosting workflows; retry remains available when the external usage window resets.
