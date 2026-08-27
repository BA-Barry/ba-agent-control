# Completion audit

This audit maps the approved BA Agent implementation plan to reproducible evidence. It distinguishes implemented behavior from release certification; a feature is not marked complete merely because a supporting component exists.

## Status legend

- **Proven locally** — implemented and exercised by an automated test, local release candidate, or recorded live run.
- **Proven hosted** — exercised successfully on the named clean GitHub-hosted runner and tied to an immutable commit and workflow run.
- **Proven in parts** — an aggregate requirement has strong component evidence but still lacks one or more certification checks named in that requirement.
- **Pending external** — requires a clean hosted platform, remote repository, legal owner, or explicit publication authorization.
- **Blocked externally** — the implementation is ready, but the authenticated Codex account cannot currently complete the run.

## Milestone evidence

| Milestone | Status | Reproducible evidence | Remaining gate |
| --- | --- | --- | --- |
| M1 Foundation | Proven locally and hosted | Embedded single binary, SQLite migrations/integrity, plugin bundle, and the green Windows/Ubuntu/macOS, console and dependency-audit jobs in [CI run 32866913236](https://github.com/BA-Lau/ba-agent-control/actions/runs/32866913236) | None |
| M2 Core vertical slice | Proven locally | Transactional state tests, temporary Git/worktree tests, App Server fixture/live contract, gates, local commit, receipts, restart recovery | None for local Alpha |
| M3 Complete Codex control | Proven locally | Live single-turn and four-parallel-Agent demonstrations, fifth-run capacity rejection, approvals, messages, Hooks, recovery, exec fallback, credential-safe GitHub adapter | None for local Alpha |
| M4 Personal Alpha console | Proven locally and hosted | 40 functional/accessibility and two isolated performance scenarios cover seven routes, desktop/Pixel 7, Chinese/English, Field Book/Chart Room/Signal Room interfaces, light/dark, real milestone/task mutations, truthful runtime authentication/degradation, keyboard operation, loading, empty, disconnected, extreme text, forced colors, 200% text and Web Vitals; three consecutive real self-hosted workflows survived intervening restarts with 84 ordered events, 18 passed gates and three chained receipts | Manual assistive-technology certification and the known Signal Room forced-colors contrast finding remain Public Beta gates |
| M5 Claude compatibility and migration | Proven locally | Canonical generator, checksum-managed merges, compatibility report, TheAgency dry-run/apply import, runtime-state evidence repair and hash-chain tests | Claude live dispatch is intentionally outside Alpha |
| M6 Public Beta | Four-platform hosted candidate proven; final human gates pending | [Release-candidate run 33011040260](https://github.com/BA-Lau/ba-agent-control/actions/runs/33011040260) at commit `308eb7cd2967cbdc21344c3be995471cc3e84685` passed dependency audits and native Windows x64, Linux x64, macOS Intel and macOS ARM build/test/performance/package/security/lifecycle jobs; downloaded archives were independently checksum- and entry-verified | Signal Room forced-colors contrast, manual assistive-tech checks, release-owner legal review and explicit publication approval |

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
| Light/dark/system appearance and three interface themes | Proven locally | Independent persisted appearance/interface-theme settings; route-wide axe coverage across Field Book, Chart Room and Signal Room; system behavior unit/manual coverage |
| Keyboard, focus and screen-reader semantics | Proven locally for automated rules | Skip-link test and zero axe violations in 168 route/interface-theme/appearance/language/device scans plus desktop/mobile forced-color checks; forced-color sidebar foreground/background regression assertion |
| Responsive and extreme content | Proven locally | 1440×900 and Pixel 7 document-overflow checks, long CJK/English/emoji/path fixture, and 200% text regression |
| Loading, empty, error/fallback and disconnected states | Proven locally | Slow/empty mocked API, explicit demo fallback, disconnected EventSource tests |
| WCAG 2.2 AA Public Beta certification | Proven in parts | Route-wide axe and the existing Field Book forced-color/200% regression pass; an exploratory combined forced-colors/200% run found insufficient active-navigation contrast in Signal Room on desktop, and native Windows high-contrast, browser zoom and manual screen-reader checks remain release gates |

### Release, licensing and performance

| Requirement | Status | Evidence |
| --- | --- | --- |
| Deterministic third-party inventory and full notices | Proven locally and hosted | 273 Rust and 39 runtime frontend packages; 196 deduplicated documents; generation diff gate passed on all four hosted package jobs |
| Windows archive and lifecycle | Proven hosted | Eight-entry archive SHA-256 `16b98c3111f6ad3d6e04d7e9c7aadd4b98e7eaa2b16cdc55322ec188f29e2751`; hosted install, upgrade, rollback, uninstall and retained SQLite data passed |
| Malicious package rejection | Proven hosted | Windows checksum/traversal/reparse-point and Unix checksum/traversal/symlink tests passed natively |
| Linux/macOS archives and lifecycle | Proven hosted | Linux x64, macOS Intel and macOS ARM native package, checksum, install, upgrade, rollback, uninstall, data-retention and malicious-package tests passed in run 33011040260 |
| Core startup/API/memory budgets | Proven hosted | Cold start was 11.245–259.462 ms, API p95 0.460–1.903 ms and idle memory 6.89–12.73 MiB across all four clean hosted targets |
| UI LCP/INP/CLS budgets | Proven hosted | Across all four hosted targets, desktop LCP 204–512 ms / INP 24–96 ms / CLS 0.02253 and mobile LCP 156–536 ms / INP 24–56 ms / CLS 0–0.03777; all budgets passed |
| No P0/P1 defects | Proven for core automated gates; release audit pending | CI and the complete hosted candidate matrix are green with zero failed required automated gates; the known Signal Room forced-colors contrast finding, manual assistive-tech checks and legal sign-off remain before Public Beta |

## Alpha acceptance scenario

The complete non-model acceptance scenario is proven locally by `alpha_acceptance_survives_four_way_dispatch_restart_and_retention`. In one continuous test it registers three real temporary Git repositories, models a completed dependency, admits four different agent profiles, rejects a fifth run at the concurrency limit, isolates three write tasks in distinct worktrees, pauses and resumes a protected push approval, passes configured gates, creates three `ba/<workstream>/<task-id>` branches and receipts, reopens SQLite, resumes a persisted reader thread after orphan recovery, then applies a future 90-day runtime-data cutoff and verifies that all three receipt chains survive another reopen.

This deterministic test proves orchestration and persistence without consuming model quota. It is now supplemented by three consecutive real-model self-hosted successes, each with a completed Task, succeeded Run, six configured gate passes, permanent receipt, and post-restart verification.

## Remaining blockers and next actions

1. Fix the Signal Room active-navigation contrast under Windows forced colors plus 200% text, then cover all three interface themes with that combined regression.
2. Complete a manual screen-reader pass plus native Windows High Contrast and actual browser 200% zoom checks.
3. Have the release owner review the generated third-party inventory and full notice bundle.
4. Only after those gates pass, request separate approval for public repository visibility, a version tag, GitHub Release and public archives.

A private verification remote, authenticated push, CI and hosted candidate workflow were explicitly authorized and completed. Public visibility, tags, GitHub Releases, public archives and deployment remain unauthorized.
