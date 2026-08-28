# Release candidate process

BA Agent's release-candidate workflow is deliberately separate from publication. `.github/workflows/release-candidate.yml` runs only through `workflow_dispatch`, has read-only repository permissions, and uploads archives as workflow artifacts. It cannot create a GitHub repository, tag, GitHub Release, push, merge, deploy, or register credentials.

## Candidate matrix

| Label | Hosted runner | Rust target | Archive |
| --- | --- | --- | --- |
| `windows-x64` | `windows-latest` | `x86_64-pc-windows-msvc` | `.zip` |
| `linux-x64` | `ubuntu-latest` | `x86_64-unknown-linux-gnu` | `.tar.gz` |
| `macos-x64` | `macos-15-intel` | `x86_64-apple-darwin` | `.tar.gz` |
| `macos-arm64` | `macos-15` | `aarch64-apple-darwin` | `.tar.gz` |

The macOS labels follow GitHub's current hosted-runner reference: `macos-15-intel` is Intel and `macos-15` is arm64. Recheck the [official runner table](https://docs.github.com/en/actions/reference/runners/github-hosted-runners) before a Public Beta candidate because labels and images can change.

Every matrix job installs locked dependencies, validates and embeds the console, regenerates the license inventory, runs Clippy and workspace tests for its native target, builds the release binary, enforces startup/API/memory budgets, packages it, rejects checksum/traversal/symlink-or-reparse test packages, and verifies the complete install lifecycle while retaining a test database.

The workflow also gates the matrix on RustSec and production npm high-severity audits. Third-party actions and the stable Rust toolchain setup are pinned to immutable commit SHAs; dependency installs and Rust builds use locked manifests. Browser functional/accessibility scenarios and performance scenarios run separately so platform contention cannot weaken or distort the Web Vitals gate.

## Certified private candidate

[Run 33161059914](https://github.com/BA-Lau/ba-agent-control/actions/runs/33161059914) completed successfully for commit `74a01754d768bd6789a0315b0ebbaa6b568d18b8`. Each platform passed 44 functional/accessibility browser scenarios, two isolated Web Vitals scenarios, strict Clippy, all-feature workspace tests, release packaging, performance budgets, malicious-package rejection, install, upgrade, rollback, uninstall and SQLite data retention. This candidate includes the three-theme forced-colors plus 200% text regression, the Signal Room active-navigation correction, the user-approved simplified Field Book label, and the non-yanked `chacha20` lockfile refresh. Interface colors and structures are unchanged.

| Target | Inner release archive SHA-256 | Core cold start | API p95 | Idle memory |
| --- | --- | ---: | ---: | ---: |
| `windows-x64` | `463c43503da23a2e23ece89034bd36b4f5e155c3bc34d46a81423d5f7bbbbc1d` | 221.087 ms | 2.078 ms | 12.72 MiB |
| `linux-x64` | `57f31dde94b8352a088de81c26a95615331b8264cc876eb6d3809a8276b359cb` | 12.153 ms | 0.544 ms | 11.96 MiB |
| `macos-x64` | `237243301c27544db25f8bcbd695af557279cd73f80cd5e706c3b820bb406664` | 102.110 ms | 0.921 ms | 6.88 MiB |
| `macos-arm64` | `0c953886ecfda353583f95e9b0a18a99cfc36ba135b7439ce6a841ee1905639c` | 62.783 ms | 1.522 ms | 9.28 MiB |

The four downloaded inner archives and adjacent checksum files were independently rechecked after download. Every digest matched and every archive contained exactly the expected eight payload files. These are private workflow artifacts, not a tag, GitHub Release or public distribution. Because this evidence record was committed after the run, a future publication candidate must rerun the workflow on the exact tag commit; these hashes certify only the immutable commit named above.

## Local Windows candidate

```powershell
pnpm install --frozen-lockfile
pnpm build
pnpm licenses:generate
cargo build --release --locked -p ba-agent-control
./scripts/package-release.ps1 `
  -Target windows-x64 `
  -Binary target/release/ba-agent-control.exe `
  -OutputDirectory dist/release
```

The packager refuses to overwrite an existing archive. Each archive contains the binary, BA Agent license and notice, the third-party inventory, deduplicated full third-party license/notice texts, README, and installation guide. The adjacent `.sha256` file uses the conventional `<digest>  <filename>` format and is mandatory for the bundled installer.

## Release-owner license sign-off

On 2026-08-27 CST, the release owner explicitly accepted the generated third-party license inventory for the private Alpha candidate after reviewing the technical summary: 273 Rust runtime packages, 39 embedded-console runtime packages, no detected GPL/AGPL/LGPL expression, one MPL-2.0 package (`option-ext`), one CDLA-Permissive-2.0 package (`webpki-roots`), and the provenance-locked MIT text override for `react-remove-scroll-bar`. The 196 deduplicated license/notice documents remain bundled and mapped back to packages. This is a project release-owner technical sign-off, not an external legal opinion.

## Accessibility hardening

On 2026-08-27 CST, the Signal Room active-navigation contrast defect under Windows forced colors plus 200% text was reproduced and fixed without changing any interface theme. Theme-specific navigation rules had overridden the system `Highlight` background while retaining `HighlightText`; the forced-color selector now controls the link, label, icon, count and complete border as one state. The combined regression now runs across Field Book, Chart Room and Signal Room on desktop and Pixel 7, contributing six checks to 44 functional/accessibility scenarios. All six combined checks and both isolated Web Vitals scenarios pass locally and in the four hosted jobs of run 33161059914.

## Public Beta blockers

- Complete manual screen-reader, native Windows High Contrast and actual browser 200% zoom checks.
- Obtain explicit user approval before changing the private repository to public or publishing tags, GitHub Releases or public archives.

The pinned RustSec action currently emits GitHub's non-blocking Node.js 20 deprecation annotation while GitHub forces it onto Node.js 24. The audit succeeds; a future immutable action update should remove the annotation after its provenance and compatibility are reviewed.
