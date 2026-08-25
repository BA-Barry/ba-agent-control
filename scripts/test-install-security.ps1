param(
    [Parameter(Mandatory = $true)]
    [string]$Archive,
    [string]$InstallScript = (Join-Path $PSScriptRoot "install.ps1")
)

$ErrorActionPreference = "Stop"
$resolvedArchive = (Resolve-Path -LiteralPath $Archive).Path
$resolvedChecksum = (Resolve-Path -LiteralPath "$resolvedArchive.sha256").Path
$resolvedInstaller = (Resolve-Path -LiteralPath $InstallScript).Path
$testRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("ba-agent-install-security-" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Path $testRoot | Out-Null
$junction = $null

function Assert-Fails {
    param([scriptblock]$Action, [string]$MessagePattern)
    try {
        & $Action
    }
    catch {
        if ($_.Exception.Message -notmatch $MessagePattern) {
            throw "expected failure matching '$MessagePattern', received: $($_.Exception.Message)"
        }
        return
    }
    throw "expected installer failure matching '$MessagePattern'"
}

try {
    $mismatchArchive = Join-Path $testRoot "checksum-mismatch.zip"
    $mismatchChecksum = "$mismatchArchive.sha256"
    Copy-Item -LiteralPath $resolvedArchive -Destination $mismatchArchive
    ("0" * 64) + "  checksum-mismatch.zip`n" | Set-Content -LiteralPath $mismatchChecksum -NoNewline -Encoding utf8
    Assert-Fails {
        & $resolvedInstaller -Archive $mismatchArchive -InstallDirectory (Join-Path $testRoot "mismatch-install")
    } "checksum mismatch"

    $unsafeArchive = Join-Path $testRoot "unsafe-entry.zip"
    Copy-Item -LiteralPath $resolvedArchive -Destination $unsafeArchive
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $zip = [System.IO.Compression.ZipFile]::Open($unsafeArchive, [System.IO.Compression.ZipArchiveMode]::Update)
    try {
        $entry = $zip.CreateEntry("../escape.txt")
        $writer = [System.IO.StreamWriter]::new($entry.Open())
        try { $writer.Write("must never be extracted") } finally { $writer.Dispose() }
    }
    finally {
        $zip.Dispose()
    }
    $unsafeHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $unsafeArchive).Hash.ToLowerInvariant()
    "$unsafeHash  unsafe-entry.zip`n" | Set-Content -LiteralPath "$unsafeArchive.sha256" -NoNewline -Encoding utf8
    Assert-Fails {
        & $resolvedInstaller -Archive $unsafeArchive -InstallDirectory (Join-Path $testRoot "unsafe-install")
    } "unexpected number|unsafe or unexpected"
    if (Test-Path -LiteralPath (Join-Path $testRoot "escape.txt")) { throw "unsafe archive escaped the staging directory" }

    $realDirectory = Join-Path $testRoot "real-install"
    $junction = Join-Path $testRoot "junction-install"
    New-Item -ItemType Directory -Path $realDirectory | Out-Null
    New-Item -ItemType Junction -Path $junction -Target $realDirectory | Out-Null
    Assert-Fails {
        & $resolvedInstaller -Archive $resolvedArchive -ChecksumFile $resolvedChecksum -InstallDirectory $junction
    } "reparse point"

    Write-Output "Windows installer security tests passed: checksum, archive entries, and reparse points"
}
finally {
    if ($junction -and (Test-Path -LiteralPath $junction)) { Remove-Item -LiteralPath $junction -Force }
    $resolvedTestRoot = [System.IO.Path]::GetFullPath($testRoot)
    $expectedPrefix = [System.IO.Path]::GetFullPath([System.IO.Path]::GetTempPath()).TrimEnd('\') + '\ba-agent-install-security-'
    if ($resolvedTestRoot.StartsWith($expectedPrefix, [System.StringComparison]::OrdinalIgnoreCase)) {
        Remove-Item -LiteralPath $resolvedTestRoot -Recurse -Force -ErrorAction SilentlyContinue
    } else {
        throw "refusing to remove unexpected installer test path: $resolvedTestRoot"
    }
}
