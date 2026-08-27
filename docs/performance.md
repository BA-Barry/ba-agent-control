# Performance evidence

This document preserves both local development measurements and the final private hosted release-candidate certification. The hosted section supersedes earlier open clean-host risks while retaining the earlier measurements as an audit trail.

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

The final local rebuild in this pass embeds the production-tested console, including real milestone/dependency operations, forced-color hardening, live runtime authentication/capability reporting, configured-gate anti-spoofing at both execution and finalization, and quality-gated read-review receipts, plus the mandatory archive-checksum installation policy. The binary is 15,876,608 bytes with SHA-256 `2aefcdbfe4132c6454aa7bb757e56461069495db4ac0438c1a7d2ae333447088`. Its eight-entry archive is 6,319,279 bytes with SHA-256 `57ab9439d74cda312fbbefd68ab28d32da8d296b0c11de3a18cd3c1c3bd2f914`.

Runtime capability handshake and orphan recovery now begin immediately after the loopback listener and durable store are ready instead of blocking the listening-address handoff. Database migration/integrity, built-in role persistence, control-descriptor ownership and loopback binding remain on the synchronous safety path.

| Budget | Three fresh-data measurements | Result |
| --- | ---: | --- |
| Core cold start `< 1 s` | 827.557 ms; 142.794 ms; 183.179 ms | Pass, including first sample |
| Common local API p95 `< 100 ms` | 6.441 ms; 6.336 ms; 6.555 ms | Pass |
| Core idle memory `< 100 MB` | 14.53–14.57 MiB | Pass |

One request in the first API sample reached 132.677 ms while p95 remained 6.441 ms; subsequent maxima were 29.300 ms and 7.358 ms. Clean-host certification must still retain first-sample results and investigate recurring outliers.

## 2026-08-25 — Native WSL2 Linux x64 candidate

The repository was compiled and tested inside the installed Ubuntu/WSL2 user space with Rust 1.98.0. Strict Clippy passed and 91 platform-applicable non-model tests passed; the two live Codex tests remained explicitly ignored by the default suite and were separately exercised successfully on Windows. The release binary is 15,738,224 bytes with SHA-256 `8bf850e09d7bcde9cf67aa99c496f0fd6fe4695ed579f74de9beb8939200bbd6`. Its tar archive contains eight payload files, is 6,476,421 bytes, and has SHA-256 `087e48ac7d8f7cf187d638d692adaa3765d4b2cdfafe8095c76e08674e8c746c`.

The Unix installer rejected checksum mismatch, traversal entries and a symlink install directory, then passed install, upgrade, rollback, uninstall and SQLite data-retention checks. `scripts/measure-local-performance.sh` provides the same fresh-data measurement contract for Linux and macOS as the PowerShell benchmark provides on Windows.

| Budget | Three fresh-data measurements | Result |
| --- | ---: | --- |
| Core cold start `< 1 s` | 21.282 ms; 31.506 ms; 31.713 ms | Pass |
| Common local API p95 `< 100 ms` | 2.203 ms; 2.165 ms; 1.992 ms | Pass |
| Core idle memory `< 100 MB` | 12.57–12.88 MiB | Pass |

At this checkpoint the mounted Windows workspace and warm WSL kernel meant the result was not yet a clean hosted Ubuntu candidate. A native Codex CLI 0.149.1 was installed and its App Server capability was detected, while `codex login status` confirmed that the WSL-specific session was not authenticated. The hardened runtime correctly reported `authenticated=false`, `available=false`, and `doctor=attention` with an explicit `codex login` action; the already authenticated Windows Codex CLI remained `doctor=ready`. The hosted section below closes Ubuntu release certification; a real authenticated WSL Codex turn still requires logging in within that separate WSL environment.

## 2026-08-25 UTC / 2026-08-26 CST — Four-platform hosted candidate

[Release-candidate run 32866937107](https://github.com/BA-Lau/ba-agent-control/actions/runs/32866937107) certified commit `542abe5241c4d35418700b3c5272b93aef685f7a` on clean GitHub-hosted Windows x64, Ubuntu x64, macOS Intel and macOS ARM runners. The RustSec and production npm audits passed before the matrix started. Every target then passed 24 functional/accessibility browser scenarios, two isolated Web Vitals scenarios, strict Clippy, all-feature workspace tests, native release build, performance budgets, packaging, malicious-package rejection and the complete install lifecycle with retained SQLite data.

### Core budgets

| Target | Cold start `< 1 s` | API p95 `< 100 ms` | Idle memory `< 100 MiB` | Result |
| --- | ---: | ---: | ---: | --- |
| Windows x64 | 161.310 ms | 1.965 ms | 12.65 MiB | Pass |
| Linux x64 | 81.904 ms | 0.410 ms | 12.16 MiB | Pass |
| macOS Intel | 106.743 ms | 1.395 ms | 6.91 MiB | Pass |
| macOS ARM | 76.620 ms | 0.501 ms | 9.44 MiB | Pass |

The Windows sample retained an isolated 356.629 ms API maximum in the evidence; the stated budget is p95, which was 1.965 ms. No sample or outlier was discarded.

### Embedded console Web Vitals

| Hosted target | Desktop LCP / INP / CLS | Mobile LCP / INP / CLS | Result |
| --- | ---: | ---: | --- |
| Windows x64 | 464 ms / 32 ms / 0.02253 | 408 ms / 24 ms / 0 | Pass |
| Linux x64 | 296 ms / 24 ms / 0.02253 | 276 ms / 24 ms / 0 | Pass |
| macOS Intel | 364 ms / 144 ms / 0.02253 | 244 ms / 120 ms / 0 | Pass |
| macOS ARM | 152 ms / 40 ms / 0.02253 | 160 ms / 24 ms / 0 | Pass |

All observations remained below LCP 1.5 seconds, INP 200 ms and CLS 0.1. Functional/accessibility and performance scenarios were intentionally executed in separate Playwright invocations, with one performance worker, so runner contention could not mask a product regression or create a false budget failure.

## 2026-08-26 — Interface-theme hardening

The console retained the approved Field Book as its unchanged default and introduced two structural alternatives: Chart Room reorganizes the shell as a horizontal drafting table, while Signal Room separates navigation and live surfaces into a modular operations desk. Forty functional/accessibility scenarios cover all three themes across light/dark appearance, both languages, seven routes and desktop/Pixel 7 viewports, including 168 route scans with zero axe violations plus explicit forced-color sidebar checks. The isolated production confirmation measured desktop LCP/INP/CLS at 340 ms / 24 ms / 0.02253 and Pixel 7 emulation at 256 ms / 16 ms / 0; both remained within budget.

## 2026-08-27 CST — Forced-color navigation correction

The Signal Room active-navigation contrast defect under forced colors plus 200% text was reproduced as a theme-specific selector overriding the system highlight background. The forced-color state now controls the active link, label, icon, count and border together. The non-performance suite contains 44 functional/accessibility scenarios, including six combined checks across Field Book, Chart Room and Signal Room on desktop and Pixel 7. Every product assertion passed locally; one parallel full-suite request hit the Windows host's transient `ERR_NO_BUFFER_SPACE` before navigation and its isolated single-worker retry passed. The two production performance scenarios then passed independently at desktop 476 ms LCP / 24 ms INP / 0 CLS and Pixel 7 404 ms LCP / 16 ms INP / 0 CLS.

### Downloaded artifact verification

| Target | Archive bytes | Inner archive SHA-256 | Files |
| --- | ---: | --- | ---: |
| Windows x64 | 6,214,773 | `6c7e6b1632259d3fecf93de7d8c04db2d7012ce1170da9034ed45453237fdb51` | 8 |
| Linux x64 | 6,323,251 | `2ad62efa177f4bf627d9f78f15b4ef47bc70218ffdd38f1767b88ae3953f9172` | 8 |
| macOS Intel | 5,818,116 | `c81d07b88837d779a7153fefab53a74ba0b0408e388fb163f782db033086fa74` | 8 |
| macOS ARM | 5,489,324 | `3b1516ee40a0ac0cd2b82467b5ce1e7d4f4a2317b1d038b29da9493877b9ec02` | 8 |

After downloading the four workflow artifacts, the adjacent checksum files were parsed and every inner archive was rehashed locally. All four matched. ZIP/tar entry inspection found exactly the expected binary, installer, installation guide, README, BA Agent license and notice, third-party inventory and deduplicated notice bundle on every platform.

## 2026-08-26 UTC / 2026-08-27 CST — Refreshed interface-theme candidate

[Release-candidate run 33011040260](https://github.com/BA-Lau/ba-agent-control/actions/runs/33011040260) certified commit `308eb7cd2967cbdc21344c3be995471cc3e84685` after the interface-theme hardening. RustSec and the production npm audit passed before all four native jobs completed 40 functional/accessibility scenarios, two isolated Web Vitals scenarios, strict Clippy, all-feature workspace tests, release builds, performance budgets, packaging, malicious-package rejection and install/upgrade/rollback/uninstall with retained SQLite data.

| Target | Cold start `< 1 s` | API p95 `< 100 ms` | Idle memory `< 100 MiB` | Archive bytes | Inner archive SHA-256 |
| --- | ---: | ---: | ---: | ---: | --- |
| Windows x64 | 259.462 ms | 1.903 ms | 12.73 MiB | 6,217,836 | `16b98c3111f6ad3d6e04d7e9c7aadd4b98e7eaa2b16cdc55322ec188f29e2751` |
| Linux x64 | 11.245 ms | 0.460 ms | 11.96 MiB | 6,327,135 | `b9be5312f84540d205482c0152cf26b178d1216f888253fab8da1173aa890b20` |
| macOS Intel | 69.746 ms | 0.863 ms | 6.89 MiB | 5,821,905 | `e616b860fde7ba8c421f7d6bc2b8ab1a87c65a717ec081970bbee853bf539f01` |
| macOS ARM | 119.783 ms | 0.498 ms | 9.39 MiB | 5,493,253 | `3f4c6d12186c8651af3df961c189e47b8a3327204a3becad07ebebe455194063` |

Hosted Web Vitals remained within budget on every target: desktop LCP 204–512 ms, INP 24–96 ms and CLS 0.02253; Pixel 7 LCP 156–536 ms, INP 24–56 ms and CLS 0–0.03777. The Windows API sample retained a 362.910 ms maximum while p95 remained 1.903 ms; no outlier was discarded.

The four private artifacts were downloaded after the run. Every adjacent checksum matched a fresh local SHA-256 calculation, and each ZIP/tar archive contained exactly the expected eight payload files. This certifies the immutable commit above, not this later evidence-only documentation commit and not any future publication tag.

## 2026-08-27 CST — Accessibility-hardened four-platform candidate

[Release-candidate run 33043702277](https://github.com/BA-Lau/ba-agent-control/actions/runs/33043702277) certified commit `309b719d33205518ed29bb1f83894b210eb5bbfb` with 44 functional/accessibility scenarios and two isolated Web Vitals scenarios on every native target. The expanded suite includes Field Book, Chart Room and Signal Room under forced colors plus 200% text on desktop and Pixel 7. RustSec, production npm audit, Clippy, all-feature workspace tests, packaging, malicious-package rejection, installation lifecycle and retained runtime data all passed.

| Target | Cold start `< 1 s` | API p95 `< 100 ms` | Idle memory `< 100 MiB` | Archive bytes | Inner archive SHA-256 |
| --- | ---: | ---: | ---: | ---: | --- |
| Windows x64 | 146.856 ms | 2.017 ms | 12.56 MiB | 6,217,823 | `5212299662fdff092f14ec61ecce87bc135333cb4b8fc27b46db4689b1ff96bd` |
| Linux x64 | 11.215 ms | 2.292 ms | 12.04 MiB | 6,326,932 | `deecb4b3c5b7f34cecd4ffb3ff52370a70960e81613a25ceede8e930ff5927ab` |
| macOS Intel | 135.519 ms | 1.073 ms | 6.93 MiB | 5,822,232 | `9d722f8d7579832b1c49048124898ccd63fca7004beb867fe69d3b7492e70c64` |
| macOS ARM | 53.264 ms | 0.645 ms | 9.33 MiB | 5,493,395 | `4d60da77f7e50f1588f0102d072c8c690bffff621f23cb39e3cacb9cb049adf0` |

Hosted Web Vitals remained within budget: desktop LCP 188–496 ms, INP 32–120 ms and CLS 0.02253; Pixel 7 LCP 144–448 ms, INP 24–80 ms and CLS 0. The downloaded sidecar digests matched independent local SHA-256 calculations and every archive contained exactly eight non-directory payload entries.
