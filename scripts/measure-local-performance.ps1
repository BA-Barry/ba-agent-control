param(
    [string]$Executable = "target/release/ba-agent-control.exe",
    [int]$Samples = 100
)

$ErrorActionPreference = "Stop"
$resolvedExecutable = (Resolve-Path -LiteralPath $Executable).Path
$benchmarkRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("ba-agent-bench-" + [guid]::NewGuid().ToString("N"))
$dataRoot = Join-Path $benchmarkRoot "data"
$stdoutPath = Join-Path $benchmarkRoot "stdout.log"
$stderrPath = Join-Path $benchmarkRoot "stderr.log"
New-Item -ItemType Directory -Path $dataRoot | Out-Null

$startupWatch = [System.Diagnostics.Stopwatch]::StartNew()
$process = Start-Process `
    -FilePath $resolvedExecutable `
    -ArgumentList @("--data-dir", $dataRoot, "ui", "--no-open") `
    -RedirectStandardOutput $stdoutPath `
    -RedirectStandardError $stderrPath `
    -PassThru `
    -WindowStyle Hidden

try {
    $port = $null
    for ($attempt = 0; $attempt -lt 200; $attempt++) {
        if (Test-Path -LiteralPath $stdoutPath) {
            $match = Get-Content -LiteralPath $stdoutPath -ErrorAction SilentlyContinue |
                Select-String -Pattern "BA Agent: http://127\.0\.0\.1:(\d+)/" |
                Select-Object -First 1
            if ($null -ne $match) {
                $port = [int]$match.Matches[0].Groups[1].Value
                break
            }
        }
        Start-Sleep -Milliseconds 10
    }
    $startupWatch.Stop()
    if ($null -eq $port) {
        throw "server did not report a listening address"
    }

    $durations = [System.Collections.Generic.List[double]]::new()
    for ($index = 0; $index -lt $Samples; $index++) {
        $requestWatch = [System.Diagnostics.Stopwatch]::StartNew()
        $null = Invoke-WebRequest -UseBasicParsing -Uri "http://127.0.0.1:$port/api/v1/health"
        $requestWatch.Stop()
        $durations.Add($requestWatch.Elapsed.TotalMilliseconds)
    }
    $ordered = $durations | Sort-Object
    $p50Index = [math]::Max(0, [math]::Ceiling($Samples * 0.50) - 1)
    $p95Index = [math]::Max(0, [math]::Ceiling($Samples * 0.95) - 1)
    $process.Refresh()

    [pscustomobject]@{
        coldStartMs  = [math]::Round($startupWatch.Elapsed.TotalMilliseconds, 3)
        apiSamples   = $Samples
        apiP50Ms     = [math]::Round($ordered[$p50Index], 3)
        apiP95Ms     = [math]::Round($ordered[$p95Index], 3)
        apiMaxMs     = [math]::Round($ordered[$Samples - 1], 3)
        workingSetMiB = [math]::Round($process.WorkingSet64 / 1MB, 2)
    } | ConvertTo-Json -Compress
}
finally {
    $process.Refresh()
    if (-not $process.HasExited) {
        Stop-Process -Id $process.Id
    }
}
