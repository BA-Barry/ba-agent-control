# Windows 本地数据迁移

这份手册用于把已经运行过的 BA Agent 全局数据、worktree 和开发工具缓存从
C 盘迁移到另一个本地磁盘（例如 `G:`）。迁移只改变本机路径，不创建远程仓库、
不上传数据，也不需要 GitHub Actions。命令默认只复制和验证；删除旧副本必须由
用户在完成验收后单独执行。

## 迁移前检查

先在 BA Agent 所在的终端读取当前数据目录。`doctor` 产生的是机器可读 JSON，
不要手工猜测旧目录：

```powershell
$cli = (Get-Command ba-agent-control).Source
$doctor = & $cli --json doctor | ConvertFrom-Json
$source = [System.IO.Path]::GetFullPath([string]$doctor.dataDirectory)
$doctor.controlService | ConvertTo-Json -Depth 5
Test-Path -LiteralPath (Join-Path $source 'ba-agent.db')
```

若 `controlService.status` 为 `running`，记下 `processId`，先确认它确实属于
BA Agent，再停止该进程。数据库正在使用时不要复制 `ba-agent.db`、`-wal` 或
`-shm` 文件：

```powershell
$servicePid = [int]$doctor.controlService.processId
Get-CimInstance Win32_Process -Filter "ProcessId = $servicePid" |
  Select-Object ProcessId,Name,ExecutablePath,CommandLine
Stop-Process -Id $servicePid -Force
```

重新执行 `& $cli --json doctor`，直到状态为 `stopped` 或控制描述符不存在。若
进程信息不是 BA Agent，停止并排查，不要结束该 PID。

目标目录必须是一个新的、明确的本地路径。例如：

```powershell
$target = 'G:\BA Agent Data\ba-agent-control\data'
if ([System.IO.Path]::GetFullPath($source) -eq [System.IO.Path]::GetFullPath($target)) {
  throw 'source and target must be different'
}
New-Item -ItemType Directory -Force -Path $target | Out-Null
```

## 复制并验证

`robocopy` 的 `/XJ` 防止跟随 junction；退出码 0–7 表示复制完成或只有可接受的
差异，8 及以上表示失败。命令不会删除源目录：

```powershell
robocopy $source $target /E /XJ /COPY:DAT /DCOPY:DAT /R:2 /W:2 /V
if ($LASTEXITCODE -ge 8) { throw "data copy failed: robocopy exit $LASTEXITCODE" }
```

至少核对数据库和目录差异：

```powershell
$sourceDb = Join-Path $source 'ba-agent.db'
$targetDb = Join-Path $target 'ba-agent.db'
if (!(Test-Path -LiteralPath $targetDb)) { throw 'target database is missing' }
$sourceHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $sourceDb).Hash
$targetHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $targetDb).Hash
if ($sourceHash -ne $targetHash) { throw 'database hash mismatch' }
robocopy $source $target /E /XJ /L /BYTES /FP /NS /NC /NDL /NJH /NJS
```

这里的哈希只用于发现“复制过程被打断但文件大小仍相同”的数据库替换错误；
它不会成为 BA Agent 的新数据契约，也不会替代启动时的 SQLite 完整性检查。

已有 worktree 的记录包含绝对路径。复制全局目录不会自动改写这些记录。启动新
目录前，逐个项目检查控制台中的 worktree 路径，并在每个项目根目录执行：

```powershell
git worktree list --porcelain
```

所有仍在使用的 worktree 都必须存在于目标磁盘；发现旧盘路径时保留旧副本，先
用 Git 修复该 worktree 并重新运行一次受控任务，再继续迁移。不要直接编辑 SQLite
中的 JSON 路径。

## 切换到目标目录

验证数据库、worktree 和永久收据后，设置当前终端和未来用户会话的变量：

```powershell
$env:BA_AGENT_DATA_DIR = $target
[Environment]::SetEnvironmentVariable('BA_AGENT_DATA_DIR', $target, 'User')
```

启动并验证：

```powershell
& $cli --data-dir $target --json doctor | ConvertFrom-Json | ConvertTo-Json -Depth 8
& $cli --data-dir $target receipt verify <PROJECT_ID>
& $cli --data-dir $target ui --no-open
```

重启服务后再次运行 `doctor --json`，确认 `dataDirectory` 为目标路径、数据库
完整性为 `ok`，并检查项目、任务、运行事件、审批和收据仍可读取。至少完成两次
服务重启和一次只读收据验证，再考虑清理旧目录。

## 将 Rust、Cargo 和 pnpm 缓存放到目标盘

工具链与 BA Agent 数据是两个独立迁移。先停止正在运行的 Cargo/Rustup 进程，
使用同样的 `robocopy /E /XJ` 复制到新目录，再设置变量并在新终端验证：

```powershell
$env:CARGO_HOME = 'G:\DevTools\cargo'
$env:RUSTUP_HOME = 'G:\DevTools\rustup'
[Environment]::SetEnvironmentVariable('CARGO_HOME', $env:CARGO_HOME, 'User')
[Environment]::SetEnvironmentVariable('RUSTUP_HOME', $env:RUSTUP_HOME, 'User')
$env:Path = "G:\DevTools\cargo\bin;" + $env:Path
Get-Command cargo
cargo --version
rustup show active-toolchain
```

pnpm 的 store 可以独立迁移：

```powershell
pnpm config set store-dir 'G:\.pnpm-store' --location=global
pnpm store path
```

只有这些命令都指向目标盘、项目构建和测试成功后，才可将旧缓存目录移入一个
可恢复的备份目录。不要在验证前删除源数据或依赖兼容性 junction 掩盖复制错误。

## 回滚与清理

迁移失败时停止新服务，临时用旧目录运行 `--data-dir $source`，确认旧数据库和
收据仍完整，再修复目标目录。保留一份带时间戳的源目录备份；升级、回滚或卸载
BA Agent 二进制不会删除数据库、日志或永久收据。

清理前应同时满足：目标目录连续两次启动完整性为 `ok`、所有项目 worktree 路径
已确认、`receipt verify` 成功、最近一次任务恢复成功。删除动作只允许针对已核对
的旧目录，例如：

```powershell
$oldBackup = 'G:\BA Agent Data\migration-backup\ba-agent-data-from-C'
if (!(Test-Path -LiteralPath $oldBackup)) { throw 'refusing to remove unknown backup path' }
# 先人工核对路径，再对这个明确的备份目录执行 Remove-Item。
```

本手册不会自动删除任何源目录；迁移证据和回滚副本应按项目保留策略保存。
