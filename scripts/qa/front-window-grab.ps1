param(
  [Parameter(Mandatory = $true)]
  [string]$WindowName,

  [Parameter(Mandatory = $true)]
  [int]$WindowNumber,

  [int]$HoldMilliseconds = 0
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

Add-Type @"
using System;
using System.Text;
using System.Runtime.InteropServices;

public static class OslFrontWindowGrab {
  public delegate bool EnumWindowsProc(IntPtr hWnd, IntPtr lParam);

  [StructLayout(LayoutKind.Sequential)]
  public struct INPUT {
    public UInt32 type;
    public InputUnion U;
  }

  [StructLayout(LayoutKind.Explicit)]
  public struct InputUnion {
    [FieldOffset(0)]
    public MOUSEINPUT mi;
    [FieldOffset(0)]
    public KEYBDINPUT ki;
    [FieldOffset(0)]
    public HARDWAREINPUT hi;
  }

  [StructLayout(LayoutKind.Sequential)]
  public struct MOUSEINPUT {
    public Int32 dx;
    public Int32 dy;
    public UInt32 mouseData;
    public UInt32 dwFlags;
    public UInt32 time;
    public UIntPtr dwExtraInfo;
  }

  [StructLayout(LayoutKind.Sequential)]
  public struct KEYBDINPUT {
    public UInt16 wVk;
    public UInt16 wScan;
    public UInt32 dwFlags;
    public UInt32 time;
    public UIntPtr dwExtraInfo;
  }

  [StructLayout(LayoutKind.Sequential)]
  public struct HARDWAREINPUT {
    public UInt32 uMsg;
    public UInt16 wParamL;
    public UInt16 wParamH;
  }

  public const UInt32 INPUT_KEYBOARD = 1;
  public const UInt32 KEYEVENTF_KEYUP = 0x0002;
  public const UInt16 VK_MENU = 0x12;

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

  [DllImport("user32.dll", SetLastError=true)]
  public static extern UInt32 SendInput(UInt32 nInputs, INPUT[] pInputs, Int32 cbSize);

  [DllImport("kernel32.dll")]
  public static extern UInt32 GetLastError();

  public static UInt32 SendOneAltPressAndRelease() {
    INPUT down = new INPUT();
    down.type = INPUT_KEYBOARD;
    down.U.ki.wVk = VK_MENU;
    down.U.ki.wScan = 0;
    down.U.ki.dwFlags = 0;
    down.U.ki.time = 0;
    down.U.ki.dwExtraInfo = UIntPtr.Zero;

    INPUT up = new INPUT();
    up.type = INPUT_KEYBOARD;
    up.U.ki.wVk = VK_MENU;
    up.U.ki.wScan = 0;
    up.U.ki.dwFlags = KEYEVENTF_KEYUP;
    up.U.ki.time = 0;
    up.U.ki.dwExtraInfo = UIntPtr.Zero;

    INPUT[] inputs = new INPUT[] { down, up };
    return SendInput((UInt32)inputs.Length, inputs, Marshal.SizeOf(typeof(INPUT)));
  }
}
"@

function Get-WindowInfo {
  param([Parameter(Mandatory = $true)][IntPtr]$Hwnd)

  $pidValue = [uint32]0
  if ($Hwnd -ne [IntPtr]::Zero) {
    [void][OslFrontWindowGrab]::GetWindowThreadProcessId($Hwnd, [ref]$pidValue)
  }
  $process = if ($pidValue -ne 0) {
    Get-Process -Id $pidValue -ErrorAction SilentlyContinue
  } else {
    $null
  }
  $title = New-Object Text.StringBuilder 512
  if ($Hwnd -ne [IntPtr]::Zero) {
    [void][OslFrontWindowGrab]::GetWindowText($Hwnd, $title, $title.Capacity)
  }

  [pscustomobject]@{
    hwnd = $Hwnd.ToInt64()
    pid = $pidValue
    process = if ($null -ne $process) { $process.ProcessName } else { $null }
    title = $title.ToString()
  }
}

function Send-OneAltPressAndRelease {
  [OslFrontWindowGrab]::SendOneAltPressAndRelease()
}

function Write-ResultAndExit {
  param(
    [Parameter(Mandatory = $true)][object]$Result,
    [Parameter(Mandatory = $true)][int]$Code
  )

  $Result | ConvertTo-Json -Compress -Depth 6
  exit $Code
}

if ($WindowNumber -lt 1) {
  Write-ResultAndExit ([pscustomobject]@{
    ok = $false
    command = 'frontWindowGrab'
    windowName = $WindowName
    windowNumber = $WindowNumber
    matchingWindowCount = 0
    error = '--WindowNumber is one-based'
  }) 1
}

$all = New-Object System.Collections.Generic.List[object]
[OslFrontWindowGrab]::EnumWindows({
  param($hwnd, $lparam)

  if ([OslFrontWindowGrab]::IsWindowVisible($hwnd)) {
    $info = Get-WindowInfo -Hwnd $hwnd
    if (-not [string]::IsNullOrWhiteSpace($info.title) -or $null -ne $info.process) {
      $script:all.Add($info)
    }
  }
  return $true
}, [IntPtr]::Zero) | Out-Null

$processMatches = @($all | Where-Object { $null -ne $_.process -and $_.process -ieq $WindowName })
$matchedWindows = @(if ($processMatches.Count -gt 0) {
  @($processMatches)
} else {
  @($all | Where-Object { $_.title -like "*$WindowName*" })
})

if ($WindowNumber -gt $matchedWindows.Count) {
  Write-ResultAndExit ([pscustomobject]@{
    ok = $false
    command = 'frontWindowGrab'
    windowName = $WindowName
    windowNumber = $WindowNumber
    matchingWindowCount = $matchedWindows.Count
    error = "$WindowName window #$WindowNumber was not found"
  }) 1
}

$requested = $matchedWindows[$WindowNumber - 1]
$altAccepted = Send-OneAltPressAndRelease
$sendInputLastError = if ($altAccepted -eq 2) { 0 } else { [OslFrontWindowGrab]::GetLastError() }
$setReturned = [OslFrontWindowGrab]::SetForegroundWindow([IntPtr]$requested.hwnd)
Start-Sleep -Milliseconds 250
$foreground = Get-WindowInfo -Hwnd ([OslFrontWindowGrab]::GetForegroundWindow())
$matched = $foreground.hwnd -eq $requested.hwnd
if ($HoldMilliseconds -gt 0) {
  Start-Sleep -Milliseconds $HoldMilliseconds
}

Write-ResultAndExit ([pscustomobject]@{
  ok = $matched
  command = 'frontWindowGrab'
  windowName = $WindowName
  windowNumber = $WindowNumber
  altPressCount = 1
  altReleaseCount = 1
  sendInputAccepted = $altAccepted
  sendInputLastError = $sendInputLastError
  setForegroundWindowCallCount = 1
  setForegroundWindowReturned = $setReturned
  readbackSource = 'GetForegroundWindow'
  requestedWindow = $requested
  foregroundAfter = $foreground
  foregroundMatched = $matched
}) $(if ($matched) { 0 } else { 1 })
