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

[Run 33043702277](https://github.com/BA-Lau/ba-agent-control/actions/runs/33043702277) completed successfully for commit `309b719d33205518ed29bb1f83894b210eb5bbfb`. Each platform passed 44 functional/accessibility browser scenarios, two isolated Web Vitals scenarios, strict Clippy, all-feature workspace tests, release packaging, performance budgets, malicious-package rejection, install, upgrade, rollback, uninstall and SQLite data retention. This candidate includes the three-theme forced-colors plus 200% text regression and the Signal Room active-navigation correction.

| Target | Inner release archive SHA-256 | Core cold start | API p95 | Idle memory |
| --- | --- | ---: | ---: | ---: |
| `windows-x64` | `5212299662fdff092f14ec61ecce87bc135333cb4b8fc27b46db4689b1ff96bd` | 146.856 ms | 2.017 ms | 12.56 MiB |
| `linux-x64` | `deecb4b3c5b7f34cecd4ffb3ff52370a70960e81613a25ceede8e930ff5927ab` | 11.215 ms | 2.292 ms | 12.04 MiB |
| `macos-x64` | `9d722f8d7579832b1c49048124898ccd63fca7004beb867fe69d3b7492e70c64` | 135.519 ms | 1.073 ms | 6.93 MiB |
| `macos-arm64` | `4d60da77f7e50f1588f0102d072c8c690bffff621f23cb39e3cacb9cb049adf0` | 53.264 ms | 0.645 ms | 9.33 MiB |

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

On 2026-08-27 CST, the Signal Room active-navigation contrast defect under Windows forced colors plus 200% text was reproduced and fixed without changing any interface theme. Theme-specific navigation rules had overridden the system `Highlight` background while retaining `HighlightText`; the forced-color selector now controls the link, label, icon, count and complete border as one state. The combined regression now runs across Field Book, Chart Room and Signal Room on desktop and Pixel 7, contributing six checks to 44 functional/accessibility scenarios. All six combined checks and both isolated Web Vitals scenarios pass locally and in the four hosted jobs of run 33043702277.

## Public Beta blockers

- Complete manual screen-reader, native Windows High Contrast and actual browser 200% zoom checks.
- Obtain explicit user approval before changing the private repository to public or publishing tags, GitHub Releases or public archives.

The pinned RustSec action currently emits GitHub's non-blocking Node.js 20 deprecation annotation while GitHub forces it onto Node.js 24. The audit succeeds; a future immutable action update should remove the annotation after its provenance and compatibility are reviewed.
