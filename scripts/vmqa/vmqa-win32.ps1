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
    public delegate bool EnumProc(IntPtr hWnd, IntPtr lParam);

    [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc cb, IntPtr p);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    public static extern int GetClassName(IntPtr hWnd, StringBuilder buf, int max);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint pid);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT r);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hWnd);
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
    [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
    [DllImport("user32.dll")] public static extern void mouse_event(uint f, uint x, uint y, uint d, int extra);

    public const uint MOUSEEVENTF_LEFTDOWN = 0x0002;
    public const uint MOUSEEVENTF_LEFTUP   = 0x0004;

    public struct RECT { public int Left, Top, Right, Bottom; }

    public class Marker {
        public IntPtr Hwnd;
        public uint   Pid;
        public string ClassName;
        public bool   Visible;
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

    $all   = Get-VmqaMarkerWindows
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

function Set-VmqaForeground {
    [CmdletBinding()]
    param([Parameter(Mandatory)][VmqaSubject]$Subject)
    [void][VmqaNative]::SetForegroundWindow($Subject.Hwnd)
    Start-Sleep -Milliseconds 250
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
    $rect = Get-VmqaWindowRect -Subject $Subject
    if ($WinX -lt 0 -or $WinY -lt 0 -or $WinX -ge $rect.Width -or $WinY -ge $rect.Height) {
        throw "VMQA_CLICK_OUT_OF_BOUNDS: ($WinX,$WinY) outside $($rect.Width)x$($rect.Height)"
    }
    Set-VmqaForeground -Subject $Subject
    $sx = $rect.Left + $WinX
    $sy = $rect.Top + $WinY
    [void][VmqaNative]::SetCursorPos($sx, $sy)
    Start-Sleep -Milliseconds 120
    [VmqaNative]::mouse_event([VmqaNative]::MOUSEEVENTF_LEFTDOWN, 0, 0, 0, 0)
    Start-Sleep -Milliseconds 60
    [VmqaNative]::mouse_event([VmqaNative]::MOUSEEVENTF_LEFTUP, 0, 0, 0, 0)
    Start-Sleep -Milliseconds $SettleMs
    return [pscustomobject]@{ ScreenX = $sx; ScreenY = $sy; Rect = $rect }
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
    Set-VmqaForeground -Subject $Subject
    [System.Windows.Forms.SendKeys]::SendWait($Text)
    Start-Sleep -Milliseconds $SettleMs
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
    Set-VmqaForeground -Subject $Subject
    [System.Windows.Forms.SendKeys]::SendWait($Key)
    Start-Sleep -Milliseconds $SettleMs
}

function Get-VmqaScreenshot {
    <# Full-desktop composited capture. CopyFromScreen works because the agent runs in a real
       interactive session; PrintWindow returns black on Chromium surfaces, which is why the
       window-only path is not the default.

       DistinctColors is the evidence the capture is real. A black or failed frame collapses to a
       handful of colours, so a screenshot step that cannot clear the floor is 'unmeasurable'
       rather than a pass — an all-black PNG must never be filed as proof that something rendered. #>
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string]$OutPath,
        [int]$SampleStride = 4
    )
    $b = [System.Windows.Forms.SystemInformation]::VirtualScreen
    $bmp = New-Object System.Drawing.Bitmap($b.Width, $b.Height)
    try {
        $g = [System.Drawing.Graphics]::FromImage($bmp)
        try { $g.CopyFromScreen($b.Location, [System.Drawing.Point]::Empty, $b.Size) }
        finally { $g.Dispose() }

        $dir = Split-Path -Parent $OutPath
        if ($dir -and -not (Test-Path -LiteralPath $dir)) { New-Item -ItemType Directory -Force -Path $dir | Out-Null }
        $bmp.Save($OutPath, [System.Drawing.Imaging.ImageFormat]::Png)

        $colors = New-Object 'System.Collections.Generic.HashSet[int]'
        for ($y = 0; $y -lt $b.Height; $y += $SampleStride) {
            for ($x = 0; $x -lt $b.Width; $x += $SampleStride) {
                [void]$colors.Add($bmp.GetPixel($x, $y).ToArgb())
            }
        }
        return [pscustomobject]@{
            Path = $OutPath; Width = $b.Width; Height = $b.Height
            DistinctColors = $colors.Count
            CapturedUtc = [datetime]::UtcNow
        }
    } finally { $bmp.Dispose() }
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
