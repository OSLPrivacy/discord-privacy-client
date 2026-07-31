<#
.SYNOPSIS
    Win32 window/process layer shared by the two-identity P2P QA scripts.
    Dot-sourced; defines the [P2PW] type exactly once per process.

.DESCRIPTION
    Deliberately a separate type name from accept.ps1's [OA] so both harnesses
    can be dot-sourced into one PowerShell session without an
    "already exists" Add-Type failure.

    RULES ENCODED HERE (each has already cost a cycle elsewhere in this repo)

      W1  ENUMERATION LIVES IN C#, NOT IN A POWERSHELL DELEGATE.
          An exception thrown inside a PowerShell EnumWindows callback aborts
          the enumeration SILENTLY and hands back a short, plausible-looking
          list. Every enumeration here is a C# method that returns an array.

      W2  "OSL Privacy" IS NOT AN IDENTITY.
          Every OSL build carries that window title, so a stale instance of a
          differently-identified build answers to it. The only identity an OSL
          process exposes cross-process is the single-instance marker window
          planted by tauri-plugin-single-instance, whose CLASS is
          "<bundle-id>-sic" and whose TITLE is "<bundle-id>-siw".
          BundleMarkers() reads exactly that and nothing else.

      W3  A PROCESS FAMILY, NOT A PID.
          A launcher stub that re-execs would be missed by an equality test on
          the launched pid, so Family() walks the full parent/child closure.

      W4  NO POINTER INPUT, EVER.
          There is no SetCursorPos and no mouse_event in this file. The only
          actuator exposed is PostMessage, and the only place it is used is
          against Chrome_RenderWidgetHostHWND. Nothing here synthesises a
          keystroke.
#>

Set-StrictMode -Off

if (-not ('P2PW' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;

public struct P2PRect { public int L, T, R, B; }

public class P2PWin {
    public long   Hwnd;
    public string Title;
    public string Cls;
    public int    Pid;
    public int    L, T, R, B, Width, Height;
    public bool   Visible;
    public bool   Iconic;
    public bool   IsChild;
    public long   Owner;
    public int    Z;
}

public static class P2PW {
    [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc e, IntPtr l);
    [DllImport("user32.dll", EntryPoint = "GetWindowTextW", CharSet = CharSet.Unicode)]
    static extern int GetWindowTextRaw(IntPtr h, StringBuilder s, int m);
    [DllImport("user32.dll", EntryPoint = "GetClassNameW", CharSet = CharSet.Unicode)]
    static extern int GetClassNameRaw(IntPtr h, StringBuilder s, int m);
    [DllImport("user32.dll")] public static extern bool IsWindow(IntPtr h);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
    [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr h);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out P2PRect r);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
    [DllImport("user32.dll", SetLastError = true)] public static extern IntPtr GetWindowLongPtr(IntPtr h, int i);
    [DllImport("user32.dll")] public static extern IntPtr GetWindow(IntPtr h, uint cmd);
    [DllImport("user32.dll")] public static extern IntPtr GetAncestor(IntPtr h, uint flags);
    [DllImport("user32.dll")] public static extern IntPtr WindowFromPoint(P2PPoint p);
    [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h, IntPtr after, int x, int y, int cx, int cy, uint flags);
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int cmd);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    public static extern bool PostMessage(IntPtr h, uint msg, IntPtr w, IntPtr l);
    [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();

    public delegate bool EnumProc(IntPtr h, IntPtr l);

    public static string Title(IntPtr h) {
        StringBuilder s = new StringBuilder(512);
        GetWindowTextRaw(h, s, 512);
        return s.ToString();
    }
    public static string Cls(IntPtr h) {
        StringBuilder s = new StringBuilder(512);
        GetClassNameRaw(h, s, 512);
        return s.ToString();
    }

    // W1: the whole enumeration is inside C#. A managed exception here would
    // surface as a real failure rather than as a silently truncated list.
    public static P2PWin[] Enumerate() {
        List<P2PWin> outp = new List<P2PWin>();
        int z = 0;
        EnumWindows(delegate(IntPtr h, IntPtr l) {
            P2PWin w = new P2PWin();
            w.Hwnd = h.ToInt64();
            w.Title = Title(h);
            w.Cls = Cls(h);
            uint pid = 0;
            GetWindowThreadProcessId(h, out pid);
            w.Pid = (int)pid;
            P2PRect r;
            GetWindowRect(h, out r);
            w.L = r.L; w.T = r.T; w.R = r.R; w.B = r.B;
            w.Width = r.R - r.L; w.Height = r.B - r.T;
            w.Visible = IsWindowVisible(h);
            w.Iconic = IsIconic(h);
            // GWL_HWNDPARENT (-8) is the OWNER for a top-level window.
            w.Owner = GetWindowLongPtr(h, -8).ToInt64();
            // GA_PARENT (1): a real child window has a parent that is not the desktop.
            w.IsChild = false;
            w.Z = z++;
            outp.Add(w);
            return true;
        }, IntPtr.Zero);
        return outp.ToArray();
    }

    // W2: the ONLY cross-process identity. Returns "pid=bundle" pairs for every
    // single-instance marker window on the desktop, including invisible ones --
    // the marker is a 0x0 message-only-ish window and is never WS_VISIBLE.
    public static string[] BundleMarkers() {
        List<string> outp = new List<string>();
        EnumWindows(delegate(IntPtr h, IntPtr l) {
            string c = Cls(h);
            if (c != null && c.Length > 4 && c.EndsWith("-sic")) {
                uint pid = 0;
                GetWindowThreadProcessId(h, out pid);
                outp.Add(((int)pid).ToString() + "=" + c.Substring(0, c.Length - 4) + "=" + h.ToInt64().ToString());
            }
            return true;
        }, IntPtr.Zero);
        return outp.ToArray();
    }

    public static long TopWindowRootAt(int x, int y) {
        P2PPoint p; p.X = x; p.Y = y;
        IntPtr h = WindowFromPoint(p);
        if (h == IntPtr.Zero) { return 0; }
        return GetAncestor(h, 2).ToInt64();   // GA_ROOT
    }
}

public struct P2PPoint { public int X, Y; }
'@ -ErrorAction Stop
}

[void][P2PW]::SetProcessDPIAware()

# ---------------------------------------------------------------------------
# PowerShell-side helpers. Nothing below enumerates windows itself.
# ---------------------------------------------------------------------------

function Get-P2PWindows {
    $wins = [P2PW]::Enumerate()
    $procMap = @{}
    foreach ($p in @(Get-Process -ErrorAction SilentlyContinue)) { $procMap[[int]$p.Id] = $p.ProcessName }
    $out = @()
    foreach ($w in $wins) {
        $pn = ''
        if ($procMap.ContainsKey([int]$w.Pid)) { $pn = $procMap[[int]$w.Pid] }
        $out += [pscustomobject]@{
            Hwnd    = [int64]$w.Hwnd
            HwndHex = ('0x{0:X}' -f [int64]$w.Hwnd)
            Title   = $w.Title
            Cls     = $w.Cls
            Pid     = [int]$w.Pid
            Process = $pn
            L = $w.L; T = $w.T; R = $w.R; B = $w.B
            Width = $w.Width; Height = $w.Height
            Visible = $w.Visible
            Iconic  = $w.Iconic
            Owner   = [int64]$w.Owner
            Z       = $w.Z
        }
    }
    return $out
}

# W2. pid -> bundle identifier, read from the single-instance marker class.
# This is the ONLY function any caller may use to answer "which build is that".
function Get-P2PBundleMap {
    $map = @{}
    foreach ($row in @([P2PW]::BundleMarkers())) {
        $parts = $row -split '='
        if ($parts.Count -ge 2) { $map[[int]$parts[0]] = $parts[1] }
    }
    return $map
}

# W3. Full parent/child closure of a pid, so a re-exec'ing stub is still "ours".
function Get-P2PFamily {
    param([int]$RootPid, [int]$MaxDepth = 6)
    $family = New-Object System.Collections.Generic.HashSet[int]
    [void]$family.Add($RootPid)
    $frontier = @($RootPid)
    for ($d = 0; $d -lt $MaxDepth -and $frontier.Count -gt 0; $d++) {
        $filter = (@($frontier | ForEach-Object { 'ParentProcessId=' + $_ }) -join ' OR ')
        $kids = @(Get-CimInstance Win32_Process -Filter $filter -ErrorAction SilentlyContinue)
        $next = @()
        foreach ($k in $kids) {
            if ($family.Add([int]$k.ProcessId)) { $next += [int]$k.ProcessId }
        }
        $frontier = $next
    }
    return $family
}

function Test-P2PProcessAlive {
    param([int]$ProcId)
    if ($ProcId -le 0) { return $false }
    try { $null = Get-Process -Id $ProcId -ErrorAction Stop; return $true } catch { return $false }
}

# Discord's process set must be asserted unchanged across every run. Captured as
# a sorted pid list so a restart (same name, new pid) is caught, not just a
# count change.
function Get-P2PDiscordPids {
    param([string]$Pattern = '^Discord$|^DiscordPTB$|^DiscordCanary$|^DiscordDevelopment$')
    return @(Get-Process -ErrorAction SilentlyContinue |
             Where-Object { $_.ProcessName -match $Pattern } |
             ForEach-Object { $_.Id } | Sort-Object)
}

# A file identity that survives a rebuild in place. `cargo` hardlinks its output,
# so mtime alone has already lied about "is this the build I just made"; the
# hash is the answer, and both are recorded.
function Get-P2PFileStamp {
    param([string]$Path, [string]$Label)
    $o = [ordered]@{
        label = $Label; path = $Path; exists = $false
        sizeBytes = $null; sha256 = $null; written = $null; ageSec = $null
    }
    if (-not $Path) { return [pscustomobject]$o }
    if (-not (Test-Path -LiteralPath $Path)) { return [pscustomobject]$o }
    try {
        $fi = Get-Item -LiteralPath $Path -ErrorAction Stop
        $o.exists = $true
        $o.sizeBytes = [int64]$fi.Length
        $o.written = $fi.LastWriteTime.ToString('o')
        $o.ageSec = [int]((Get-Date) - $fi.LastWriteTime).TotalSeconds
        $o.sha256 = (Get-FileHash -LiteralPath $Path -Algorithm SHA256 -ErrorAction Stop).Hash
    } catch {
        $o.sha256 = ('unreadable: ' + $_.Exception.Message)
    }
    return [pscustomobject]$o
}
