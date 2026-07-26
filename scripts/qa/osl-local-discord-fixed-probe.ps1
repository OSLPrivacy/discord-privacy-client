[CmdletBinding()]
param(
  [Parameter(Mandatory = $true)]
  [ValidateSet('Trigger', 'Inspect', 'TriggerAndInspect')]
  [string]$Action,

  [Parameter(Mandatory = $true)]
  [string]$QaCoreRoot,

  [string]$OslExePath,

  [ValidatePattern('^[0-9a-fA-F]{64}$')]
  [string]$OslExeSha256,

  [ValidateRange(1, 30)]
  [int]$WaitSeconds = 20
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$receiptNames = @{
  SendStage   = 'discord-qa-send-stage-receipt.json'
  Outbound    = 'discord-qa-outbound-receipt.json'
  Inbound     = 'discord-qa-inbound-receipt.json'
  Poll        = 'discord-qa-inbound-poll-receipt.json'
  OverlayOpen = 'discord-qa-overlay-open-receipt.json'
}

function Read-Receipt([string]$Name) {
  $path = Join-Path $QaCoreRoot $receiptNames[$Name]
  if (-not [IO.File]::Exists($path)) { return $null }
  $bytes = [IO.File]::ReadAllBytes($path)
  if ($bytes.Length -eq 0 -or $bytes.Length -gt 65536) {
    throw "Invalid bounded $Name receipt."
  }
  try {
    return [Text.Encoding]::UTF8.GetString($bytes) | ConvertFrom-Json
  } catch {
    throw "Invalid $Name receipt JSON."
  }
}

function Assert-HashOrNull([object]$Value, [string]$Field) {
  if ($null -ne $Value -and [string]$Value -cnotmatch '^[0-9a-f]{64}$') {
    throw "Invalid hash-safe receipt field: $Field."
  }
}

function Get-SafeReceiptSnapshot {
  $send = Read-Receipt 'SendStage'
  $outbound = Read-Receipt 'Outbound'
  $inbound = Read-Receipt 'Inbound'
  $poll = Read-Receipt 'Poll'
  $overlay = Read-Receipt 'OverlayOpen'

  if ($null -ne $send) { Assert-HashOrNull $send.plaintextSha256 'send.plaintextSha256' }
  if ($null -ne $outbound) {
    Assert-HashOrNull $outbound.plaintextSha256 'outbound.plaintextSha256'
    Assert-HashOrNull $outbound.messageIdSha256 'outbound.messageIdSha256'
  }

  $safeMessages = @()
  if ($null -ne $inbound -and $null -ne $inbound.messages) {
    foreach ($message in @($inbound.messages)) {
      Assert-HashOrNull $message.plaintextSha256 'inbound.messages.plaintextSha256'
      $safeMessages += [pscustomobject]@{
        PlaintextSha256    = [string]$message.plaintextSha256
        Utf8Bytes          = [int]$message.utf8Bytes
        Lines              = [int]$message.lines
        ContextVerified    = [bool]$message.contextVerified
        PersonToPersonE2ee = [bool]$message.personToPersonE2ee
        ViewOnceConsumed   = [bool]$message.viewOnceConsumed
        ExpiresAt          = [long]$message.expiresAt
      }
    }
  }

  [pscustomobject]@{
    SendStage = if ($null -eq $send) { $null } else {
      [pscustomobject]@{
        ObservedAtUnixMs          = [decimal]$send.observedAtUnixMs
        PlaintextSha256           = [string]$send.plaintextSha256
        RegistrationTerminalState = [string]$send.registrationTerminalState
        KeyserverAvailable        = [bool]$send.keyserverAvailable
        IdentityUnchanged         = [bool]$send.identityUnchanged
        OverlayContextVerified    = [bool]$send.overlayContextVerified
        PostSucceeded             = [bool]$send.postSucceeded
        Phase                     = [string]$send.phase
        PhaseOutcome              = [string]$send.phaseOutcome
        ErrorClass                = if ($null -eq $send.errorClass) { $null } else { [string]$send.errorClass }
      }
    }
    Outbound = if ($null -eq $outbound) { $null } else {
      [pscustomobject]@{
        ObservedAtUnixMs       = [decimal]$outbound.observedAtUnixMs
        PlaintextSha256        = [string]$outbound.plaintextSha256
        MessageIdSha256        = [string]$outbound.messageIdSha256
        Utf8Bytes              = [int]$outbound.utf8Bytes
        Lines                  = [int]$outbound.lines
        PersonToPersonE2ee     = [bool]$outbound.personToPersonE2ee
        ViewOnce               = [bool]$outbound.viewOnce
        DeliveredToOslInbox    = [bool]$outbound.deliveredToOslInbox
      }
    }
    Inbound = [pscustomobject]@{
      ObservedAtUnixMs = if ($null -eq $inbound) { $null } else { [decimal]$inbound.observedAtUnixMs }
      Messages         = $safeMessages
    }
    Poll = if ($null -eq $poll) { $null } else {
      [pscustomobject]@{
        ObservedAtUnixMs     = [decimal]$poll.observedAtUnixMs
        Outcome              = [string]$poll.outcome
        ErrorClass           = if ($null -eq $poll.errorClass) { $null } else { [string]$poll.errorClass }
        OpenedCount          = [int]$poll.openedCount
        PendingViewOnceCount = [int]$poll.pendingViewOnceCount
        AcknowledgmentCount  = [int]$poll.acknowledgmentCount
        Fetched              = [int]$poll.fetched
      }
    }
    OverlayOpen = if ($null -eq $overlay) { $null } else {
      [pscustomobject]@{
        ObservedAtUnixMs = [decimal]$overlay.observedAtUnixMs
        Outcome          = [string]$overlay.outcome
        ErrorClass       = if ($null -eq $overlay.errorClass) { $null } else { [string]$overlay.errorClass }
      }
    }
  }
}

function Assert-TriggerInputs {
  if ([string]::IsNullOrWhiteSpace($OslExePath) -or [string]::IsNullOrWhiteSpace($OslExeSha256)) {
    throw 'Trigger requires OslExePath and OslExeSha256.'
  }
  $script:ExpectedPath = [IO.Path]::GetFullPath($OslExePath)
  if (-not [IO.File]::Exists($script:ExpectedPath)) { throw 'Exact OSL executable is absent.' }
  $actual = (Get-FileHash -LiteralPath $script:ExpectedPath -Algorithm SHA256).Hash.ToLowerInvariant()
  if ($actual -cne $OslExeSha256.ToLowerInvariant()) { throw 'Exact OSL executable hash mismatch.' }
}

if ($Action -ne 'Inspect') {
  Assert-TriggerInputs
  Add-Type @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;

public static class OslFixedProbeNative {
  public delegate bool EnumWindowsProc(IntPtr hwnd, IntPtr state);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumWindowsProc callback, IntPtr state);
  [DllImport("user32.dll")] public static extern bool EnumChildWindows(IntPtr parent, EnumWindowsProc callback, IntPtr state);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hwnd);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint processId);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetWindowText(IntPtr hwnd, StringBuilder value, int capacity);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetClassName(IntPtr hwnd, StringBuilder value, int capacity);
  [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr hwnd, uint message, IntPtr wParam, IntPtr lParam);

  private static string Text(IntPtr hwnd) {
    var value = new StringBuilder(256);
    GetWindowText(hwnd, value, value.Capacity);
    return value.ToString();
  }
  private static string Class(IntPtr hwnd) {
    var value = new StringBuilder(256);
    GetClassName(hwnd, value, value.Capacity);
    return value.ToString();
  }
  public static IntPtr[] ExactOverlayRoots(uint processId) {
    var values = new List<IntPtr>();
    EnumWindows((hwnd, ignored) => {
      uint actual;
      GetWindowThreadProcessId(hwnd, out actual);
      if (actual == processId && IsWindowVisible(hwnd) && Text(hwnd) == "OSL private composer") values.Add(hwnd);
      return true;
    }, IntPtr.Zero);
    return values.ToArray();
  }
  public static IntPtr[] VisibleRenderers(IntPtr root) {
    var values = new List<IntPtr>();
    EnumChildWindows(root, (hwnd, ignored) => {
      if (IsWindowVisible(hwnd) && Class(hwnd) == "Chrome_RenderWidgetHostHWND") values.Add(hwnd);
      return true;
    }, IntPtr.Zero);
    return values.ToArray();
  }
  public static bool PostFixedF12(IntPtr renderer) {
    const uint WM_KEYDOWN = 0x0100, WM_KEYUP = 0x0101;
    const int VK_F12 = 0x7B, SCAN_F12 = 0x58;
    var down = (IntPtr)(1 | (SCAN_F12 << 16));
    var up = (IntPtr)(1 | (SCAN_F12 << 16) | (1 << 30) | unchecked((int)0x80000000));
    return renderer != IntPtr.Zero
      && PostMessage(renderer, WM_KEYDOWN, (IntPtr)VK_F12, down)
      && PostMessage(renderer, WM_KEYUP, (IntPtr)VK_F12, up);
  }
}
'@

  $processes = @(Get-Process -ErrorAction Stop | Where-Object {
    try { [IO.Path]::GetFullPath($_.Path) -ceq $script:ExpectedPath } catch { $false }
  })
  if ($processes.Count -ne 1) { throw 'Expected exactly one running exact OSL QA process.' }
  $roots = @([OslFixedProbeNative]::ExactOverlayRoots([uint32]$processes[0].Id))
  if ($roots.Count -ne 1) { throw 'Expected exactly one visible exact OSL private composer.' }
  $renderers = @([OslFixedProbeNative]::VisibleRenderers($roots[0]))
  if ($renderers.Count -ne 1) { throw 'Expected exactly one visible trusted overlay renderer.' }

  $before = Get-SafeReceiptSnapshot
  $baseline = if ($null -eq $before.Outbound) { [decimal]0 } else { $before.Outbound.ObservedAtUnixMs }
  $foreground = [OslFixedProbeNative]::GetForegroundWindow()
  if (-not [OslFixedProbeNative]::PostFixedF12($renderers[0])) {
    throw 'The fixed background QA gesture was rejected.'
  }
  if ([OslFixedProbeNative]::GetForegroundWindow() -ne $foreground) {
    throw 'Foreground changed while posting the background QA gesture.'
  }

  if ($Action -eq 'Trigger') {
    [pscustomobject]@{ Posted = $true; ForegroundUnchanged = $true } | ConvertTo-Json -Depth 4 -Compress
    exit 0
  }

  $deadline = [DateTime]::UtcNow.AddSeconds($WaitSeconds)
  do {
    Start-Sleep -Milliseconds 250
    $safe = Get-SafeReceiptSnapshot
    if ($null -ne $safe.Outbound -and $safe.Outbound.ObservedAtUnixMs -gt $baseline) { break }
    if ($null -ne $safe.SendStage -and $safe.SendStage.PhaseOutcome -ceq 'error') { break }
  } while ([DateTime]::UtcNow -lt $deadline)

  [pscustomobject]@{
    Posted              = $true
    ForegroundUnchanged = ([OslFixedProbeNative]::GetForegroundWindow() -eq $foreground)
    Receipts            = $safe
  } | ConvertTo-Json -Depth 8 -Compress
  exit 0
}

Get-SafeReceiptSnapshot | ConvertTo-Json -Depth 8 -Compress
