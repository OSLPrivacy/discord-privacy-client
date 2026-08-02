<#
.SYNOPSIS
  TW-14 — prove that a borrowed application's separately-owned popup is observable.

.DESCRIPTION
  Opens a print popup in the borrowed window, then delegates HWND collection to T20-B2's
  transient sampler. A popup is only a pass when a visible top-level HWND owned by one of the
  borrowed process IDs is observed in addition to the supplied primary borrowed HWND. The caller
  must include browser child-process IDs where the browser puts dialogs in a utility process.
#>

[CmdletBinding()]
param(
    [Parameter(Mandatory)][uint32[]]$BorrowedProcessId,
    [Parameter(Mandatory)][ValidatePattern('^0x[0-9A-Fa-f]+$')][string]$PrimaryWindowHwnd,
    [ValidateRange(50, 600000)][int]$DurationMs = 3000,
    [ValidateRange(10, 1000)][int]$SampleIntervalMs = 50,
    [string]$FixtureReportPath,
    [string]$OutputPath
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Get-Tw14PopupWindows {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]$Report,
        [Parameter(Mandatory)][uint32[]]$ExpectedProcessIds,
        [Parameter(Mandatory)][string]$PrimaryHwnd
    )

    $expected = [System.Collections.Generic.HashSet[uint32]]::new()
    foreach ($processId in $ExpectedProcessIds) { [void]$expected.Add($processId) }

    return @($Report.windows | Where-Object {
        $window = $_
        if ([string]$window.hwnd -eq $PrimaryHwnd) { return $false }
        foreach ($snapshot in @($window.snapshots)) {
            if ($expected.Contains([uint32]$snapshot.pid) -and [bool]$snapshot.isWindowVisible) {
                return $true
            }
        }
        return $false
    })
}

function Assert-Tw14PopupReport {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]$Report,
        [Parameter(Mandatory)][uint32[]]$ExpectedProcessIds,
        [Parameter(Mandatory)][string]$PrimaryHwnd
    )

    $popups = @(Get-Tw14PopupWindows -Report $Report -ExpectedProcessIds $ExpectedProcessIds -PrimaryHwnd $PrimaryHwnd)
    if ($popups.Count -eq 0) {
        throw 'TW-14_POPUP_NOT_OBSERVED: no visible separately-owned top-level HWND was observed'
    }
    return $popups
}

function Invoke-Tw14PrintPopup {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][uint32[]]$ExpectedProcessIds,
        [Parameter(Mandatory)][string]$PrimaryHwnd,
        [Parameter(Mandatory)][int]$RunDurationMs,
        [Parameter(Mandatory)][int]$IntervalMs
    )

    # Input injection is guarded by vmqa-win32.ps1's Azure-VM allow-list. Do not replace this
    # with SendKeys alone: TW-14 must never steal a developer's desktop keyboard.
    . (Join-Path $PSScriptRoot 'vmqa-win32.ps1')
    $primary = [IntPtr]::Parse($PrimaryHwnd.Substring(2), [Globalization.NumberStyles]::HexNumber)
    [uint32]$primaryProcessId = 0
    [void][VmqaNative]::GetWindowThreadProcessId($primary, [ref]$primaryProcessId)
    if ($ExpectedProcessIds -notcontains $primaryProcessId) {
        throw "TW-14_PRIMARY_WINDOW_PID_MISMATCH: $PrimaryHwnd belongs to pid $primaryProcessId"
    }
    if (-not [VmqaNative]::SetForegroundWindow($primary)) {
        throw "TW-14_FOREGROUND_REFUSED: Windows refused to foreground $PrimaryHwnd"
    }
    Start-Sleep -Milliseconds 200
    if ([VmqaNative]::GetForegroundWindow() -ne $primary) {
        throw "TW-14_FOREGROUND_UNCONFIRMED: $PrimaryHwnd did not own the foreground"
    }

    # Ctrl+P is intentionally used instead of a browser-specific URL or DevTools protocol: it is
    # a user-visible browser popup and works for every borrowed browser the VM image supplies.
    [System.Windows.Forms.SendKeys]::SendWait('^p')
    Start-Sleep -Milliseconds 300
    & (Join-Path $PSScriptRoot 'winstate-transient.ps1') -ProcessId $ExpectedProcessIds `
        -DurationMs $RunDurationMs -SampleIntervalMs $IntervalMs | ConvertFrom-Json
}

if ($FixtureReportPath) {
    $report = Get-Content -Raw -LiteralPath $FixtureReportPath | ConvertFrom-Json
} else {
    $report = Invoke-Tw14PrintPopup -ExpectedProcessIds $BorrowedProcessId -PrimaryHwnd $PrimaryWindowHwnd `
        -RunDurationMs $DurationMs -IntervalMs $SampleIntervalMs
}

$popups = @(Assert-Tw14PopupReport -Report $report -ExpectedProcessIds $BorrowedProcessId -PrimaryHwnd $PrimaryWindowHwnd)
$result = [ordered]@{
    test = 'TW-14'
    verdict = 'pass'
    primaryWindowHwnd = $PrimaryWindowHwnd
    popupWindows = @($popups | ForEach-Object {
        [ordered]@{
            hwnd = $_.hwnd
            firstSeenMs = $_.firstSeenMs
            departedAtMs = $_.departedAtMs
            snapshots = @($_.snapshots)
        }
    })
}
$json = $result | ConvertTo-Json -Depth 8
if ($OutputPath) { Set-Content -LiteralPath $OutputPath -Value $json -Encoding utf8 }
$json
