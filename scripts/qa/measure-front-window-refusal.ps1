param(
  [Parameter(Mandatory = $true)]
  [string]$OutputPath
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

Add-Type @"
using System;
using System.Text;
using System.Runtime.InteropServices;

public static class OslFrontWindowRefusalProbe {
  public delegate bool EnumWindowsProc(IntPtr hWnd, IntPtr lParam);

  [DllImport("user32.dll")]
  public static extern bool EnumWindows(EnumWindowsProc lpEnumFunc, IntPtr lParam);

  [DllImport("user32.dll")]
  public static extern bool IsWindowVisible(IntPtr hWnd);

  [DllImport("user32.dll", CharSet=CharSet.Unicode)]
  public static extern int GetWindowText(IntPtr hWnd, StringBuilder text, int count);

  [DllImport("user32.dll")]
  public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint pid);

  [DllImport("user32.dll")]
  public static extern IntPtr GetForegroundWindow();

  [DllImport("user32.dll")]
  public static extern bool SetForegroundWindow(IntPtr hWnd);
}
"@

function Get-WindowInfo {
  param([Parameter(Mandatory = $true)][IntPtr]$Hwnd)

  $pidValue = [uint32]0
  [void][OslFrontWindowRefusalProbe]::GetWindowThreadProcessId($Hwnd, [ref]$pidValue)
  $process = if ($pidValue -ne 0) {
    Get-Process -Id $pidValue -ErrorAction SilentlyContinue
  } else {
    $null
  }
  $title = New-Object Text.StringBuilder 512
  [void][OslFrontWindowRefusalProbe]::GetWindowText($Hwnd, $title, $title.Capacity)

  [pscustomobject]@{
    hwnd = $Hwnd.ToInt64()
    pid = $pidValue
    process = if ($null -ne $process) { $process.ProcessName } else { $null }
    title = $title.ToString()
  }
}

$discord = [IntPtr]::Zero
[OslFrontWindowRefusalProbe]::EnumWindows({
  param($hwnd, $lparam)

  if ([OslFrontWindowRefusalProbe]::IsWindowVisible($hwnd)) {
    $windowPid = [uint32]0
    [void][OslFrontWindowRefusalProbe]::GetWindowThreadProcessId($hwnd, [ref]$windowPid)
    $process = if ($windowPid -ne 0) {
      Get-Process -Id $windowPid -ErrorAction SilentlyContinue
    } else {
      $null
    }
    $title = New-Object Text.StringBuilder 512
    [void][OslFrontWindowRefusalProbe]::GetWindowText($hwnd, $title, $title.Capacity)
    if ($null -ne $process -and $process.ProcessName -eq 'Discord' -and $title.ToString() -match 'Discord') {
      $script:discord = $hwnd
      return $false
    }
  }
  return $true
}, [IntPtr]::Zero) | Out-Null

if ($discord -eq [IntPtr]::Zero) {
  throw 'No visible Discord top-level window was found.'
}

$foregroundBefore = [OslFrontWindowRefusalProbe]::GetForegroundWindow()
Start-Sleep -Milliseconds 250
$setForegroundReturned = [OslFrontWindowRefusalProbe]::SetForegroundWindow($discord)
Start-Sleep -Milliseconds 250
$actualForeground = [OslFrontWindowRefusalProbe]::GetForegroundWindow()
$refusalCount = if ($actualForeground -eq $discord) { 0 } else { 1 }

$requested = Get-WindowInfo -Hwnd $discord
$before = Get-WindowInfo -Hwnd $foregroundBefore
$actual = Get-WindowInfo -Hwnd $actualForeground

$lines = @(
  'starting measurement only - this does not prove the new front-window grab works',
  "windows build: $([Environment]::OSVersion.VersionString)",
  "background probe pid: $PID",
  'alt press before request: none',
  'setforegroundwindow call count: 1',
  "setforegroundwindow returned: $setForegroundReturned",
  "requested window: hwnd=$($requested.hwnd) pid=$($requested.pid) process=$($requested.process) title=`"$($requested.title)`"",
  "foreground before request: hwnd=$($before.hwnd) pid=$($before.pid) process=$($before.process) title=`"$($before.title)`"",
  "actual foreground window: hwnd=$($actual.hwnd) pid=$($actual.pid) process=$($actual.process) title=`"$($actual.title)`"",
  "refusal count: $refusalCount"
)

$fullOutputPath = [IO.Path]::GetFullPath($OutputPath)
$parent = [IO.Path]::GetDirectoryName($fullOutputPath)
if (-not [string]::IsNullOrWhiteSpace($parent)) {
  New-Item -ItemType Directory -Force -Path $parent | Out-Null
}
$utf8NoBom = New-Object Text.UTF8Encoding $false
[IO.File]::WriteAllLines($fullOutputPath, $lines, $utf8NoBom)
$lines -join [Environment]::NewLine
