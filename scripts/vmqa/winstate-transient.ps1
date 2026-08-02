<#
.SYNOPSIS
  Records every top-level HWND owned by selected processes while a transition runs.

.DESCRIPTION
  Unlike a single window-state snapshot, this sampler keeps windows which have already
  disappeared. That makes short-lived WebView2 tooltip and native select HWNDs observable.
  The normal mode samples the desktop every 50 ms. FixtureSamplesPath is deliberately provided
  for deterministic contract tests; it uses the same lifecycle reducer as live sampling.
#>

[CmdletBinding()]
param(
    [uint32[]]$ProcessId,
    [ValidateRange(50, 600000)][int]$DurationMs = 1000,
    [ValidateRange(10, 1000)][int]$SampleIntervalMs = 50,
    [string]$OutputPath,
    [string]$FixtureSamplesPath
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function ConvertTo-TransientWindowReport {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][object[]]$Samples,
        [Parameter(Mandatory)][int]$IntervalMs
    )

    $active = @{}
    $completed = [System.Collections.Generic.List[object]]::new()
    foreach ($sample in $Samples) {
        $seen = @{}
        foreach ($window in @($sample.windows)) {
            $key = [string]$window.hwnd
            if ([string]::IsNullOrWhiteSpace($key)) {
                throw 'WINSTATE_TRANSIENT_INVALID_SAMPLE: window has no hwnd'
            }
            $seen[$key] = $true
            if (-not $active.ContainsKey($key)) {
                $active[$key] = [ordered]@{
                    hwnd = $key
                    firstSeenMs = [int]$sample.elapsedMs
                    lastSeenMs = [int]$sample.elapsedMs
                    sampleCount = 0
                    snapshots = [System.Collections.Generic.List[object]]::new()
                }
            }
            $entry = $active[$key]
            $entry.lastSeenMs = [int]$sample.elapsedMs
            $entry.sampleCount++
            $entry.snapshots.Add($window)
        }
        foreach ($key in @($active.Keys)) {
            if (-not $seen.ContainsKey($key)) {
                $entry = $active[$key]
                $entry['departedAtMs'] = [int]$sample.elapsedMs
                $entry['transient'] = $true
                $completed.Add([pscustomobject]$entry)
                $active.Remove($key)
            }
        }
    }
    foreach ($key in @($active.Keys)) {
        $entry = $active[$key]
        $entry['departedAtMs'] = $null
        $entry['transient'] = $false
        $completed.Add([pscustomobject]$entry)
    }
    return [ordered]@{
        schemaVersion = 1
        sampleIntervalMs = $IntervalMs
        sampleCount = $Samples.Count
        windows = @($completed | Sort-Object firstSeenMs, hwnd)
    }
}

function Get-LiveTransientSamples {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][uint32[]]$WantedProcessIds,
        [Parameter(Mandatory)][int]$RunDurationMs,
        [Parameter(Mandatory)][int]$IntervalMs
    )

    if ($WantedProcessIds.Count -eq 0) { throw 'WINSTATE_TRANSIENT_PROCESS_ID_REQUIRED' }
    if (-not ('VmqaTransientNative' -as [type])) {
        Add-Type @"
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;
public static class VmqaTransientNative {
  public delegate bool EnumProc(IntPtr h, IntPtr l);
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
  [StructLayout(LayoutKind.Sequential, CharSet=CharSet.Unicode)] public struct MONITORINFOEX { public int cbSize; public RECT rcMonitor, rcWork; public int dwFlags; [MarshalAs(UnmanagedType.ByValTStr, SizeConst=32)] public string szDevice; }
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc cb, IntPtr l);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT rect);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr h);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetClassName(IntPtr h, StringBuilder b, int n);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr h, StringBuilder b, int n);
  [DllImport("user32.dll", EntryPoint="GetWindowLongPtrW")] public static extern IntPtr GetWindowLongPtr(IntPtr h, int index);
  [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern IntPtr MonitorFromWindow(IntPtr h, uint flags);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern bool GetMonitorInfo(IntPtr monitor, ref MONITORINFOEX info);
  const int GWL_STYLE=-16, GWL_EXSTYLE=-20, GWLP_HWNDPARENT=-8; const uint MONITOR_DEFAULTTONEAREST=2;
  public static List<Dictionary<string,object>> Snapshot(HashSet<uint> wanted) {
    var result=new List<Dictionary<string,object>>(); int z=0; bool failed=false;
    bool complete=EnumWindows((h,l)=> { try { uint pid; if(GetWindowThreadProcessId(h,out pid)==0) throw new Exception("GetWindowThreadProcessId"); if(!wanted.Contains(pid)){z++; return true;} var r=new RECT(); if(!GetWindowRect(h,out r)) throw new Exception("GetWindowRect"); var cls=new StringBuilder(512); var title=new StringBuilder(1024); GetClassName(h,cls,cls.Capacity); GetWindowText(h,title,title.Capacity); var mi=new MONITORINFOEX(); mi.cbSize=Marshal.SizeOf(typeof(MONITORINFOEX)); var monitor=MonitorFromWindow(h,MONITOR_DEFAULTTONEAREST); if(monitor!=IntPtr.Zero) GetMonitorInfo(monitor,ref mi); result.Add(new Dictionary<string,object>{{"hwnd","0x"+h.ToInt64().ToString("X")},{"pid",pid},{"className",cls.ToString()},{"title",title.ToString()},{"style","0x"+GetWindowLongPtr(h,GWL_STYLE).ToInt64().ToString("X")},{"exStyle","0x"+GetWindowLongPtr(h,GWL_EXSTYLE).ToInt64().ToString("X")},{"ownerHwnd","0x"+GetWindowLongPtr(h,GWLP_HWNDPARENT).ToInt64().ToString("X")},{"rect",new Dictionary<string,int>{{"left",r.Left},{"top",r.Top},{"right",r.Right},{"bottom",r.Bottom}}},{"isIconic",IsIconic(h)},{"isWindowVisible",IsWindowVisible(h)},{"zOrderIndex",z},{"dpi",GetDpiForWindow(h)},{"monitor",mi.szDevice ?? ""}}); z++; } catch { failed=true; return false; } return true; },IntPtr.Zero); if(!complete||failed) throw new Exception("WINSTATE_TRANSIENT_ENUMERATION_FAILED"); return result;
  }
}
"@
    }

    $wanted = [System.Collections.Generic.HashSet[uint32]]::new()
    foreach ($id in $WantedProcessIds) { [void]$wanted.Add($id) }
    $watch = [Diagnostics.Stopwatch]::StartNew()
    $samples = [System.Collections.Generic.List[object]]::new()
    do {
        $samples.Add([ordered]@{ elapsedMs = [int]$watch.ElapsedMilliseconds; windows = @([VmqaTransientNative]::Snapshot($wanted)) })
        if ($watch.ElapsedMilliseconds -ge $RunDurationMs) { break }
        Start-Sleep -Milliseconds $IntervalMs
    } while ($true)
    return @($samples)
}

if ($FixtureSamplesPath) {
    $fixture = Get-Content -Raw -LiteralPath $FixtureSamplesPath | ConvertFrom-Json
    $samples = @($fixture.samples)
} else {
    $samples = Get-LiveTransientSamples -WantedProcessIds $ProcessId -RunDurationMs $DurationMs -IntervalMs $SampleIntervalMs
}
$report = ConvertTo-TransientWindowReport -Samples $samples -IntervalMs $SampleIntervalMs
$json = $report | ConvertTo-Json -Depth 8
if ($OutputPath) { Set-Content -LiteralPath $OutputPath -Value $json -Encoding utf8 }
$json
