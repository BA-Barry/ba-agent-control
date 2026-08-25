param(
    [string]$Archive,
    [string]$ChecksumFile,
    [string]$InstallDirectory = (Join-Path $env:LOCALAPPDATA "Programs\BA Agent"),
    [switch]$Rollback,
    [switch]$Uninstall
)

$ErrorActionPreference = "Stop"
$installRoot = [System.IO.Path]::GetFullPath($InstallDirectory)
$executable = Join-Path $installRoot "ba-agent-control.exe"
$previous = Join-Path $installRoot "ba-agent-control.previous.exe"
if (Test-Path -LiteralPath $installRoot) {
    $rootItem = Get-Item -LiteralPath $installRoot -Force
    if (($rootItem.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0) {
        throw "install directory must not be a reparse point: $installRoot"
    }
}
foreach ($path in @($executable, $previous)) {
    if (Test-Path -LiteralPath $path) {
        $item = Get-Item -LiteralPath $path -Force
        if (($item.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0) {
            throw "installed binary path must not be a reparse point: $path"
        }
    }
}

if ($Uninstall) {
    foreach ($path in @($executable, $previous)) {
        if (Test-Path -LiteralPath $path) { Remove-Item -LiteralPath $path -Force }
    }
    Write-Output "Removed BA Agent binaries. Runtime data was retained."
    exit 0
}

if ($Rollback) {
    if (!(Test-Path -LiteralPath $previous)) { throw "no previous BA Agent binary is available" }
    $swap = Join-Path $installRoot ("ba-agent-control.swap-" + [guid]::NewGuid().ToString("N") + ".exe")
    if (Test-Path -LiteralPath $executable) { Move-Item -LiteralPath $executable -Destination $swap }
    try {
        Move-Item -LiteralPath $previous -Destination $executable
    }
    catch {
        if ((Test-Path -LiteralPath $swap) -and !(Test-Path -LiteralPath $executable)) {
            Move-Item -LiteralPath $swap -Destination $executable
        }
        throw
    }
    if (Test-Path -LiteralPath $swap) { Move-Item -LiteralPath $swap -Destination $previous }
    Write-Output "Rolled back BA Agent. Runtime data was retained."
    exit 0
}

if ([string]::IsNullOrWhiteSpace($Archive)) { throw "-Archive is required for install or upgrade" }
$resolvedArchive = (Resolve-Path -LiteralPath $Archive).Path
$resolvedChecksum = if ([string]::IsNullOrWhiteSpace($ChecksumFile)) {
    "$resolvedArchive.sha256"
} else {
    (Resolve-Path -LiteralPath $ChecksumFile).Path
}
if (!(Test-Path -LiteralPath $resolvedChecksum -PathType Leaf)) {
    throw "checksum sidecar is required: $resolvedChecksum"
}
$checksumText = (Get-Content -LiteralPath $resolvedChecksum -Raw).Trim()
$expectedHash = ($checksumText -split '\s+')[0].ToLowerInvariant()
if ($expectedHash -notmatch '^[0-9a-f]{64}$') { throw "checksum sidecar does not start with a valid SHA-256 digest" }
$actualHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $resolvedArchive).Hash.ToLowerInvariant()
if ($actualHash -ne $expectedHash) { throw "archive checksum mismatch" }
$temporaryRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("ba-agent-install-" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Path $temporaryRoot | Out-Null
try {
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $allowedEntries = @(
        "ba-agent-control.exe",
        "install.ps1",
        "installation.md",
        "LICENSE",
        "NOTICE",
        "README.md",
        "THIRD_PARTY_LICENSES.md",
        "THIRD_PARTY_NOTICES.txt"
    )
    $zip = [System.IO.Compression.ZipFile]::OpenRead($resolvedArchive)
    try {
        $entryNames = @($zip.Entries | ForEach-Object { $_.FullName.Replace('\', '/') })
        if ($entryNames.Count -ne $allowedEntries.Count) { throw "archive contains an unexpected number of entries" }
        foreach ($entry in $entryNames) {
            if ($entry -notin $allowedEntries) { throw "archive contains an unsafe or unexpected entry: $entry" }
        }
        foreach ($allowed in $allowedEntries) {
            if (@($entryNames | Where-Object { $_ -eq $allowed }).Count -ne 1) {
                throw "archive must contain exactly one entry named $allowed"
            }
        }
    }
    finally {
        $zip.Dispose()
    }
    Expand-Archive -LiteralPath $resolvedArchive -DestinationPath $temporaryRoot
    $candidate = Join-Path $temporaryRoot "ba-agent-control.exe"
    if (!(Test-Path -LiteralPath $candidate -PathType Leaf)) { throw "archive does not contain ba-agent-control.exe" }
    $candidateItem = Get-Item -LiteralPath $candidate -Force
    if (($candidateItem.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0) {
        throw "candidate binary must not be a reparse point"
    }
    & $candidate --help | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "candidate binary failed its --help smoke test" }
    New-Item -ItemType Directory -Path $installRoot -Force | Out-Null
    $incoming = Join-Path $installRoot ("ba-agent-control.incoming-" + [guid]::NewGuid().ToString("N") + ".exe")
    Copy-Item -LiteralPath $candidate -Destination $incoming
    if (Test-Path -LiteralPath $previous) { Remove-Item -LiteralPath $previous -Force }
    if (Test-Path -LiteralPath $executable) { Move-Item -LiteralPath $executable -Destination $previous }
    try {
        Move-Item -LiteralPath $incoming -Destination $executable
    }
    catch {
        if ((Test-Path -LiteralPath $previous) -and !(Test-Path -LiteralPath $executable)) {
            Move-Item -LiteralPath $previous -Destination $executable
        }
        if (Test-Path -LiteralPath $incoming) { Remove-Item -LiteralPath $incoming -Force }
        throw
    }
    Write-Output "Installed BA Agent at $executable. Runtime data was retained."
}
finally {
    $resolvedTemporary = [System.IO.Path]::GetFullPath($temporaryRoot)
    $expectedPrefix = [System.IO.Path]::GetFullPath([System.IO.Path]::GetTempPath()).TrimEnd('\') + '\ba-agent-install-'
    if ($resolvedTemporary.StartsWith($expectedPrefix, [System.StringComparison]::OrdinalIgnoreCase)) {
        Remove-Item -LiteralPath $resolvedTemporary -Recurse -Force -ErrorAction SilentlyContinue
    } else {
        throw "refusing to remove unexpected temporary path: $resolvedTemporary"
    }
}
