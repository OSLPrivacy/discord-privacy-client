<#
.SYNOPSIS
  Emits a machine-readable snapshot of every top-level window owned by selected processes.

.DESCRIPTION
  This is the stable observation primitive for the window-lifecycle matrix.  It deliberately
  records native window facts rather than inferring state from screenshots.
#>

[CmdletBinding()]
param(
    [Parameter(Mandatory)][uint32[]]$ProcessId,
    [string]$OutputPath,
    # Deterministic input for the contract test.  It is intentionally opt-in: normal operation
    # always queries the live desktop.
    [string]$FixtureWindowsPath
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function ConvertTo-WindowStateSnapshot {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][object[]]$Windows,
        [Parameter(Mandatory)][uint32[]]$ExpectedProcessIds
    )

    $expected = [System.Collections.Generic.HashSet[uint32]]::new()
    foreach ($processId in $ExpectedProcessIds) { [void]$expected.Add($processId) }

    $records = [System.Collections.Generic.List[object]]::new()
    foreach ($window in $Windows) {
        if (-not $expected.Contains([uint32]$window.processId)) { continue }
        foreach ($field in 'hwnd', 'class', 'style', 'exStyle', 'ownerHwnd', 'rect', 'isIconic', 'isWindowVisible', 'zOrderIndex', 'dpi', 'monitorId') {
            if ($null -eq $window.PSObject.Properties[$field]) {
                throw "WINSTATE_INVALID_WINDOW: missing $field for hwnd $($window.hwnd)"
            }
        }
        $records.Add([ordered]@{
            hwnd = [string]$window.hwnd
            processId = [uint32]$window.processId
            class = [string]$window.class
            style = [string]$window.style
            exStyle = [string]$window.exStyle
            ownerHwnd = [string]$window.ownerHwnd
            rect = [ordered]@{
                left = [int]$window.rect.left; top = [int]$window.rect.top
                right = [int]$window.rect.right; bottom = [int]$window.rect.bottom
            }
            isIconic = [bool]$window.isIconic
            isWindowVisible = [bool]$window.isWindowVisible
            zOrderIndex = [int]$window.zOrderIndex
            dpi = [uint32]$window.dpi
            monitorId = [string]$window.monitorId
        })
    }
    return [ordered]@{ schemaVersion = 1; processIds = @($ExpectedProcessIds); windows = @($records) }
}

function Get-LiveWindowState {
    [CmdletBinding()]
    param([Parameter(Mandatory)][uint32[]]$ExpectedProcessIds)

    if (-not ('VmqaWindowStateNative' -as [type])) {
        Add-Type @"
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;
public static class VmqaWindowStateNative {
  public delegate bool EnumProc(IntPtr h, IntPtr l);
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
  [StructLayout(LayoutKind.Sequential, CharSet=CharSet.Unicode)] public struct MONITORINFOEX { public int cbSize; public RECT rcMonitor, rcWork; public int dwFlags; [MarshalAs(UnmanagedType.ByValTStr, SizeConst=32)] public string szDevice; }
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc cb, IntPtr l);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT rect);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr h);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetClassName(IntPtr h, StringBuilder b, int n);
  [DllImport("user32.dll", EntryPoint="GetWindowLongPtrW")] public static extern IntPtr GetWindowLongPtr(IntPtr h, int index);
  [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern IntPtr MonitorFromWindow(IntPtr h, uint flags);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern bool GetMonitorInfo(IntPtr monitor, ref MONITORINFOEX info);
  const int GWL_STYLE=-16, GWL_EXSTYLE=-20, GWLP_HWNDPARENT=-8; const uint MONITOR_DEFAULTTONEAREST=2;
  public static List<Dictionary<string,object>> Snapshot(HashSet<uint> wanted) {
    var result = new List<Dictionary<string,object>>(); int z = 0; string error = null;
    bool complete = EnumWindows((h,l) => { try {
      uint pid; if (GetWindowThreadProcessId(h, out pid) == 0) throw new Exception("GetWindowThreadProcessId");
      if (wanted.Contains(pid)) { var rect = new RECT(); if (!GetWindowRect(h, out rect)) throw new Exception("GetWindowRect");
        var cls = new StringBuilder(512); if (GetClassName(h, cls, cls.Capacity) == 0) throw new Exception("GetClassName");
        var info = new MONITORINFOEX(); info.cbSize = Marshal.SizeOf(typeof(MONITORINFOEX)); var monitor = MonitorFromWindow(h, MONITOR_DEFAULTTONEAREST);
        if (monitor == IntPtr.Zero || !GetMonitorInfo(monitor, ref info)) throw new Exception("GetMonitorInfo");
        result.Add(new Dictionary<string,object>{{"hwnd", "0x"+h.ToInt64().ToString("X")}, {"processId", pid}, {"class", cls.ToString()}, {"style", "0x"+GetWindowLongPtr(h,GWL_STYLE).ToInt64().ToString("X")}, {"exStyle", "0x"+GetWindowLongPtr(h,GWL_EXSTYLE).ToInt64().ToString("X")}, {"ownerHwnd", "0x"+GetWindowLongPtr(h,GWLP_HWNDPARENT).ToInt64().ToString("X")}, {"rect", new Dictionary<string,int>{{"left",rect.Left},{"top",rect.Top},{"right",rect.Right},{"bottom",rect.Bottom}}}, {"isIconic", IsIconic(h)}, {"isWindowVisible", IsWindowVisible(h)}, {"zOrderIndex", z}, {"dpi", GetDpiForWindow(h)}, {"monitorId", info.szDevice ?? ""}}); }
      z++; return true;
    } catch (Exception ex) { error = ex.Message; return false; } }, IntPtr.Zero);
    if (!complete || error != null) throw new Exception("WINSTATE_ENUMERATION_FAILED: " + error);
    return result;
  }
}
"@
    }
    $wanted = [System.Collections.Generic.HashSet[uint32]]::new()
    foreach ($processId in $ExpectedProcessIds) { [void]$wanted.Add($processId) }
    return @([VmqaWindowStateNative]::Snapshot($wanted))
}

if ($FixtureWindowsPath) {
    $windows = @(Get-Content -Raw -LiteralPath $FixtureWindowsPath | ConvertFrom-Json)
} else {
    $windows = Get-LiveWindowState -ExpectedProcessIds $ProcessId
}
$snapshot = ConvertTo-WindowStateSnapshot -Windows $windows -ExpectedProcessIds $ProcessId
$json = $snapshot | ConvertTo-Json -Depth 5
if ($OutputPath) { Set-Content -LiteralPath $OutputPath -Value $json -Encoding utf8 }
$json
