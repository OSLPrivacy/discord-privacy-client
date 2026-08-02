$ErrorActionPreference = 'Stop'
$script = Join-Path $PSScriptRoot 'winstate-transient.ps1'
$fixturePath = Join-Path ([IO.Path]::GetTempPath()) ("winstate-transient-" + [guid]::NewGuid() + '.json')
try {
    @{
        samples = @(
            @{ elapsedMs = 0; windows = @(@{ hwnd = '0x100'; className = 'Main'; pid = 10 }) },
            @{ elapsedMs = 50; windows = @(@{ hwnd = '0x100'; className = 'Main'; pid = 10 }, @{ hwnd = '0x200'; className = 'Chrome_WidgetWin_0'; pid = 10 }) },
            @{ elapsedMs = 100; windows = @(@{ hwnd = '0x100'; className = 'Main'; pid = 10 }) }
        )
    } | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $fixturePath -Encoding utf8
    $report = & $script -FixtureSamplesPath $fixturePath -SampleIntervalMs 50 | ConvertFrom-Json
    $tooltip = @($report.windows | Where-Object hwnd -eq '0x200')
    if ($tooltip.Count -ne 1) { throw "Expected one transient HWND, got $($tooltip.Count)" }
    if ($tooltip[0].firstSeenMs -ne 50 -or $tooltip[0].departedAtMs -ne 100) { throw 'Transient HWND lifecycle was not retained' }
    if (-not $tooltip[0].transient -or $tooltip[0].sampleCount -ne 1) { throw 'Short-lived HWND was not recorded as transient' }
    Write-Output 'PASS: short-lived HWND retained through departure'
} finally {
    Remove-Item -LiteralPath $fixturePath -Force -ErrorAction SilentlyContinue
}
