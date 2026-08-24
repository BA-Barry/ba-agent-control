# Performance evidence

Measurements are local development evidence, not release-candidate certification. Release builds and all target platforms must be measured again before Public Beta.

## 2026-08-24 — Windows x64 development build

Environment: Windows x64, Codex CLI 0.130.0, SQLite database created in a fresh ignored directory under `target/`, loopback HTTP only, no projects or active runs.

| Budget | Measurement | Result |
| --- | ---: | --- |
| Core cold start to listening address `< 1 s` | 102 ms | Pass |
| Common local API p95 `< 100 ms` | `/api/v1/health`, 100 sequential requests: p50 4.081 ms, p95 5.505 ms, max 92.172 ms | Pass |
| Core idle memory `< 100 MB` | 17.0 MB working set after the request sample | Pass |

The startup benchmark includes opening/migrating a fresh SQLite database, probing Codex capabilities, binding a random loopback port, and generating the session credentials. Tokens were redacted from benchmark output.

### Release artifact

The local Windows x64 GNU release build produced one `ba-agent-control.exe` file of 8.79 MB. Running `--json doctor` from that artifact returned `ready`, Codex CLI 0.130.0, and database integrity `ok`. PE import inspection found only Windows system and UCRT DLLs; no project-specific runtime DLL is required beside the executable.

## Live runtime contract

The ignored `ba-runtime` live integration test completed one logged-in Codex App Server turn through `initialize`, `thread/start`, `turn/start`, and `turn/completed` in 69.63 seconds. This is a runtime compatibility check, not an interactive latency budget.

The ignored `ba-scheduler` four-agent integration test completed Coordinator, Planner, Explorer, and Reviewer runs concurrently in 63.01 seconds. All four reached `succeeded` with their tasks in `review`; a fifth task was rejected by the default concurrency limit of four.

## 2026-08-24 — M3 GitHub adapter release rebuild

The reproducible local script `scripts/measure-local-performance.ps1` measured the release binary after adding pure-Rust HTTPS and native credential-store support.

| Budget | Measurement | Result |
| --- | ---: | --- |
| Core cold start to listening address `< 1 s` | Three fresh-data launches after artifact scan: 557.964 ms, 542.087 ms, 616.646 ms | Pass, with first-launch risk below |
| Common local API p95 `< 100 ms` | Latest 100 sequential `/api/v1/health` requests: p50 3.846 ms, p95 5.049 ms, max 15.024 ms | Pass |
| Core idle memory `< 100 MB` | 13.15 MB working set after the latest request sample | Pass |

The rebuilt Windows x64 GNU artifact remains a single executable and is 11.58 MB (12,141,056 bytes). `doctor` returned `ready`, verified Codex 0.130.0 and SQLite integrity, and safely reported the absent optional GitHub credentials without exposing token material.

The very first launch immediately after relinking the PE took 1,388.962 ms, while the next three launches each used a separate fresh data directory and stayed below 617 ms. This is consistent with a one-time artifact scan/cache effect but is not proven; clean-machine release-candidate testing must treat first-ever launch as an open performance risk rather than discarding the outlier.
