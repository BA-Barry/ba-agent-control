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

## Three consecutive quality-gated workflows

After the usage window recovered on 2026-08-25, a minimal live App Server contract turn and the four-parallel-Agent acceptance test both succeeded. BA Agent then completed three consecutive read-only workflows against its own repository. Each workflow used a real authenticated Codex thread, ran all six configured project gates, transitioned the Task through review to completed, appended a permanent `quality_gated_read_review` receipt, verified the receipt chain, and survived a full BA Agent service restart before the next workflow.

| Workflow | Task | Run / thread | Ordered events | Receipt |
| --- | --- | --- | ---: | --- |
| 1 | `01M0W2TM9MPBZ1Y1JCY3FVB1Y9` | `01M0W2TMBA3PZBC08V1T4VEEFD` / `01a0382d-5926-7ab1-9fb6-eefa0940481a` | 28 | `01M0W34JA1FMPC98CBW100MBE1` / `1aaa1444cae4320d6fe8f1a740370056633ab651dd5b3094441b0539995675f6` |
| 2 | `01M0W361AC64N16SFXPBCE6WX0` | `01M0W361BZGRPHG052HZH8JKS9` / `01a03833-0d9e-7081-bc31-a6f88c845c73` | 28 | `01M0W378NGV5VJVSVDPZPD8067` / `a7375ff3b8bc61091da6fee4aff781a02b57702e0f395be0b26995ea2f1f00ef` |
| 3 | `01M0W38DKFN5TAV6G9KBD5SYFC` | `01M0W38DN0XDV45CB8W86JARB9` / `01a03834-3ed6-7241-9316-e2e2b0eb0913` | 28 | `01M0W3A6D2H00GMQ2RY2013QRZ` / `5b419fa4038b4beb4644eb090604405a52434f93811cab87dd3613004f86f95b` |

The first workflow also retained an initial failed lint result when the new read-review finalizer exceeded the strict Clippy line budget. The implementation was refactored, the same configured lint gate was rerun successfully, and finalization used the latest authentic result. This exercised repair-in-place and confirmed that a failed result is never rewritten or discarded. After the final restart, all three Tasks remained `completed`, all three Runs remained `succeeded`, every Run retained sequence 1–28, all 18 required gate passes remained present, and the three receipt hashes formed one valid forward chain.
