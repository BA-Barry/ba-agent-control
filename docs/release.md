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

- Run the four-platform candidate workflow on an approved remote repository and retain logs and artifacts.
- Complete the release-owner legal review of every third-party license expression. Full installed license and notice texts are already generated, deduplicated by SHA-256, mapped back to packages, and bundled; the one npm package that omits its declared MIT text has a provenance-locked upstream override.
- Repeat performance, install, upgrade, rollback, uninstall, and data-retention tests on clean hosts.
- Obtain explicit user approval before creating the remote repository or publishing tags, releases, or archives.
