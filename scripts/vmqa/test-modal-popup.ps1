$ErrorActionPreference = 'Stop'
$script = Join-Path $PSScriptRoot 'modal-popup.ps1'
$fixturePath = Join-Path ([IO.Path]::GetTempPath()) ("modal-popup-" + [guid]::NewGuid() + '.json')
try {
    @{
        schemaVersion = 1
        windows = @(
            @{ hwnd = '0x100'; snapshots = @(@{ pid = 42; isWindowVisible = $true }) },
            @{ hwnd = '0x200'; firstSeenMs = 50; departedAtMs = 100; snapshots = @(@{ pid = 42; isWindowVisible = $true; className = '#32770'; title = 'Print' }) }
        )
    } | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $fixturePath -Encoding utf8

    $result = & $script -BorrowedProcessId 42 -PrimaryWindowHwnd '0x100' -FixtureReportPath $fixturePath | ConvertFrom-Json
    if ($result.test -ne 'TW-14' -or $result.verdict -ne 'pass') { throw 'Expected a TW-14 pass result' }
    if (@($result.popupWindows).Count -ne 1 -or $result.popupWindows[0].hwnd -ne '0x200') {
        throw 'Expected the separately-owned popup HWND in the TW-14 result'
    }

    @{ schemaVersion = 1; windows = @(@{ hwnd = '0x100'; snapshots = @(@{ pid = 42; isWindowVisible = $true }) }) } |
        ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $fixturePath -Encoding utf8
    $powerShell = (Get-Process -Id $PID).Path
    & $powerShell -NoProfile -File $script -BorrowedProcessId 42 -PrimaryWindowHwnd '0x100' -FixtureReportPath $fixturePath *> $null
    $missingPopupExitCode = $LASTEXITCODE
    if ($missingPopupExitCode -eq 0) { throw 'TW-14 accepted a report with no popup HWND' }
    Write-Output 'PASS: TW-14 detects separately-owned visible popup HWNDs and rejects their absence'
} finally {
    Remove-Item -LiteralPath $fixturePath -Force -ErrorAction SilentlyContinue
}
