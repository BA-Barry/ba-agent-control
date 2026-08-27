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

[Run 33011040260](https://github.com/BA-Lau/ba-agent-control/actions/runs/33011040260) completed successfully for commit `308eb7cd2967cbdc21344c3be995471cc3e84685`. Each platform passed 40 functional/accessibility browser scenarios, two isolated Web Vitals scenarios, strict Clippy, all-feature workspace tests, release packaging, performance budgets, malicious-package rejection, install, upgrade, rollback, uninstall and SQLite data retention.

| Target | Inner release archive SHA-256 | Core cold start | API p95 | Idle memory |
| --- | --- | ---: | ---: | ---: |
| `windows-x64` | `16b98c3111f6ad3d6e04d7e9c7aadd4b98e7eaa2b16cdc55322ec188f29e2751` | 259.462 ms | 1.903 ms | 12.73 MiB |
| `linux-x64` | `b9be5312f84540d205482c0152cf26b178d1216f888253fab8da1173aa890b20` | 11.245 ms | 0.460 ms | 11.96 MiB |
| `macos-x64` | `e616b860fde7ba8c421f7d6bc2b8ab1a87c65a717ec081970bbee853bf539f01` | 69.746 ms | 0.863 ms | 6.89 MiB |
| `macos-arm64` | `3f4c6d12186c8651af3df961c189e47b8a3327204a3becad07ebebe455194063` | 119.783 ms | 0.498 ms | 9.39 MiB |

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

## Public Beta blockers

- Resolve the Signal Room active-navigation contrast finding exposed by the expanded Windows forced-colors plus 200% text check, then run that combined regression across all three interface themes.
- Complete manual screen-reader, native Windows High Contrast and actual browser 200% zoom checks.
- Obtain explicit user approval before changing the private repository to public or publishing tags, GitHub Releases or public archives.
