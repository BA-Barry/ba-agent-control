# Completion audit

This audit maps the approved BA Agent implementation plan to reproducible evidence. It distinguishes implemented behavior from release certification; a feature is not marked complete merely because a supporting component exists.

## Status legend

- **Proven locally** — implemented and exercised by an automated test, local release candidate, or recorded live run.
- **Proven in parts** — an aggregate requirement has strong component evidence but still lacks one or more certification checks named in that requirement.
- **Pending external** — requires a clean hosted platform, remote repository, legal owner, or explicit publication authorization.
- **Blocked externally** — the implementation is ready, but the authenticated Codex account cannot currently complete the run.

## Milestone evidence

| Milestone | Status | Reproducible evidence | Remaining gate |
| --- | --- | --- | --- |
| M1 Foundation | Proven locally | Embedded single binary, SQLite migrations/integrity, plugin bundle, 3-OS CI definition, 92 Windows / 91 Linux non-model Rust tests and console suites | Hosted CI has not run without a user-approved remote |
| M2 Core vertical slice | Proven locally | Transactional state tests, temporary Git/worktree tests, App Server fixture/live contract, gates, local commit, receipts, restart recovery | None for local Alpha |
| M3 Complete Codex control | Proven locally | Live single-turn and four-parallel-Agent demonstrations, fifth-run capacity rejection, approvals, messages, Hooks, recovery, exec fallback, credential-safe GitHub adapter | None for local Alpha |
| M4 Personal Alpha console | Proven locally | 22 Playwright/axe scenarios cover seven routes, desktop/Pixel 7, Chinese/English, light/dark, real milestone/task mutations, truthful runtime authentication/degradation, keyboard skip, loading, empty, disconnected, extreme text, forced colors, 200% text and Web Vitals; three consecutive real self-hosted workflows survived intervening restarts with 84 ordered events, 18 passed gates and three chained receipts | None for local Alpha |
| M5 Claude compatibility and migration | Proven locally | Canonical generator, checksum-managed merges, compatibility report, TheAgency dry-run/apply import, runtime-state evidence repair and hash-chain tests | Claude live dispatch is intentionally outside Alpha |
| M6 Public Beta | Proven locally on Windows and WSL2 Linux; pending hosted/macOS certification | Notice-complete Windows and native Linux archives, mandatory SHA-256 verification, install/upgrade/rollback/uninstall/data retention, malicious archive/checksum/link tests, performance samples | Four hosted targets, clean-machine startup, legal review, manual assistive-tech pass, and publication approval |

## Requirement coverage

### Architecture, data and security

| Requirement | Status | Evidence |
| --- | --- | --- |
| Rust 2024 workspace and embedded React/Vite console | Proven locally | Workspace build and server embedded-asset test |
| Global SQLite with WAL, foreign keys, migrations, backup and integrity check | Proven locally | Store migration, backup, round-trip and integrity tests |
| Fixed domain entities, ULIDs and UTC timestamps | Proven locally | Domain/store type and persistence tests; active milestone replacement is atomic and leaves one current milestone per project |
| Task and Run state machines | Proven locally | Transactional allowed/denied transition tests |
| Append-only ordered run events | Proven locally | Strict sequence tests; latest event sequence is exposed by the Run API and console |
| Permanent canonical JSON/SHA-256 receipt chain | Proven locally | Tamper, normalization, contiguous sequence and retention tests |
| Loopback-only HTTP, random session, Origin/CSRF/CSP/path checks | Proven locally | Server security-header/session/mutation tests and path safety tests |
| No model credential storage, no telemetry/cloud sync by default | Proven locally by architecture | Runtime and GitHub adapters consume existing CLI/session or process-scoped credentials; no persistence path exists |

### Runtime, scheduling and Git

| Requirement | Status | Evidence |
| --- | --- | --- |
| App Server first, `codex exec --json` fallback, Codex CLI 0.130.0+ handshake | Proven locally | Schema fixture contract tests, fallback tests, live ignored integration evidence |
| Maximum concurrency four | Proven locally | Live four-agent demonstration plus integrated three-repository/four-run acceptance test with fifth-dispatch rejection |
| Detached isolated worktree for writes | Proven locally | Integrated acceptance test creates three distinct worktrees across three repositories; retry tests prove attempt-specific isolation |
| Overlapping write scopes serialize | Proven locally | Scheduler scope-conflict tests |
| Gates before local commit and `ba/<workstream>/<task-id>` branch | Proven locally | Scheduler finalize/gate/receipt integration tests |
| Push, merge, deploy, delete and elevation require approval | Proven locally | Approval state tests and 15 Hook guard fixtures; GitHub adapter has no mutation endpoint |
| Crash/orphan recovery and persisted thread resume | Proven locally | Integrated acceptance test reopens SQLite, records an orphaned run, preserves its thread identity and restores it when live |

### CLI, API, MCP and compatibility

| Requirement | Status | Evidence |
| --- | --- | --- |
| Required CLI command families | Proven locally | Clap command tree and CLI tests; task creation accepts milestones, dependencies, write intent and validated scopes |
| `/api/v1` REST plus SSE, no interactive terminal | Proven locally | Server resource, milestone/task mutation and stable SSE envelope tests; console E2E |
| MCP structured operations without direct SQLite access | Proven locally | MCP tests and shared lifecycle service; milestone and dependency-aware scoped task creation are persisted through domain validation |
| Eight narrow roles and eleven focused Skills | Proven locally | Bundle validator: 8 roles/11 Skills; official quick validator: 11/11 |
| Full lifecycle Hooks | Proven locally | Bundle descriptor validation and 15 fixture tests |
| Managed Codex/Claude configuration without silent overwrite | Proven locally | Merge checksum/conflict tests and dual-runtime generation demonstration |
| Claude Alpha scope is configuration/import only | Proven locally | Explicit capability report; no false live-dispatch endpoint |
| `migrate the-agency` dry-run by default and explicit apply | Proven locally | CLI migration tests and receipt-backed import demonstration |

### Console and accessibility

| Requirement | Status | Evidence |
| --- | --- | --- |
| Seven fixed pages and real local operations | Proven locally | Unit/API tests plus production-browser demonstration, including real active-milestone registration, dependency selection and task association |
| Approved comp-first “测量员外业簿” direction | Proven locally | PRODUCT/DESIGN/surface brief, approved comp inventory, semantic HTML/CSS implementation |
| Chinese default and equivalent English | Proven locally | Route-wide unit and Playwright coverage; root `lang` changes |
| Light/dark/system theme | Proven locally | Light/dark automated axe coverage; system behavior unit/manual coverage |
| Keyboard, focus and screen-reader semantics | Proven locally for automated rules | Skip-link test and zero axe violations in 56 route/theme/language/device scans plus desktop/mobile forced-color scans |
| Responsive and extreme content | Proven locally | 1440×900 and Pixel 7 document-overflow checks, long CJK/English/emoji/path fixture, and 200% text regression |
| Loading, empty, error/fallback and disconnected states | Proven locally | Slow/empty mocked API, explicit demo fallback, disconnected EventSource tests |
| WCAG 2.2 AA Public Beta certification | Proven in parts | Automated axe, forced-color and 200% text passes are complete; native Windows high-contrast, browser zoom and manual screen-reader checks remain release gates |

### Release, licensing and performance

| Requirement | Status | Evidence |
| --- | --- | --- |
| Deterministic third-party inventory and full notices | Proven locally | 273 Rust and 63 runtime npm packages; 202 deduplicated documents; generation diff gate |
| Windows archive and lifecycle | Proven locally | Exact eight-entry archive, checksum, install, upgrade, rollback, uninstall and retained SQLite data |
| Malicious package rejection | Proven locally on Windows | Checksum mismatch, traversal archive and reparse-point installation tests |
| Linux/macOS archives and lifecycle | Proven locally for Linux; pending external for macOS | Native WSL2 Linux build passed strict Clippy, 89 applicable non-model tests, packaging, checksum/traversal/symlink rejection, lifecycle and performance budgets; macOS still requires hosted native runners |
| Core startup/API/memory budgets | Proven locally | Final Windows release samples: startup 827.557/142.794/183.179 ms, API p95 6.441/6.336/6.555 ms, memory 14.53–14.57 MiB; clean-host target certification remains pending |
| UI LCP/INP/CLS budgets | Proven locally | Production-build observations stayed within budget: desktop LCP 452–616 ms / INP 72–96 ms / CLS 0–0.0226; Pixel 7 LCP 392–504 ms / INP 56–80 ms / CLS 0; clean-host target certification remains pending |
| No P0/P1 defects | Pending release audit | Current local automated suites pass, but hosted candidates and manual assistive-tech validation are outstanding |

## Alpha acceptance scenario

The complete non-model acceptance scenario is proven locally by `alpha_acceptance_survives_four_way_dispatch_restart_and_retention`. In one continuous test it registers three real temporary Git repositories, models a completed dependency, admits four different agent profiles, rejects a fifth run at the concurrency limit, isolates three write tasks in distinct worktrees, pauses and resumes a protected push approval, passes configured gates, creates three `ba/<workstream>/<task-id>` branches and receipts, reopens SQLite, resumes a persisted reader thread after orphan recovery, then applies a future 90-day runtime-data cutoff and verifies that all three receipt chains survive another reopen.

This deterministic test proves orchestration and persistence without consuming model quota. It is now supplemented by three consecutive real-model self-hosted successes, each with a completed Task, succeeded Run, six configured gate passes, permanent receipt, and post-restart verification.

## Remaining blockers and next actions

1. With explicit user authorization, create or select a remote repository and execute the four-target release-candidate workflow.
2. On clean candidates, record first-ever startup plus UI LCP, INP and CLS; investigate any budget miss instead of discarding the first sample.
3. Complete manual screen-reader, native Windows high-contrast and browser 200% zoom checks, then perform the release-owner third-party license review.
4. Only after those gates pass, request separate approval for tags, GitHub Release or public archives.

No remote repository, credential registration, tag, release, push, merge or deployment is authorized by this audit.
