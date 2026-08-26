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

[Run 32866937107](https://github.com/BA-Lau/ba-agent-control/actions/runs/32866937107) completed successfully for commit `542abe5241c4d35418700b3c5272b93aef685f7a`. Each platform passed 24 functional/accessibility browser scenarios, two isolated Web Vitals scenarios, strict Clippy, all-feature workspace tests, release packaging, performance budgets, malicious-package rejection, install, upgrade, rollback, uninstall and SQLite data retention.

| Target | Inner release archive SHA-256 | Core cold start | API p95 | Idle memory |
| --- | --- | ---: | ---: | ---: |
| `windows-x64` | `6c7e6b1632259d3fecf93de7d8c04db2d7012ce1170da9034ed45453237fdb51` | 161.310 ms | 1.965 ms | 12.65 MiB |
| `linux-x64` | `2ad62efa177f4bf627d9f78f15b4ef47bc70218ffdd38f1767b88ae3953f9172` | 81.904 ms | 0.410 ms | 12.16 MiB |
| `macos-x64` | `c81d07b88837d779a7153fefab53a74ba0b0408e388fb163f782db033086fa74` | 106.743 ms | 1.395 ms | 6.91 MiB |
| `macos-arm64` | `3b1516ee40a0ac0cd2b82467b5ce1e7d4f4a2317b1d038b29da9493877b9ec02` | 76.620 ms | 0.501 ms | 9.44 MiB |

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

## Public Beta blockers

- Complete the release-owner legal review of every third-party license expression. Full installed license and notice texts are already generated, deduplicated by SHA-256, mapped back to packages, and bundled; the one npm package that omits its declared MIT text has a provenance-locked upstream override.
- Complete manual screen-reader, native Windows High Contrast and actual browser 200% zoom checks.
- Obtain explicit user approval before changing the private repository to public or publishing tags, GitHub Releases or public archives.
