# Install, upgrade, rollback, and uninstall

Release candidates are portable archives containing one BA Agent binary, `LICENSE`, `NOTICE`, this guide, the generated third-party license inventory, and deduplicated full third-party license/notice texts. Keep the adjacent `.sha256` file beside the archive: the installer validates it before reading or extracting package entries. The scripts never delete BA Agent's operating-system data directory.

## Windows x64

```powershell
Expand-Archive .\ba-agent-control-v0.1.0-alpha.1-windows-x64.zip -DestinationPath .\ba-agent-package
.\ba-agent-package\install.ps1 -Archive .\ba-agent-control-v0.1.0-alpha.1-windows-x64.zip
& "$env:LOCALAPPDATA\Programs\BA Agent\ba-agent-control.exe" doctor
```

Run the same install command with a newer archive to upgrade. The prior binary becomes `ba-agent-control.previous.exe`:

```powershell
.\ba-agent-package\install.ps1 -Rollback
.\ba-agent-package\install.ps1 -Uninstall
```

## Linux x64 and macOS x64/arm64

```sh
mkdir ba-agent-package
tar -C ba-agent-package -xzf ba-agent-control-v0.1.0-alpha.1-linux-x64.tar.gz
sh ba-agent-package/install.sh install ba-agent-control-v0.1.0-alpha.1-linux-x64.tar.gz "$HOME/.local/bin"
"$HOME/.local/bin/ba-agent-control" doctor
sh ba-agent-package/install.sh rollback "$HOME/.local/bin"
sh ba-agent-package/install.sh uninstall "$HOME/.local/bin"
```

BA Agent reuses the Codex CLI session and never stores model credentials. Install Codex CLI 0.130.0 or newer, run `codex login`, and confirm the same native environment reports an authenticated runtime with `ba-agent-control doctor`. Windows and WSL2 have separate CLI installations and login state.

On macOS, use `shasum -a 256` when `sha256sum` is unavailable. Replace the archive label with `macos-x64` or `macos-arm64` as appropriate.

## Data retention

The default database, logs, and worktrees stay under the platform application-data directory reported by `ba-agent-control --json doctor`. Upgrade, rollback, and uninstall only manipulate the installed binary and its one previous-version sibling. To remove runtime data, first inspect the exact `dataDirectory` returned by `doctor`, stop BA Agent, back up permanent receipts, and remove that explicit directory manually; no bundled script performs this destructive step.

## Local console recovery

Run `ba-agent-control --json doctor` and inspect `controlService.status`:

- `running`: run `ba-agent-control ui` to authenticate and reopen the existing service.
- `stopped`: run the same command to start a new loopback service.
- `stale`: run the same command; BA Agent replaces the stale descriptor only after confirming that it does not belong to an authenticated live BA Agent service.

An expired or incorrect one-time launch URL returns a bilingual recovery page and does not persist or forward the rejected token. If the old port is no longer listening, the browser cannot repair that dead connection itself; rerunning `ba-agent-control ui` starts a new service and opens its new authenticated URL. When an explicitly requested `--port` is occupied, the CLI reports that boundary instead of describing the listener as BA Agent.
