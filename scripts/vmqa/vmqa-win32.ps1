<#
.SYNOPSIS
  VMQA Win32 layer: subject resolution and input actuation, for isolated QA VMs only.

.DESCRIPTION
  This module exists to make ONE class of bug unexpressible.

  Three false greens landed in a single day, and all three were the same defect: a harness that
  guesses its subject confirms whatever it happened to find. Selecting "the first process with a
  window" drove the WRONG lane's application through six UI steps and graded a stale instance.
  Every OSL build is titled "OSL Privacy", so a title match cannot tell two builds apart.

  The countermeasure is structural, not advisory. A subject can only be produced by
  Resolve-VmqaSubject, which matches exclusively on the single-instance marker window class
  "<identifier>-sic". Every actuator takes a [VmqaSubject] as a mandatory typed parameter, and
  [VmqaSubject] cannot be constructed without the run nonce that Start-VmqaRun generated. There is
  no overload accepting a pid, a title, or a process name. A caller who wants to guess has nowhere
  to put the guess.

  INPUT INJECTION. SetCursorPos / mouse_event / SendKeys are used here deliberately. The
  PostMessage-only rule protects the OWNER'S DESKTOP, where synthetic input steals his cursor and
  keystrokes mid-sentence. That rationale does not survive the trip to a disposable VM: nobody is
  sitting at that machine, and a consent flow that must actually be clicked cannot be proven any
  other way. Import-VmqaWin32 refuses to load on a machine that is not an OSL-* QA VM, so this
  module cannot be dot-sourced onto the owner's desktop by accident.

  Enumeration runs inside C#. A PowerShell EnumWindows callback that throws silently truncates the
  list, which would turn "the app is not running" and "the enumerator died halfway" into the same
  observation — the exact conflation this file exists to prevent.
#>

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

# --------------------------------------------------------------------------------------------
# Refuse to load anywhere but a QA VM. This is the whole safety story for the injection APIs
# below, so it is a load-time hard failure with no override switch. A -Force here would be a
# loaded gun pointed at the owner's desktop.
#
# The check is IMDS, NOT the Windows hostname. Two reasons, the first found the hard way:
#   1. This fleet's Windows hostnames are OSLCLIENT1, OSLCLIENT2 and so on. They do NOT match
#      'OSL-*', which needs a literal hyphen, so a hostname guard refused to load on the exact
#      machines it exists to permit. A guard that fails closed on its own fleet is still a bug.
#   2. A hostname is trivially spoofable - anyone can rename a desktop 'OSL-whatever'. The Azure
#      Instance Metadata Service at 169.254.169.254 is link-local and answers only on an Azure
#      VM, so a developer desktop cannot satisfy it at all, and the name it returns is the
#      authoritative Azure resource name rather than something a user typed.
# The allow-list is closed: an Azure VM outside this fleet is refused too.
# --------------------------------------------------------------------------------------------
$script:VmqaAllowedVms = @(
    'OSL-Azure-Client-1', 'OSL-Azure-Client-2',
    'OSL-Independent-Client-1', 'OSL-Independent-Client-2',
    'OSL-WhatsApp-Client-1', 'OSL-WhatsApp-Client-2',
    'OSL-Telegram-QA-1', 'OSL-Telegram-QA-2',
    'OSL-Signal-Client-1', 'OSL-Signal-Client-2'
)

$script:VmqaMachineName = $null
try {
    $script:VmqaMachineName = ([string](Invoke-RestMethod -Method Get -TimeoutSec 5 `
        -Uri 'http://169.254.169.254/metadata/instance/compute/name?api-version=2021-02-01&format=text' `
        -Headers @{ Metadata = 'true' })).Trim()
} catch {
    throw "VMQA_NOT_A_QA_VM: vmqa-win32.ps1 synthesises real mouse and keyboard input and may " +
          "only load on an isolated Azure QA VM. Azure IMDS did not answer, so this is not one " +
          "(hostname '$env:COMPUTERNAME')."
}

if ($script:VmqaAllowedVms -notcontains $script:VmqaMachineName) {
    throw "VMQA_WRONG_MACHINE: IMDS reports this VM is '$($script:VmqaMachineName)', which is not " +
          "in the QA fleet allow-list. Refusing to load an input-synthesis module here."
}

Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing

Add-Type @"
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;

public class VmqaNative {
    public const int DWMWA_EXTENDED_FRAME_BOUNDS = 9;
    public const int DWMWA_CLOAKED = 14;
    public delegate bool EnumProc(IntPtr hWnd, IntPtr lParam);

    [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc cb, IntPtr p);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    public static extern int GetClassName(IntPtr hWnd, StringBuilder buf, int max);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint pid);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT r);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hWnd);
    [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr hWnd);
    [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr hWnd);
    [DllImport("dwmapi.dll")] public static extern int DwmGetWindowAttribute(
        IntPtr hWnd, int attribute, out int value, int valueSize);
    [DllImport("dwmapi.dll", EntryPoint = "DwmGetWindowAttribute")]
    public static extern int DwmGetWindowAttributeRect(
        IntPtr hWnd, int attribute, out RECT value, int valueSize);
    [DllImport("dwmapi.dll")] public static extern int DwmFlush();
    [DllImport("user32.dll")] public static extern bool SetWindowPos(
        IntPtr hWnd, IntPtr insertAfter, int x, int y, int cx, int cy, uint flags);
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [DllImport("kernel32.dll")] public static extern uint GetCurrentThreadId();
    [DllImport("user32.dll")] public static extern bool AttachThreadInput(uint attach, uint attachTo, bool value);
    [DllImport("user32.dll")] public static extern bool BringWindowToTop(IntPtr hWnd);
    [DllImport("user32.dll")] public static extern bool ShowWindowAsync(IntPtr hWnd, int command);
    [DllImport("user32.dll")] public static extern IntPtr WindowFromPoint(POINT p);
    [DllImport("user32.dll")] public static extern IntPtr GetAncestor(IntPtr hWnd, uint flags);
    [DllImport("user32.dll")] public static extern bool GetCursorPos(out POINT p);
    [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
    public const uint GA_ROOT = 2;
    public struct POINT { public int X, Y; }
    [DllImport("user32.dll")] public static extern void mouse_event(uint f, uint x, uint y, uint d, int extra);

    public const uint MOUSEEVENTF_LEFTDOWN = 0x0002;
    public const uint MOUSEEVENTF_LEFTUP   = 0x0004;
    public const uint SWP_NOZORDER = 0x0004;
    public const uint SWP_NOACTIVATE = 0x0010;

    public struct RECT { public int Left, Top, Right, Bottom; }

    public class Marker {
        public IntPtr Hwnd;
        public uint   Pid;
        public string ClassName;
        public bool   Visible;
    }

    public class Surface {
        public IntPtr Hwnd;
        public uint   Pid;
        public string ClassName;
        public RECT   Rect;
        public RECT   RawRect;
        public string BoundsSource;
        public uint   Dpi;
        public bool   NormalizedForCapture;
    }

    // Enumeration lives in C# on purpose: an exception thrown out of a PowerShell callback
    // silently truncates EnumWindows, which would under-report markers and make a broken
    // enumerator look exactly like an absent application.
    public static List<Marker> MarkerWindows() {
        List<Marker> found = new List<Marker>();
        EnumWindows(delegate(IntPtr h, IntPtr l) {
            try {
                StringBuilder sb = new StringBuilder(512);
                int n = GetClassName(h, sb, sb.Capacity);
                if (n > 0) {
                    string cls = sb.ToString();
                    if (cls.EndsWith("-sic", StringComparison.Ordinal)) {
                        uint pid;
                        GetWindowThreadProcessId(h, out pid);
                        Marker m = new Marker();
                        m.Hwnd = h; m.Pid = pid; m.ClassName = cls; m.Visible = IsWindowVisible(h);
                        found.Add(m);
                    }
                }
            } catch { /* one bad window must not truncate the sweep */ }
            return true;
        }, IntPtr.Zero);
        return found;
    }

    private static bool Contains(RECT outer, RECT inner) {
        return inner.Left >= outer.Left && inner.Top >= outer.Top &&
               inner.Right <= outer.Right && inner.Bottom <= outer.Bottom;
    }

    private static string RectText(RECT r) {
        return r.Left + "," + r.Top + " " +
               (r.Right - r.Left) + "x" + (r.Bottom - r.Top);
    }

    public static void ValidateExtendedFrameBounds(RECT raw, RECT dwm, RECT virtualScreen) {
        if (dwm.Right <= dwm.Left || dwm.Bottom <= dwm.Top)
            throw new InvalidOperationException("VMQA_DWM_BOUNDS_EMPTY");
        if ((dwm.Right - dwm.Left) < 200 || (dwm.Bottom - dwm.Top) < 120)
            throw new InvalidOperationException("VMQA_DWM_BOUNDS_TOO_SMALL");
        if (!Contains(raw, dwm))
            throw new InvalidOperationException(
                "VMQA_DWM_BOUNDS_OUTSIDE_GETWINDOWRECT: raw=" + RectText(raw) +
                " dwm=" + RectText(dwm));
        if (!Contains(virtualScreen, dwm))
            throw new InvalidOperationException(
                "VMQA_DWM_BOUNDS_OUTSIDE_VIRTUAL_SCREEN: dwm=" + RectText(dwm) +
                " virtual=" + RectText(virtualScreen));
        if (dwm.Left == virtualScreen.Left && dwm.Top == virtualScreen.Top &&
            dwm.Right == virtualScreen.Right && dwm.Bottom == virtualScreen.Bottom)
            throw new InvalidOperationException("VMQA_DWM_BOUNDS_WHOLE_DESKTOP");
    }

    public static RECT TrustedExtendedFrameBounds(IntPtr h, RECT virtualScreen) {
        RECT raw;
        if (!GetWindowRect(h, out raw))
            throw new InvalidOperationException("VMQA_GETWINDOWRECT_FAILED");
        RECT dwm;
        int hr = DwmGetWindowAttributeRect(
            h, DWMWA_EXTENDED_FRAME_BOUNDS, out dwm, Marshal.SizeOf(typeof(RECT)));
        if (hr != 0)
            throw new InvalidOperationException(
                "VMQA_DWM_BOUNDS_UNTRUSTED: hwnd=" + h +
                " HRESULT=0x" + hr.ToString("x8"));
        ValidateExtendedFrameBounds(raw, dwm, virtualScreen);
        return dwm;
    }

    private static RECT TrustedOccluderBounds(IntPtr h, RECT virtualScreen) {
        RECT raw;
        if (!GetWindowRect(h, out raw))
            throw new InvalidOperationException("VMQA_OCCLUDER_GETWINDOWRECT_FAILED");
        RECT dwm;
        int hr = DwmGetWindowAttributeRect(
            h, DWMWA_EXTENDED_FRAME_BOUNDS, out dwm, Marshal.SizeOf(typeof(RECT)));
        if (hr != 0)
            throw new InvalidOperationException(
                "VMQA_OCCLUDER_DWM_BOUNDS_UNTRUSTED: hwnd=" + h +
                " HRESULT=0x" + hr.ToString("x8"));
        if (dwm.Right <= dwm.Left || dwm.Bottom <= dwm.Top)
            throw new InvalidOperationException("VMQA_OCCLUDER_DWM_BOUNDS_EMPTY");
        if (!Contains(raw, dwm))
            throw new InvalidOperationException("VMQA_OCCLUDER_DWM_BOUNDS_OUTSIDE_GETWINDOWRECT");
        if (!Contains(virtualScreen, dwm))
            throw new InvalidOperationException("VMQA_OCCLUDER_DWM_BOUNDS_OUTSIDE_VIRTUAL_SCREEN");
        return dwm;
    }

    public static Surface BuildTrustedSurface(
        IntPtr h, uint pid, string className, RECT raw, RECT dwm, RECT virtualScreen, uint dpi) {
        ValidateExtendedFrameBounds(raw, dwm, virtualScreen);
        if (dpi != 96)
            throw new InvalidOperationException("VMQA_DWM_DPI_UNTRUSTED: dpi=" + dpi);
        Surface s = new Surface();
        s.Hwnd = h; s.Pid = pid; s.ClassName = className;
        s.Rect = dwm; s.RawRect = raw;
        s.BoundsSource = "dwm-extended-frame"; s.Dpi = dpi;
        return s;
    }

    public static List<Surface> VisibleTopLevelWindowsForPid(
        uint wantedPid, string markerClass, RECT virtualScreen) {
        List<Surface> found = new List<Surface>();
        bool failed = false;
        string failureMessage = "";
        bool completed = EnumWindows(delegate(IntPtr h, IntPtr l) {
            try {
                uint pid;
                if (GetWindowThreadProcessId(h, out pid) == 0) throw new InvalidOperationException("GetWindowThreadProcessId");
                if (pid != wantedPid || !IsWindowVisible(h) || IsIconic(h)) return true;
                StringBuilder sb = new StringBuilder(512);
                if (GetClassName(h, sb, sb.Capacity) <= 0)
                    throw new InvalidOperationException("GetClassName");
                string className = sb.ToString();
                // The marker is identity, never pixels. Some marker HWNDs do not publish DWM
                // state at all; skip it before either DWM query.
                if (String.Equals(className, markerClass, StringComparison.Ordinal)) return true;
                int cloaked;
                if (DwmGetWindowAttribute(h, DWMWA_CLOAKED, out cloaked, sizeof(int)) != 0) throw new InvalidOperationException("DwmGetWindowAttribute");
                if (cloaked != 0) return true;
                RECT raw;
                if (!GetWindowRect(h, out raw)) throw new InvalidOperationException("GetWindowRect");
                // Auxiliary Tauri/Chromium top-level HWNDs may be visible but tiny. They are not
                // capture surfaces and must be excluded before applying the selected-surface
                // minimum-size validator, or one helper window poisons the whole enumeration.
                if ((raw.Right - raw.Left) < 200 || (raw.Bottom - raw.Top) < 120) return true;
                RECT dwm = TrustedExtendedFrameBounds(h, virtualScreen);
                uint dpi = GetDpiForWindow(h);
                found.Add(BuildTrustedSurface(h, pid, className, raw, dwm, virtualScreen, dpi));
            } catch (Exception ex) { failed = true; failureMessage = ex.Message; return false; }
            return true;
        }, IntPtr.Zero);
        if (!completed || failed)
            throw new InvalidOperationException(
                "VMQA_SURFACE_ENUMERATION_FAILED: " + failureMessage);
        return found;
    }

    public static bool NormalizeVisibleTopLevelWindowForPid(
        uint wantedPid, string markerClass, RECT virtualScreen) {
        List<Surface> found = new List<Surface>();
        string failureMessage = "";
        bool failed = false;
        bool completed = EnumWindows(delegate(IntPtr h, IntPtr l) {
            try {
                uint pid;
                if (GetWindowThreadProcessId(h, out pid) == 0)
                    throw new InvalidOperationException("GetWindowThreadProcessId");
                if (pid != wantedPid || !IsWindowVisible(h) || IsIconic(h)) return true;
                StringBuilder sb = new StringBuilder(512);
                if (GetClassName(h, sb, sb.Capacity) <= 0)
                    throw new InvalidOperationException("GetClassName");
                if (String.Equals(sb.ToString(), markerClass, StringComparison.Ordinal)) return true;
                int cloaked;
                if (DwmGetWindowAttribute(h, DWMWA_CLOAKED, out cloaked, sizeof(int)) != 0)
                    throw new InvalidOperationException("DwmGetWindowAttribute");
                if (cloaked != 0) return true;
                RECT raw;
                if (!GetWindowRect(h, out raw))
                    throw new InvalidOperationException("GetWindowRect");
                if ((raw.Right - raw.Left) < 200 || (raw.Bottom - raw.Top) < 120) return true;
                Surface candidate = new Surface();
                candidate.Hwnd = h;
                candidate.Pid = pid;
                candidate.ClassName = sb.ToString();
                candidate.RawRect = raw;
                found.Add(candidate);
            } catch (Exception ex) { failed = true; failureMessage = ex.Message; return false; }
            return true;
        }, IntPtr.Zero);
        if (!completed || failed)
            throw new InvalidOperationException(
                "VMQA_SURFACE_PREP_ENUMERATION_FAILED: " + failureMessage);
        if (found.Count != 1) {
            StringBuilder details = new StringBuilder();
            foreach (Surface candidate in found) {
                if (details.Length > 0) details.Append("; ");
                details.Append("hwnd=").Append(candidate.Hwnd)
                    .Append(" class=").Append(candidate.ClassName)
                    .Append(" raw=").Append(RectText(candidate.RawRect));
            }
            throw new InvalidOperationException(
                "VMQA_SURFACE_PREP_AMBIGUOUS: candidateCount=" + found.Count +
                " candidates=[" + details.ToString() + "]");
        }

        RECT selectedRaw = found[0].RawRect;
        if (Contains(virtualScreen, selectedRaw)) return false;

        int virtualWidth = virtualScreen.Right - virtualScreen.Left;
        int virtualHeight = virtualScreen.Bottom - virtualScreen.Top;
        if (virtualWidth < 264 || virtualHeight < 184)
            throw new InvalidOperationException(
                "VMQA_SURFACE_PREP_SCREEN_TOO_SMALL: virtual=" + RectText(virtualScreen));
        int width = Math.Min(960, virtualWidth - 64);
        int height = Math.Min(700, virtualHeight - 64);
        int x = virtualScreen.Left + (virtualWidth - width) / 2;
        int y = virtualScreen.Top + (virtualHeight - height) / 2;
        if (!SetWindowPos(
                found[0].Hwnd, IntPtr.Zero, x, y, width, height,
                SWP_NOZORDER | SWP_NOACTIVATE))
            throw new InvalidOperationException("VMQA_SURFACE_PREP_SETWINDOWPOS_FAILED");
        int hr = DwmFlush();
        if (hr != 0)
            throw new InvalidOperationException(
                "VMQA_SURFACE_PREP_DWMFLUSH_FAILED: HRESULT=0x" + hr.ToString("x8"));
        return true;
    }

    private static bool RectanglesIntersect(RECT a, RECT b) {
        return a.Left < b.Right && a.Right > b.Left && a.Top < b.Bottom && a.Bottom > b.Top;
    }

    public static bool HasOccludingWindowAbove(
        IntPtr target, RECT targetRect, RECT virtualScreen) {
        bool targetSeen = false;
        bool occluderSeen = false;
        bool failed = false;
        string failureMessage = "";
        EnumWindows(delegate(IntPtr h, IntPtr l) {
            if (h == target) { targetSeen = true; return false; }
            try {
                if (!IsWindowVisible(h) || IsIconic(h)) return true;
                StringBuilder sb = new StringBuilder(512);
                if (GetClassName(h, sb, sb.Capacity) <= 0)
                    throw new InvalidOperationException("GetClassName");
                if (sb.ToString().EndsWith("-sic", StringComparison.Ordinal)) return true;
                int cloaked;
                if (DwmGetWindowAttribute(h, DWMWA_CLOAKED, out cloaked, sizeof(int)) != 0) throw new InvalidOperationException("DwmGetWindowAttribute");
                if (cloaked != 0) return true;
                // Occluders may legitimately be small (tooltips/taskbar) or cover the entire
                // virtual screen. They still need trustworthy DWM geometry, but the selected
                // OSL surface's minimum-size and whole-desktop rules do not apply to them.
                RECT rect = TrustedOccluderBounds(h, virtualScreen);
                if (RectanglesIntersect(rect, targetRect)) { occluderSeen = true; return false; }
            } catch (Exception ex) { failed = true; failureMessage = ex.Message; return false; }
            return true;
        }, IntPtr.Zero);
        if (failed)
            throw new InvalidOperationException(
                "VMQA_OCCLUSION_ENUMERATION_FAILED: " + failureMessage);
        if (occluderSeen) return true;
        if (!targetSeen) throw new InvalidOperationException("VMQA_TARGET_NOT_IN_Z_ORDER");
        return false;
    }
}
"@

# --------------------------------------------------------------------------------------------
# The subject type. Constructing one requires the current run nonce, and the nonce is generated
# inside Start-VmqaRun and never written to disk or to a verdict. A caller cannot fabricate a
# subject around a pid they guessed, because they cannot produce the nonce.
# --------------------------------------------------------------------------------------------
class VmqaSubject {
    [int]    $Pid
    [IntPtr] $Hwnd
    [string] $Identifier
    [string] $ExePath
    [string] $ExeSha256
    [string] $Nonce
    [int]    $MarkerWindowsTotal
    [datetime] $ResolvedUtc

    VmqaSubject([string]$nonce) {
        if ([string]::IsNullOrWhiteSpace($nonce)) { throw 'VMQA_BAD_NONCE: subject requires a run nonce' }
        if ($nonce -cne $script:VmqaRunNonce) {
            throw 'VMQA_BAD_NONCE: a subject may only be built by Resolve-VmqaSubject during the current run'
        }
        $this.Nonce = $nonce
        $this.ResolvedUtc = [datetime]::UtcNow
    }
}

$script:VmqaRunNonce = $null

function Start-VmqaRun {
    <# Opens a run scope. The nonce it mints is what binds every subject to THIS run, so a subject
       object left over from a previous run cannot actuate anything in the next one. #>
    [CmdletBinding()]
    param()
    $script:VmqaRunNonce = [guid]::NewGuid().ToString('n')
    return $script:VmqaRunNonce
}

function Get-VmqaMarkerWindows {
    <# Every marker window on the desktop, across ALL identifiers. The total is the apparatus
       check: it is what distinguishes "this identifier is not running" from "the enumerator is
       broken". Callers must never use this to pick a target. #>
    [CmdletBinding()]
    param()
    return @([VmqaNative]::MarkerWindows())
}

function Resolve-VmqaSubject {
    <# The ONLY way to obtain an actuatable target.

       Matches exclusively on window class "<Identifier>-sic". Not on title, not on process name,
       not on "first process with a window" — every OSL build is titled "OSL Privacy" and that has
       already graded a stale instance.

       Throws VMQA_NO_SUBJECT (zero markers for this identifier) or VMQA_AMBIGUOUS_SUBJECT (more
       than one). Both carry MarkerWindowsTotal in .Data so the caller can tell a real absence from
       a dead enumerator. There is deliberately no "pick the newest" tie-break: two live instances
       of one identifier means the rig is wrong, and quietly choosing one is how a stale instance
       gets graded. #>
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][ValidateNotNullOrEmpty()][string]$Identifier,
        [Parameter(Mandatory)][ValidateNotNullOrEmpty()][string]$RunNonce
    )

    # Re-wrap in @(). A PowerShell function that returns an EMPTY array hands back $null, and
    # under StrictMode $null.Count throws — so the zero-marker case, which is precisely the
    # "apparatus not proven" signal this resolver exists to report, crashed instead of reporting
    # itself. The crash then surfaced as 'blocked' through the generic catch, silently converting
    # "I could not measure" into "the harness positively denied it". Those are different facts and
    # collapsing them is the exact defect this file was written to prevent.
    $all   = @(Get-VmqaMarkerWindows)
    $total = $all.Count
    $want  = "$Identifier-sic"
    $mine  = @($all | Where-Object { $_.ClassName -ceq $want })

    if ($mine.Count -eq 0) {
        $e = [System.Management.Automation.RuntimeException]::new(
            "VMQA_NO_SUBJECT: no marker window of class '$want' (markers on desktop: $total)")
        $e.Data['MarkerWindowsTotal'] = $total
        throw $e
    }
    if ($mine.Count -gt 1) {
        $e = [System.Management.Automation.RuntimeException]::new(
            "VMQA_AMBIGUOUS_SUBJECT: $($mine.Count) marker windows of class '$want'; the rig is wrong")
        $e.Data['MarkerWindowsTotal'] = $total
        throw $e
    }

    $m    = $mine[0]
    $proc = Get-Process -Id ([int]$m.Pid) -ErrorAction Stop
    $path = $null
    try { $path = $proc.Path } catch { $path = $null }

    $s = [VmqaSubject]::new($RunNonce)
    $s.Pid                = [int]$m.Pid
    $s.Hwnd               = $m.Hwnd
    $s.Identifier         = $Identifier
    $s.ExePath            = $path
    $s.MarkerWindowsTotal = $total
    # Identify the binary by content. cargo hardlinks its output, so mtime lies about whether a
    # rebuild actually shipped; a matching identifier on a stale exe is still the wrong subject.
    $s.ExeSha256 = if ($path -and (Test-Path -LiteralPath $path -PathType Leaf)) {
        (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant()
    } else { $null }
    return $s
}

function Assert-VmqaSubject {
    <# Re-validates at the point of use. Guards against a subject captured before a relaunch, and
       against anything not produced by the resolver. #>
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][VmqaSubject]$Subject,
        [Parameter(Mandatory)][ValidateNotNullOrEmpty()][string]$RunNonce
    )
    if ($Subject.Nonce -cne $RunNonce) { throw 'VMQA_STALE_SUBJECT: subject belongs to a different run' }
    if ($Subject.Nonce -cne $script:VmqaRunNonce) { throw 'VMQA_STALE_SUBJECT: run scope has moved on' }
    $live = Get-Process -Id $Subject.Pid -ErrorAction SilentlyContinue
    if (-not $live) { throw "VMQA_SUBJECT_GONE: pid $($Subject.Pid) exited" }
    # The pid must still be the same program. Windows recycles pids, and a recycled pid pointing at
    # some other binary is precisely the "something else worked" false green.
    $nowPath = $null
    try { $nowPath = $live.Path } catch { $nowPath = $null }
    if ($Subject.ExePath -and $nowPath -and ($nowPath -cne $Subject.ExePath)) {
        throw "VMQA_SUBJECT_RECYCLED: pid $($Subject.Pid) is now '$nowPath'"
    }
    return $true
}

function Get-VmqaWindowRect {
    [CmdletBinding()]
    param([Parameter(Mandatory)][VmqaSubject]$Subject)
    $r = New-Object VmqaNative+RECT
    if (-not [VmqaNative]::GetWindowRect($Subject.Hwnd, [ref]$r)) { throw 'VMQA_NO_RECT' }
    return [pscustomobject]@{
        Left = $r.Left; Top = $r.Top; Right = $r.Right; Bottom = $r.Bottom
        Width = $r.Right - $r.Left; Height = $r.Bottom - $r.Top
    }
}

function Get-VmqaVirtualScreenRect {
    [CmdletBinding()]
    param()
    $bounds = [System.Windows.Forms.SystemInformation]::VirtualScreen
    $rect = New-Object VmqaNative+RECT
    $rect.Left = $bounds.Left
    $rect.Top = $bounds.Top
    $rect.Right = $bounds.Right
    $rect.Bottom = $bounds.Bottom
    return $rect
}

function Get-VmqaCaptureWorkAreaRect {
    [CmdletBinding()]
    param()
    Add-Type -AssemblyName System.Windows.Forms
    $bounds = [System.Windows.Forms.Screen]::PrimaryScreen.WorkingArea
    $rect = New-Object VmqaNative+RECT
    $rect.Left = $bounds.Left
    $rect.Top = $bounds.Top
    $rect.Right = $bounds.Right
    $rect.Bottom = $bounds.Bottom
    return $rect
}

function Assert-VmqaTrustedSurfaceBounds {
    [CmdletBinding()]
    param([Parameter(Mandatory)]$Surface)
    if ($Surface.BoundsSource -cne 'dwm-extended-frame') {
        throw "VMQA_UNTRUSTED_BOUNDS_SOURCE: expected=dwm-extended-frame actual=$($Surface.BoundsSource)"
    }
    if ([uint32]$Surface.Dpi -ne 96) {
        throw "VMQA_DWM_DPI_UNTRUSTED: dpi=$($Surface.Dpi)"
    }
    $virtual = Get-VmqaVirtualScreenRect
    [VmqaNative]::ValidateExtendedFrameBounds($Surface.RawRect, $Surface.Rect, $virtual)
    return $true
}

function Get-VmqaVisibleSurface {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][VmqaSubject]$Subject,
        [Parameter(Mandatory)][string]$RunNonce
    )
    [void](Assert-VmqaSubject -Subject $Subject -RunNonce $RunNonce)
    $markerClass = "$($Subject.Identifier)-sic"
    $virtual = Get-VmqaVirtualScreenRect
    $workArea = Get-VmqaCaptureWorkAreaRect
    # Some VM display profiles are smaller than OSL's requested startup size. Reposition and
    # resize only the unique non-marker HWND owned by the exact launched PID, without activating
    # it or synthesizing input. Capture still re-queries DWM afterward and fails closed on any
    # untrusted, off-screen, or whole-desktop extended-frame result.
    $normalized = [VmqaNative]::NormalizeVisibleTopLevelWindowForPid(
        [uint32]$Subject.Pid, $markerClass, $workArea)
    $candidates = @(
        [VmqaNative]::VisibleTopLevelWindowsForPid(
            [uint32]$Subject.Pid, $markerClass, $virtual) |
            Where-Object {
                $_.ClassName -cne $markerClass -and
                ($_.Rect.Right - $_.Rect.Left) -ge 200 -and
                ($_.Rect.Bottom - $_.Rect.Top) -ge 120
            }
    )
    if ($candidates.Count -eq 0) {
        throw "VMQA_NO_VISIBLE_SURFACE: pid $($Subject.Pid) has no non-marker visible top-level window at least 200x120"
    }
    if ($candidates.Count -gt 1) {
        throw "VMQA_AMBIGUOUS_VISIBLE_SURFACE: pid $($Subject.Pid) has $($candidates.Count) visible top-level windows at least 200x120"
    }
    $candidates[0].NormalizedForCapture = $normalized
    [void](Assert-VmqaTrustedSurfaceBounds -Surface $candidates[0])
    return $candidates[0]
}

function Set-VmqaSurfaceForeground {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]$Surface,
        [int]$Attempts = 3
    )
    for ($i = 1; $i -le $Attempts; $i++) {
        [void][VmqaNative]::SetForegroundWindow($Surface.Hwnd)
        Start-Sleep -Milliseconds 250
        if ([VmqaNative]::GetForegroundWindow() -eq $Surface.Hwnd) { return $true }

        # The agent is in the interactive VM session but is not necessarily the foreground
        # thread. Attach only for the duration of this transfer, then detach in reverse order.
        # This changes no owner-desktop state: vmqa-win32 refuses to load off the closed Azure
        # fleet, and the target HWND was resolved from the exact launched PID.
        [uint32]$surfacePid = 0
        $surfaceThread = [VmqaNative]::GetWindowThreadProcessId($Surface.Hwnd, [ref]$surfacePid)
        $foregroundHwnd = [VmqaNative]::GetForegroundWindow()
        [uint32]$foregroundPid = 0
        $foregroundThread = if ($foregroundHwnd -ne [IntPtr]::Zero) {
            [VmqaNative]::GetWindowThreadProcessId($foregroundHwnd, [ref]$foregroundPid)
        } else { 0 }
        $currentThread = [VmqaNative]::GetCurrentThreadId()
        $attachedForeground = $false
        $attachedSurface = $false
        try {
            if ($foregroundThread -ne 0 -and $foregroundThread -ne $currentThread) {
                $attachedForeground = [VmqaNative]::AttachThreadInput($currentThread, $foregroundThread, $true)
            }
            if ($surfaceThread -ne 0 -and $surfaceThread -ne $currentThread) {
                $attachedSurface = [VmqaNative]::AttachThreadInput($currentThread, $surfaceThread, $true)
            }
            [void][VmqaNative]::ShowWindowAsync($Surface.Hwnd, 9)
            [void][VmqaNative]::BringWindowToTop($Surface.Hwnd)
            [void][VmqaNative]::SetForegroundWindow($Surface.Hwnd)
            Start-Sleep -Milliseconds 250
            if ([VmqaNative]::GetForegroundWindow() -eq $Surface.Hwnd) { return $true }
        } finally {
            if ($attachedSurface) {
                [void][VmqaNative]::AttachThreadInput($currentThread, $surfaceThread, $false)
            }
            if ($attachedForeground) {
                [void][VmqaNative]::AttachThreadInput($currentThread, $foregroundThread, $false)
            }
        }
    }
    return $false
}

function Test-VmqaSurfaceStillBound {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][VmqaSubject]$Subject,
        [Parameter(Mandatory)]$Surface,
        [Parameter(Mandatory)][string]$RunNonce
    )
    try {
        [void](Assert-VmqaSubject -Subject $Subject -RunNonce $RunNonce)
        [uint32]$surfacePid = 0
        [void][VmqaNative]::GetWindowThreadProcessId($Surface.Hwnd, [ref]$surfacePid)
        if ($surfacePid -ne [uint32]$Subject.Pid) { return $false }
        if (-not [VmqaNative]::IsWindowVisible($Surface.Hwnd) -or [VmqaNative]::IsIconic($Surface.Hwnd)) {
            return $false
        }
        [int]$cloaked = 0
        if ([VmqaNative]::DwmGetWindowAttribute(
                $Surface.Hwnd, [VmqaNative]::DWMWA_CLOAKED, [ref]$cloaked, 4) -ne 0 -or
            $cloaked -ne 0) {
            return $false
        }
        [void](Assert-VmqaTrustedSurfaceBounds -Surface $Surface)
        $now = [VmqaNative]::TrustedExtendedFrameBounds(
            $Surface.Hwnd, (Get-VmqaVirtualScreenRect))
        return $now.Left -eq $Surface.Rect.Left -and $now.Top -eq $Surface.Rect.Top -and
            $now.Right -eq $Surface.Rect.Right -and $now.Bottom -eq $Surface.Rect.Bottom
    } catch {
        return $false
    }
}

function Test-VmqaSurfaceOwnsSampleGrid {
    [CmdletBinding()]
    param([Parameter(Mandatory)]$Surface)
    $width = [int]($Surface.Rect.Right - $Surface.Rect.Left)
    $height = [int]($Surface.Rect.Bottom - $Surface.Rect.Top)
    foreach ($xFraction in @(0.1, 0.5, 0.9)) {
        foreach ($yFraction in @(0.1, 0.5, 0.9)) {
            $point = New-Object VmqaNative+POINT
            $point.X = [int]($Surface.Rect.Left + ($width * $xFraction))
            $point.Y = [int]($Surface.Rect.Top + ($height * $yFraction))
            $under = [VmqaNative]::WindowFromPoint($point)
            if ($under -eq [IntPtr]::Zero) { return $false }
            if ([VmqaNative]::GetAncestor($under, [VmqaNative]::GA_ROOT) -ne $Surface.Hwnd) {
                return $false
            }
        }
    }
    return $true
}

function Test-VmqaSurfaceUnoccluded {
    [CmdletBinding()]
    param([Parameter(Mandatory)]$Surface)
    try {
        [void](Assert-VmqaTrustedSurfaceBounds -Surface $Surface)
        return -not [VmqaNative]::HasOccludingWindowAbove(
            $Surface.Hwnd, $Surface.Rect, (Get-VmqaVirtualScreenRect))
    } catch {
        return $false
    }
}

function Set-VmqaForeground {
    <# Returns whether the subject ACTUALLY owns the foreground, measured rather than requested.

       SetForegroundWindow's return value was previously discarded. Windows refuses the foreground
       transition under a foreground lock, and a covering window can take it back — in both cases
       no exception is raised, so the caller typed into whatever happened to be focused and the
       step still reported pass. That is not a hypothetical: OSL's worst known defect is keyboard
       focus never reaching the composer, so the keystrokes land in Discord and send PLAINTEXT.
       A harness that assumes focus cannot detect the one bug it most needs to detect. #>
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][VmqaSubject]$Subject,
        [int]$Attempts = 3
    )
    for ($i = 1; $i -le $Attempts; $i++) {
        [void][VmqaNative]::SetForegroundWindow($Subject.Hwnd)
        Start-Sleep -Milliseconds 250
        if ([VmqaNative]::GetForegroundWindow() -eq $Subject.Hwnd) { return $true }
    }
    return $false
}

function Invoke-VmqaClick {
    <# Real pointer input, window-relative. Legal here and nowhere else: isolated VM, no operator
       at the keyboard. Coordinates are validated against the subject's own rect so a bad offset
       cannot land a click on some other window. #>
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][VmqaSubject]$Subject,
        [Parameter(Mandatory)][int]$WinX,
        [Parameter(Mandatory)][int]$WinY,
        [int]$SettleMs = 600,
        [Parameter(Mandatory)][string]$RunNonce
    )
    [void](Assert-VmqaSubject -Subject $Subject -RunNonce $RunNonce)
    $surface = Get-VmqaVisibleSurface -Subject $Subject -RunNonce $RunNonce
    if (-not (Test-VmqaSurfaceStillBound -Subject $Subject -Surface $surface -RunNonce $RunNonce)) {
        throw 'VMQA_CLICK_NOT_DELIVERABLE: visible surface changed before placement'
    }
    $rect = [pscustomobject]@{
        Left = $surface.Rect.Left; Top = $surface.Rect.Top
        Width = $surface.Rect.Right - $surface.Rect.Left
        Height = $surface.Rect.Bottom - $surface.Rect.Top
    }
    if ($WinX -lt 0 -or $WinY -lt 0 -or $WinX -ge $rect.Width -or $WinY -ge $rect.Height) {
        throw "VMQA_CLICK_OUT_OF_BOUNDS: ($WinX,$WinY) outside $($rect.Width)x$($rect.Height)"
    }
    $foreground = Set-VmqaSurfaceForeground -Surface $surface
    $sx = $rect.Left + $WinX
    $sy = $rect.Top + $WinY

    # Placement is measured, not requested. SetCursorPos can fail or be overridden, and a click
    # delivered somewhere other than the intended point is indistinguishable from a working one
    # unless the position is read back.
    $moved = [VmqaNative]::SetCursorPos($sx, $sy)
    Start-Sleep -Milliseconds 120
    $actual = New-Object VmqaNative+POINT
    [void][VmqaNative]::GetCursorPos([ref]$actual)
    $atTarget = ($actual.X -eq $sx -and $actual.Y -eq $sy)

    # Whose window is actually under the cursor? WindowFromPoint -> GA_ROOT is the only way to know
    # the pixel about to be clicked belongs to the subject rather than to something covering it.
    $under = [VmqaNative]::WindowFromPoint($actual)
    $underRoot = if ($under -ne [IntPtr]::Zero) { [VmqaNative]::GetAncestor($under, [VmqaNative]::GA_ROOT) } else { [IntPtr]::Zero }
    $ownsPixel = ($underRoot -eq $surface.Hwnd)

    if (-not ($foreground -and $atTarget -and $ownsPixel)) {
        # Refuse rather than click blind. A click delivered to a covering window is an action against
        # a program we did not choose, and reporting it as a pass is the "something else worked"
        # false green in its most literal form.
        throw ("VMQA_CLICK_NOT_DELIVERABLE: foreground=$foreground cursorAtTarget=$atTarget " +
               "pixelOwnedBySubject=$ownsPixel (setCursorPos=$moved, cursor=$($actual.X),$($actual.Y) target=$sx,$sy)")
    }

    [VmqaNative]::mouse_event([VmqaNative]::MOUSEEVENTF_LEFTDOWN, 0, 0, 0, 0)
    Start-Sleep -Milliseconds 60
    [VmqaNative]::mouse_event([VmqaNative]::MOUSEEVENTF_LEFTUP, 0, 0, 0, 0)
    Start-Sleep -Milliseconds $SettleMs
    return [pscustomobject]@{
        ScreenX = $sx; ScreenY = $sy; Rect = $rect
        Foreground = $foreground; CursorAtTarget = $atTarget; PixelOwnedBySubject = $ownsPixel
    }
}

function Invoke-VmqaType {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][VmqaSubject]$Subject,
        [Parameter(Mandatory)][AllowEmptyString()][string]$Text,
        [int]$SettleMs = 400,
        [Parameter(Mandatory)][string]$RunNonce
    )
    [void](Assert-VmqaSubject -Subject $Subject -RunNonce $RunNonce)
    $surface = Get-VmqaVisibleSurface -Subject $Subject -RunNonce $RunNonce
    if (-not (Test-VmqaSurfaceStillBound -Subject $Subject -Surface $surface -RunNonce $RunNonce)) {
        throw 'VMQA_INPUT_NOT_DELIVERABLE: visible surface changed before typing'
    }
    # REFUSE to type into a window we have not confirmed owns the foreground. SendKeys is global:
    # it goes wherever focus actually is. Typing blind is precisely how OSL's known defect sends
    # PLAINTEXT into Discord when focus never reached the composer, and a harness that does the
    # same thing cannot detect it. Never send first and check afterwards - by then it is delivered.
    if (-not (Set-VmqaSurfaceForeground -Surface $surface)) {
        throw "VMQA_INPUT_NOT_DELIVERABLE: subject does not own the foreground; refusing to send keystrokes that would land in another window"
    }
    [System.Windows.Forms.SendKeys]::SendWait($Text)
    Start-Sleep -Milliseconds $SettleMs
    # Focus can be stolen mid-send, so confirm the subject still owns it afterwards too.
    if ([VmqaNative]::GetForegroundWindow() -ne $surface.Hwnd -or
        -not (Test-VmqaSurfaceStillBound -Subject $Subject -Surface $surface -RunNonce $RunNonce)) {
        throw "VMQA_INPUT_DELIVERY_UNCERTAIN: foreground changed during send; part of the input may have gone elsewhere"
    }
}

function Invoke-VmqaKey {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][VmqaSubject]$Subject,
        [Parameter(Mandatory)][ValidateNotNullOrEmpty()][string]$Key,
        [int]$SettleMs = 400,
        [Parameter(Mandatory)][string]$RunNonce
    )
    [void](Assert-VmqaSubject -Subject $Subject -RunNonce $RunNonce)
    $surface = Get-VmqaVisibleSurface -Subject $Subject -RunNonce $RunNonce
    if (-not (Test-VmqaSurfaceStillBound -Subject $Subject -Surface $surface -RunNonce $RunNonce)) {
        throw "VMQA_INPUT_NOT_DELIVERABLE: visible surface changed before key '$Key'"
    }
    # Same rule as Invoke-VmqaType: a key chord is global input and must not be sent to a window
    # nobody has confirmed. A single unverified Enter is enough to commit something irreversible.
    if (-not (Set-VmqaSurfaceForeground -Surface $surface)) {
        throw "VMQA_INPUT_NOT_DELIVERABLE: subject does not own the foreground; refusing to send key '$Key'"
    }
    [System.Windows.Forms.SendKeys]::SendWait($Key)
    Start-Sleep -Milliseconds $SettleMs
    if ([VmqaNative]::GetForegroundWindow() -ne $surface.Hwnd -or
        -not (Test-VmqaSurfaceStillBound -Subject $Subject -Surface $surface -RunNonce $RunNonce)) {
        throw "VMQA_INPUT_DELIVERY_UNCERTAIN: foreground changed during send of key '$Key'"
    }
}

function Stop-VmqaSubject {
    <# Stops only the resolved subject. Never a process the resolver did not hand back. #>
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][VmqaSubject]$Subject,
        [Parameter(Mandatory)][string]$RunNonce
    )
    [void](Assert-VmqaSubject -Subject $Subject -RunNonce $RunNonce)
    Stop-Process -Id $Subject.Pid -Force
}

function Test-VmqaArtifactFresh {
    <# Freshness gate. An artifact predating run start is 'unmeasurable' with its age stated:
       never pass, never fail. A stale receipt that survived a restart will otherwise happily
       impersonate the current run. No fixed staleness window — a fixed window is what once let a
       4158-second-old receipt be reported as a live failure. #>
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string]$Path,
        [Parameter(Mandatory)][datetime]$RunStartUtc
    )
    if (-not (Test-Path -LiteralPath $Path)) {
        return [pscustomobject]@{ Fresh = $false; Reason = 'absent'; AgeSeconds = $null }
    }
    $w = (Get-Item -LiteralPath $Path).LastWriteTimeUtc
    if ($w -lt $RunStartUtc) {
        return [pscustomobject]@{
            Fresh = $false; Reason = 'predates-run-start'
            AgeSeconds = [int]($RunStartUtc - $w).TotalSeconds
        }
    }
    return [pscustomobject]@{ Fresh = $true; Reason = 'ok'; AgeSeconds = 0 }
}
