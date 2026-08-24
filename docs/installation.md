# Install, upgrade, rollback, and uninstall

Release candidates are portable archives containing one BA Agent binary, `LICENSE`, `NOTICE`, this guide, and the generated third-party license inventory. Verify the adjacent `.sha256` file before installing. The scripts never delete BA Agent's operating-system data directory.

## Windows x64

```powershell
$expected = (Get-Content .\ba-agent-control-v0.1.0-alpha.1-windows-x64.zip.sha256).Split(' ')[0]
$actual = (Get-FileHash -Algorithm SHA256 .\ba-agent-control-v0.1.0-alpha.1-windows-x64.zip).Hash.ToLowerInvariant()
if ($actual -ne $expected) { throw "checksum mismatch" }

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
expected=$(awk '{print $1}' ba-agent-control-v0.1.0-alpha.1-linux-x64.tar.gz.sha256)
actual=$(sha256sum ba-agent-control-v0.1.0-alpha.1-linux-x64.tar.gz | awk '{print $1}')
[ "$actual" = "$expected" ] || { echo "checksum mismatch" >&2; exit 1; }

mkdir ba-agent-package
tar -C ba-agent-package -xzf ba-agent-control-v0.1.0-alpha.1-linux-x64.tar.gz
sh ba-agent-package/install.sh install ba-agent-control-v0.1.0-alpha.1-linux-x64.tar.gz "$HOME/.local/bin"
"$HOME/.local/bin/ba-agent-control" doctor
sh ba-agent-package/install.sh rollback "$HOME/.local/bin"
sh ba-agent-package/install.sh uninstall "$HOME/.local/bin"
```

On macOS, use `shasum -a 256` when `sha256sum` is unavailable. Replace the archive label with `macos-x64` or `macos-arm64` as appropriate.

## Data retention

The default database, logs, and worktrees stay under the platform application-data directory reported by `ba-agent-control --json doctor`. Upgrade, rollback, and uninstall only manipulate the installed binary and its one previous-version sibling. To remove runtime data, first inspect the exact `dataDirectory` returned by `doctor`, stop BA Agent, back up permanent receipts, and remove that explicit directory manually; no bundled script performs this destructive step.
