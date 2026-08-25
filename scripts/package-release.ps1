param(
    [Parameter(Mandatory = $true)]
    [string]$Target,
    [Parameter(Mandatory = $true)]
    [string]$Binary,
    [string]$OutputDirectory = "dist/release",
    [string]$Version
)

$ErrorActionPreference = "Stop"
$repositoryRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot "..")).Path
if ([string]::IsNullOrWhiteSpace($Version)) {
    $Version = (Get-Content -LiteralPath (Join-Path $repositoryRoot "package.json") -Raw | ConvertFrom-Json).version
}
if ($Target -notmatch '^[a-z0-9]+(?:-[a-z0-9]+)*$') { throw "invalid release target: $Target" }
if ($Version -notmatch '^[0-9A-Za-z]+(?:[0-9A-Za-z.-]*[0-9A-Za-z])?$') { throw "invalid release version: $Version" }

$resolvedBinary = (Resolve-Path -LiteralPath $Binary).Path
$outputRoot = if ([System.IO.Path]::IsPathRooted($OutputDirectory)) {
    [System.IO.Path]::GetFullPath($OutputDirectory)
} else {
    [System.IO.Path]::GetFullPath((Join-Path $repositoryRoot $OutputDirectory))
}
New-Item -ItemType Directory -Path $outputRoot -Force | Out-Null
$archiveName = "ba-agent-control-v$Version-$Target"
$archivePath = Join-Path $outputRoot "$archiveName.zip"
$checksumPath = "$archivePath.sha256"
if ((Test-Path -LiteralPath $archivePath) -or (Test-Path -LiteralPath $checksumPath)) {
    throw "release output already exists; refusing to overwrite: $archivePath"
}

$staging = Join-Path $outputRoot (".staging-" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Path $staging | Out-Null
try {
    Copy-Item -LiteralPath $resolvedBinary -Destination (Join-Path $staging "ba-agent-control.exe")
    foreach ($file in @("README.md", "LICENSE", "NOTICE", "THIRD_PARTY_LICENSES.md", "THIRD_PARTY_NOTICES.txt")) {
        Copy-Item -LiteralPath (Join-Path $repositoryRoot $file) -Destination $staging
    }
    Copy-Item -LiteralPath (Join-Path $repositoryRoot "docs/installation.md") -Destination $staging
    Copy-Item -LiteralPath (Join-Path $repositoryRoot "scripts/install.ps1") -Destination $staging
    Compress-Archive -Path (Join-Path $staging "*") -DestinationPath $archivePath
    $hash = (Get-FileHash -Algorithm SHA256 -LiteralPath $archivePath).Hash.ToLowerInvariant()
    "$hash  $([System.IO.Path]::GetFileName($archivePath))`n" |
        Set-Content -LiteralPath $checksumPath -NoNewline -Encoding utf8
    [pscustomobject]@{
        archive = $archivePath
        checksum = $checksumPath
        sha256 = $hash
    } | ConvertTo-Json -Compress
}
finally {
    $resolvedStaging = [System.IO.Path]::GetFullPath($staging)
    $expectedPrefix = $outputRoot.TrimEnd('\') + '\.staging-'
    if ($resolvedStaging.StartsWith($expectedPrefix, [System.StringComparison]::OrdinalIgnoreCase)) {
        Remove-Item -LiteralPath $resolvedStaging -Recurse -Force -ErrorAction SilentlyContinue
    } else {
        throw "refusing to remove staging path outside the release output: $resolvedStaging"
    }
}
