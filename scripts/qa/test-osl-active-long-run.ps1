[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '../..')).Path
$scriptPath = Join-Path $repoRoot 'scripts/qa/osl-active-long-run.ps1'
$source = Get-Content -LiteralPath $scriptPath -Raw -Encoding utf8

foreach ($required in @('osl-chats-send', 'osl-chats-read', 'friend', 'file', 'settings', 'scrub', 'PrivateMemorySize64', 'HandleCount', 'Threads.Count', 'completed_action_count')) {
    if (-not $source.Contains($required)) { throw "TASK3676 check: missing $required" }
}
if (-not $source.Contains('for ($sampleIndex = 0; $sampleIndex -le $Minutes; $sampleIndex++)')) {
    throw 'TASK3676 check: sample loop must include zero through Minutes'
}
if (-not $source.Contains('--test-threads=1')) {
    throw 'TASK3676 check: focused cargo commands must force one test thread'
}

$output = Join-Path ([System.IO.Path]::GetTempPath()) ("osl-task-3676-check-" + [guid]::NewGuid().ToString('N'))
try {
    & pwsh -NoProfile -NonInteractive -File $scriptPath -Minutes 1 -SampleIntervalSeconds 1 -ActionTimeoutSeconds 30 -ActionPlanMode Fixture -OutputDirectory $output
    if ($LASTEXITCODE -ne 0) { throw "TASK3676 check: one-minute runner exited $LASTEXITCODE" }
    $samples = @(Get-Content -LiteralPath (Join-Path $output 'samples.jsonl') -Encoding utf8 | ForEach-Object { $_ | ConvertFrom-Json })
    foreach ($copy in @('A', 'B')) {
        $copySamples = @($samples | Where-Object { $_.copy -eq $copy })
        if ($copySamples.Count -ne 2) { throw "TASK3676 check: copy $copy has $($copySamples.Count) samples, expected 2" }
        foreach ($sample in $copySamples) {
            foreach ($metric in @('memory_bytes', 'handle_count', 'thread_count', 'completed_action_count')) {
                if ($null -eq $sample.$metric) { throw "TASK3676 check: copy $copy is missing $metric" }
            }
        }
    }
    Write-Host 'TASK3676 focused-check copies=2 samples_per_copy=2 required_metrics=4'
}
finally {
    if (Test-Path -LiteralPath $output) { Remove-Item -LiteralPath $output -Recurse -Force }
}
