[CmdletBinding()]
param(
  [Parameter(Mandatory = $true)]
  [Int64]$Hwnd,
  # The VM-specific driver supplies the live Settings/UIAutomation operation.
  # It must return only after Windows has applied the display-scale change.
  [scriptblock]$ScaleChangeCommand,
  [string]$ResultsDirectory = (Join-Path $PSScriptRoot "results")
)

$ErrorActionPreference = "Stop"
New-Item -ItemType Directory -Force -Path $ResultsDirectory | Out-Null

function Write-DpiResult([string]$Status, [string]$Reason, $Before, $After) {
  $result = [ordered]@{
    schema = "vmqa-live-dpi-change/v1"
    test = "TW-8"
    status = $Status
    reason = $Reason
    hwnd = ("0x{0:X}" -f $Hwnd)
    before = $Before
    after = $After
    collected_at_utc = [DateTime]::UtcNow.ToString("o")
  }
  $path = Join-Path $ResultsDirectory "dpi-change.json"
  $result | ConvertTo-Json -Depth 5 | Set-Content -NoNewline -Encoding utf8 $path
  Write-Output $path
  return $Status -eq "PASS"
}

if (-not $IsWindows) {
  [void](Write-DpiResult "UNTESTABLE" "TW-8 requires a Windows desktop session" $null $null)
  exit 2
}

Add-Type -AssemblyName System.Windows.Forms
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class DpiChangeNative {
  [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr hWnd);
  [DllImport("user32.dll")] [return: MarshalAs(UnmanagedType.Bool)]
  public static extern bool IsWindow(IntPtr hWnd);
}
'@

$handle = [IntPtr]$Hwnd
if (-not [DpiChangeNative]::IsWindow($handle)) {
  [void](Write-DpiResult "FAIL" "target HWND is not live" $null $null)
  exit 1
}

$screens = [System.Windows.Forms.Screen]::AllScreens
if ($screens.Count -lt 2 -and $null -eq $ScaleChangeCommand) {
  [void](Write-DpiResult "UNTESTABLE" "single-display VM has no supplied live scale-change driver" $null $null)
  exit 2
}
if ($null -eq $ScaleChangeCommand) {
  [void](Write-DpiResult "UNTESTABLE" "no live scale-change driver supplied" $null $null)
  exit 2
}

$before = [DpiChangeNative]::GetDpiForWindow($handle)
try {
  & $ScaleChangeCommand
} catch {
  [void](Write-DpiResult "FAIL" ("scale-change driver failed: " + $_.Exception.Message) $before $null)
  exit 1
}

# WM_DPICHANGED is asynchronous.  Bound the observation so a broken driver
# cannot be mistaken for success after an arbitrary sleep.
$deadline = [DateTime]::UtcNow.AddSeconds(15)
$after = $before
while ([DateTime]::UtcNow -lt $deadline) {
  if (-not [DpiChangeNative]::IsWindow($handle)) {
    [void](Write-DpiResult "FAIL" "target HWND disappeared during DPI transition" $before $null)
    exit 1
  }
  $after = [DpiChangeNative]::GetDpiForWindow($handle)
  if ($after -ne $before) { break }
  Start-Sleep -Milliseconds 100
}

if ($after -eq $before) {
  # This is the TW-8 anti-vacuity assertion: skipping the change can never
  # create a pass merely because the target stayed aligned.
  [void](Write-DpiResult "FAIL" "no live DPI change was observed after scale-change command" $before $after)
  exit 1
}

[void](Write-DpiResult "PASS" "live scale change observed on a stationary HWND" $before $after)
exit 0
