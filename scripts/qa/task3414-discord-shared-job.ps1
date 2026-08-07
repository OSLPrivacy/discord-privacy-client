[CmdletBinding()]
param(
  [Parameter(Mandatory = $true)]
  [string]$PlaceTextExe,

  [string]$Text = "",

  [ValidateRange(30, 5000)]
  [int]$SettleMilliseconds = 350
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;

public static class Task3414Win32 {
  public delegate bool EnumWindowsProc(IntPtr hwnd, IntPtr lparam);

  [StructLayout(LayoutKind.Sequential)]
  public struct Rect {
    public int Left;
    public int Top;
    public int Right;
    public int Bottom;
  }

  [DllImport("user32.dll")]
  public static extern bool EnumWindows(EnumWindowsProc callback, IntPtr lparam);
  [DllImport("user32.dll")]
  public static extern bool IsWindowVisible(IntPtr hwnd);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)]
  public static extern int GetWindowText(IntPtr hwnd, StringBuilder value, int capacity);
  [DllImport("user32.dll")]
  public static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint processId);
  [DllImport("user32.dll")]
  public static extern bool GetWindowRect(IntPtr hwnd, out Rect rect);
  [DllImport("user32.dll")]
  public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")]
  public static extern bool SetForegroundWindow(IntPtr hwnd);
  [DllImport("user32.dll")]
  public static extern bool ShowWindow(IntPtr hwnd, int command);
  [DllImport("user32.dll")]
  public static extern bool SetWindowPos(
    IntPtr hwnd, IntPtr after, int x, int y, int width, int height, uint flags
  );
  [DllImport("user32.dll")]
  public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")]
  public static extern void mouse_event(uint flags, uint dx, uint dy, uint data, UIntPtr extra);
  [DllImport("user32.dll")]
  public static extern int GetSystemMetrics(int metric);
}
'@
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes

function Get-WindowTitle([IntPtr]$Hwnd) {
  $buffer = [Text.StringBuilder]::new(512)
  [void][Task3414Win32]::GetWindowText($Hwnd, $buffer, $buffer.Capacity)
  $buffer.ToString()
}

function Get-WindowRecords {
  $records = [Collections.Generic.List[object]]::new()
  [Task3414Win32]::EnumWindows({
    param([IntPtr]$hwnd, [IntPtr]$ignored)
    if (-not [Task3414Win32]::IsWindowVisible($hwnd)) { return $true }
    $title = Get-WindowTitle $hwnd
    if ([string]::IsNullOrWhiteSpace($title)) { return $true }
    [uint32]$windowPid = 0
    [void][Task3414Win32]::GetWindowThreadProcessId($hwnd, [ref]$windowPid)
    $process = Get-Process -Id ([int]$windowPid) -ErrorAction SilentlyContinue
    $rect = New-Object Task3414Win32+Rect
    [void][Task3414Win32]::GetWindowRect($hwnd, [ref]$rect)
    $records.Add([pscustomobject]@{
      Hwnd = $hwnd
      ProcessName = if ($process) { $process.ProcessName } else { "" }
      Title = $title
      Left = $rect.Left
      Top = $rect.Top
      Width = $rect.Right - $rect.Left
      Height = $rect.Bottom - $rect.Top
    })
    $true
  }, [IntPtr]::Zero) | Out-Null
  @($records)
}

function Get-WindowRecordForHwnd([IntPtr]$Hwnd) {
  $title = Get-WindowTitle $Hwnd
  [uint32]$windowPid = 0
  [void][Task3414Win32]::GetWindowThreadProcessId($Hwnd, [ref]$windowPid)
  $process = Get-Process -Id ([int]$windowPid) -ErrorAction SilentlyContinue
  $rect = New-Object Task3414Win32+Rect
  [void][Task3414Win32]::GetWindowRect($Hwnd, [ref]$rect)
  [pscustomobject]@{
    Hwnd = $Hwnd
    ProcessName = if ($process) { $process.ProcessName } else { "" }
    Title = $title
    Left = $rect.Left
    Top = $rect.Top
    Width = $rect.Right - $rect.Left
    Height = $rect.Bottom - $rect.Top
  }
}

function Get-ForegroundRecord {
  $hwnd = [Task3414Win32]::GetForegroundWindow()
  if ($hwnd -eq [IntPtr]::Zero) { throw 'Windows reported no foreground window' }
  Get-WindowRecordForHwnd $hwnd
}

function Click-DiscordComposerBand([object]$Window) {
  $x = [int]($Window.Left + ($Window.Width / 2))
  $y = [int]($Window.Top + [Math]::Max(20, $Window.Height - 42))
  [void][Task3414Win32]::SetCursorPos($x, $y)
  [Task3414Win32]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
  Start-Sleep -Milliseconds 40
  [Task3414Win32]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
}

function Open-FirstDirectMessage([object]$Window) {
  $root = [Windows.Automation.AutomationElement]::FromHandle([IntPtr]$Window.Hwnd)
  $condition = [Windows.Automation.PropertyCondition]::new(
    [Windows.Automation.AutomationElement]::ControlTypeProperty,
    [Windows.Automation.ControlType]::Hyperlink
  )
  $links = $root.FindAll([Windows.Automation.TreeScope]::Descendants, $condition)
  for ($index = 0; $index -lt $links.Count; $index += 1) {
    $link = $links.Item($index)
    $name = [string]$link.Current.Name
    if ($name -notmatch '\(direct message\)' -or $link.Current.IsOffscreen) {
      continue
    }
    $rect = $link.Current.BoundingRectangle
    if ($rect.IsEmpty -or $rect.Width -lt 20 -or $rect.Height -lt 20) {
      continue
    }
    [void][Task3414Win32]::SetCursorPos(
      [int]($rect.Left + ($rect.Width / 2)),
      [int]($rect.Top + ($rect.Height / 2))
    )
    [Task3414Win32]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 40
    [Task3414Win32]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
    Write-Output "TASK3414_OPENED_DM=$name"
    return
  }
  Write-Output "TASK3414_OPENED_DM=already-open-or-none-visible"
}

function Find-DiscordWindow {
  $matches = @(Get-WindowRecords | Where-Object {
    $_.ProcessName -in @('Discord', 'DiscordPTB', 'DiscordCanary') -or
      $_.Title -match '(?i)Discord'
  })
  if ($matches.Count -ne 1) {
    throw "Discord window count was $($matches.Count), expected 1"
  }
  $matches[0]
}

function Find-InitialFrontWindow {
  $preferred = @(Get-WindowRecords | Where-Object {
    $_.ProcessName -in @('Photos', 'ApplicationFrameHost', 'Microsoft.Media.Player') -and
      $_.Title -match '(?i)Photos|Media Player'
  } | Select-Object -First 1)
  if ($preferred.Count -eq 1) {
    return [pscustomobject]@{ Window = $preferred[0]; Match = 'Photos' }
  }
  $fallback = @(Get-WindowRecords | Where-Object {
    $_.ProcessName -notin @('Discord', 'DiscordPTB', 'DiscordCanary') -and
      $_.Title -notmatch '(?i)Discord'
  } | Select-Object -First 1)
  if ($fallback.Count -ne 1) { throw 'no non-Discord foreground candidate is available' }
  [pscustomobject]@{ Window = $fallback[0]; Match = $fallback[0].ProcessName }
}

function Invoke-Placement([string]$InitialMatch, [string]$Mark, [bool]$AllowAlreadyFront = $false) {
  $previousPreference = $ErrorActionPreference
  $ErrorActionPreference = 'Continue'
  try {
    $arguments = @('--initial-front', $InitialMatch, '--app', 'Discord', '--text', $Mark)
    if ($AllowAlreadyFront) { $arguments += '--allow-already-front' }
    $output = & $PlaceTextExe @arguments 2>&1
  } finally {
    $ErrorActionPreference = $previousPreference
  }
  [pscustomobject]@{
    ExitCode = [int]$LASTEXITCODE
    Output = @($output | ForEach-Object { [string]$_ })
  }
}

if (-not [IO.File]::Exists($PlaceTextExe)) {
  throw "3406 placement executable is absent: $PlaceTextExe"
}

if ([string]::IsNullOrEmpty($Text)) {
  $Text = 'OSL-3414-' + [Guid]::NewGuid().ToString('N')
}

$discord = Find-DiscordWindow
[void][Task3414Win32]::ShowWindow([IntPtr]$discord.Hwnd, 9)
Open-FirstDirectMessage $discord
Start-Sleep -Milliseconds $SettleMilliseconds
$discord = Find-DiscordWindow
Click-DiscordComposerBand $discord
Start-Sleep -Milliseconds $SettleMilliseconds
$positiveFront = Get-ForegroundRecord
if ($positiveFront.ProcessName -notin @('Discord', 'DiscordPTB', 'DiscordCanary') -and
    $positiveFront.Title -notmatch '(?i)Discord') {
  throw "Discord did not become foreground for positive run: $($positiveFront.ProcessName):$($positiveFront.Title)"
}
$initial = [pscustomobject]@{
  Window = $positiveFront
  Match = 'Discord'
}

Write-Output "TASK3414_MARK=$Text"
Write-Output "TASK3414_INITIAL_MATCH=$($initial.Match)"
Write-Output "TASK3414_INITIAL_FRONT=$($initial.Window.ProcessName):$($initial.Window.Title)"
$positive = Invoke-Placement $initial.Match $Text $true
$positive.Output | ForEach-Object { Write-Output "TASK3414_POSITIVE> $_" }
Write-Output "TASK3414_POSITIVE_EXIT=$($positive.ExitCode)"

$discord = Find-DiscordWindow
$original = [pscustomobject]@{
  Left = $discord.Left
  Top = $discord.Top
  Width = $discord.Width
  Height = $discord.Height
}
$moved = $false
try {
  $offX = [Task3414Win32]::GetSystemMetrics(76) + [Task3414Win32]::GetSystemMetrics(78) + 400
  $offY = [Task3414Win32]::GetSystemMetrics(77) + [Task3414Win32]::GetSystemMetrics(79) + 400
  [void][Task3414Win32]::ShowWindow([IntPtr]$discord.Hwnd, 9)
  $moved = [Task3414Win32]::SetWindowPos(
    [IntPtr]$discord.Hwnd,
    [IntPtr]::Zero,
    $offX,
    $offY,
    [Math]::Max(320, $original.Width),
    [Math]::Max(240, $original.Height),
    0x0014
  )
  Write-Output "TASK3414_OFFSCREEN_MOVED=$moved"

  $offscreenInitial = Find-InitialFrontWindow
  [void][Task3414Win32]::SetForegroundWindow([IntPtr]$offscreenInitial.Window.Hwnd)
  Start-Sleep -Milliseconds $SettleMilliseconds
  $actualOffscreenFront = Get-ForegroundRecord
  $offscreenInitial = [pscustomobject]@{
    Window = $actualOffscreenFront
    Match = if ([string]::IsNullOrWhiteSpace($actualOffscreenFront.ProcessName)) {
      $actualOffscreenFront.Title
    } else {
      $actualOffscreenFront.ProcessName
    }
  }
  Write-Output "TASK3414_OFFSCREEN_INITIAL_MATCH=$($offscreenInitial.Match)"
  Write-Output "TASK3414_OFFSCREEN_INITIAL_FRONT=$($offscreenInitial.Window.ProcessName):$($offscreenInitial.Window.Title)"
  $offscreen = Invoke-Placement $offscreenInitial.Match $Text $true
  $offscreen.Output | ForEach-Object { Write-Output "TASK3414_OFFSCREEN> $_" }
  Write-Output "TASK3414_OFFSCREEN_EXIT=$($offscreen.ExitCode)"
} finally {
  if ($moved) {
    [void][Task3414Win32]::SetWindowPos(
      [IntPtr]$discord.Hwnd,
      [IntPtr]::Zero,
      $original.Left,
      $original.Top,
      [Math]::Max(320, $original.Width),
      [Math]::Max(240, $original.Height),
      0x0014
    )
  }
}

if ($positive.ExitCode -ne 0) { exit $positive.ExitCode }
if ($offscreen.ExitCode -ne 1) { exit 1 }
if (-not (@($offscreen.Output) -match 'Discord window is off screen')) { exit 1 }
exit 0
