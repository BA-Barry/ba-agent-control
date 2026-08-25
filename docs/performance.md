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

The local Windows x64 GNU release build from this earlier checkpoint produced one `ba-agent-control.exe` file. Running `--json doctor` from that artifact returned `ready`, Codex CLI 0.130.0, and database integrity `ok`. PE import inspection found only Windows system and UCRT DLLs; no project-specific runtime DLL was required beside the executable. Current artifact identity is recorded in the final section below.

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

## 2026-08-24 — Current M5/self-hosting rebuild

The release artifact was rebuilt after runtime-state evidence restoration and the first self-hosting hardening pass. It is a single 11.70 MiB executable (12,270,080 bytes); `doctor` returned `ready`, database integrity `ok`, and Codex CLI 0.130.0.

| Budget | Measurement | Result |
| --- | ---: | --- |
| Core cold start to listening address `< 1 s` | Three fresh-data launches: 567.658 ms, 522.592 ms, and 507.773 ms | Pass; clean-machine first-launch risk remains open |
| Common local API p95 `< 100 ms` | Three 100-request samples: p95 5.064 ms, 4.177 ms, and 4.102 ms | Pass |
| Core idle memory `< 100 MB` | 13.16–13.33 MiB working set | Pass |

An earlier first launch during the same hardening pass took 1,339.546 ms immediately after relinking, so the first-ever startup risk is not closed even though the final artifact's three measured launches passed. The final artifact's first sample also contained one 107.168 ms API maximum while its p95 remained 5.064 ms; the following samples had maxima of 7.050 ms and 13.067 ms. Public Beta must repeat this measurement on clean Windows, Ubuntu, and macOS hosts and investigate both first-ever startup and isolated API outliers.

## 2026-08-25 — Personal Alpha console and notice-complete candidate

The MSVC release artifact was rebuilt with the complete embedded console, native project/task and retry operations, worktree API, and generated third-party notice bundle. The executable is 15,666,688 bytes with SHA-256 `22bff31ed2e4c9df140a3ee9f4fc99083219ab34c748851b74f4537175bf1035`.

| Budget | Measurement | Result |
| --- | ---: | --- |
| Core cold start to listening address `< 1 s` | Three fresh-data launches after artifact scan: 726.015 ms, 646.169 ms, 713.402 ms | Pass, with first-launch risk below |
| Common local API p95 `< 100 ms` | Three 100-request samples: p95 4.857 ms, 4.894 ms, 5.108 ms | Pass |
| Core idle memory `< 100 MB` | 14.17–14.32 MiB working set | Pass |

The immediate first launch after relinking took 1,418.130 ms and one request in that first sample reached 125.424 ms; the same binary then passed all budgets in three independent fresh-data samples. This reinforces the unresolved clean-machine first-launch scan/cache risk rather than weakening the stated budget.

The local Windows release candidate contained eight expected files, including the 617,635-byte deduplicated notice text bundle. Its 6,250,133-byte archive completed install, upgrade, rollback, uninstall, and database-retention verification.

### Embedded console Web Vitals

Playwright measured the production Vite build after the full seven-route accessibility pass. The test installs buffered Performance Observers before navigation, waits for the first LCP candidate before interaction, opens the Radix display menu, changes language, and asserts the approved budgets on desktop Chrome and Pixel 7 emulation.

| Device profile | LCP `< 1.5 s` | INP `< 200 ms` | CLS `< 0.1` | Result |
| --- | ---: | ---: | ---: | --- |
| Desktop Chrome, 1440 × 900 | 452–616 ms | 72–96 ms | 0–0.0226 | Pass |
| Pixel 7 emulation | 392–504 ms | 56–80 ms | 0 | Pass |

These are repeatable local production-build measurements, not a substitute for clean-machine measurements on every release target.

### Security-hardened Windows candidate

The final local rebuild in this pass embeds the production-tested console, including real milestone/dependency operations and forced-color hardening, plus the mandatory archive-checksum installation policy. The binary is 15,843,328 bytes with SHA-256 `5ffde5c010e34fd3ab0ac5f7e5cd60420b8b8d0dfdec33df4505e81501958d53`. Its eight-entry archive is 6,306,305 bytes with SHA-256 `8199ce10bfb8a7ff0a2ca97ca476f8ecece1529ae5fa10142bbb324b17c0cd78`.

Runtime capability handshake and orphan recovery now begin immediately after the loopback listener and durable store are ready instead of blocking the listening-address handoff. Database migration/integrity, built-in role persistence, control-descriptor ownership and loopback binding remain on the synchronous safety path.

| Budget | Three fresh-data measurements | Result |
| --- | ---: | --- |
| Core cold start `< 1 s` | 624.765 ms; 126.408 ms; 134.076 ms | Pass, including first sample |
| Common local API p95 `< 100 ms` | 4.645 ms; 4.528 ms; 4.510 ms | Pass |
| Core idle memory `< 100 MB` | 14.52–14.66 MiB | Pass |

One request in the first API sample reached 108.818 ms while p95 remained 4.645 ms; subsequent maxima were 35.046 ms and 31.946 ms. Clean-host certification must still retain first-sample results and investigate recurring outliers.
