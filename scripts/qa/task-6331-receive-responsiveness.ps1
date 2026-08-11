[CmdletBinding()]
param(
  [Parameter(Mandatory = $true)]
  [ValidateSet('quiet', 'busy')]
  [string]$Run,

  [Parameter(Mandatory = $true)]
  [string]$OslExePath,

  [Parameter(Mandatory = $true)]
  [ValidatePattern('^[0-9a-fA-F]{64}$')]
  [string]$OslExeSha256,

  [Parameter(Mandatory = $true)]
  [string]$ProviderEventPath,

  [Parameter(Mandatory = $true)]
  [string]$OutputPath,

  [string]$BindingOutputPath = '',

  [Parameter(Mandatory = $true)]
  [ValidateRange(1, 9223372036854775807)]
  [long]$RunStartQpc,

  [Parameter(Mandatory = $true)]
  [ValidateRange(1, 2147483647)]
  [int]$HarnessProcessId,

  [ValidateRange(300000, 300000)]
  [int]$DurationMilliseconds = 300000
)

# TASK 6331 outside-process observer.  It is intentionally a separate
# powershell.exe, not a Tauri command, renderer hook, or in-process test seam.
# UI Automation is read-only here: identity, focus, scroll state, and content
# observation.  Input crosses the Windows input queue only through SendInput,
# and a low-level OS hook must independently return the per-input cookie before
# a response can be accepted.

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type -AssemblyName System.Windows.Forms

Add-Type -TypeDefinition @'
using System;
using System.Collections.Concurrent;
using System.Diagnostics;
using System.Runtime.InteropServices;
using System.Text;
using System.Threading;

public static class Task6331Native {
  public delegate bool EnumWindowsProc(IntPtr hwnd, IntPtr state);
  private delegate IntPtr HookProc(int code, IntPtr wParam, IntPtr lParam);

  [StructLayout(LayoutKind.Sequential)] public struct RECT {
    public int Left, Top, Right, Bottom;
  }
  [StructLayout(LayoutKind.Sequential, Size=40)] public struct INPUT {
    public uint type;
    public INPUTUNION input;
  }
  [StructLayout(LayoutKind.Explicit)] public struct INPUTUNION {
    [FieldOffset(0)] public MOUSEINPUT mouse;
    [FieldOffset(0)] public KEYBDINPUT keyboard;
  }
  [StructLayout(LayoutKind.Sequential)] public struct MOUSEINPUT {
    public int dx, dy;
    public uint mouseData, dwFlags, time;
    public UIntPtr dwExtraInfo;
  }
  [StructLayout(LayoutKind.Sequential)] public struct KEYBDINPUT {
    public ushort wVk, wScan;
    public uint dwFlags, time;
    public UIntPtr dwExtraInfo;
  }
  [StructLayout(LayoutKind.Sequential)] private struct KBDLLHOOKSTRUCT {
    public uint vkCode, scanCode, flags, time;
    public UIntPtr dwExtraInfo;
  }
  [StructLayout(LayoutKind.Sequential)] private struct MSLLHOOKSTRUCT {
    public int x, y;
    public uint mouseData, flags, time;
    public UIntPtr dwExtraInfo;
  }
  [StructLayout(LayoutKind.Sequential)] private struct MSG {
    public IntPtr hwnd;
    public uint message;
    public UIntPtr wParam;
    public IntPtr lParam;
    public uint time;
    public int x, y;
  }

  public sealed class HookEvent {
    public ulong Cookie;
    public long Qpc;
    public string Kind;
    public bool Injected;
  }

  private static readonly ConcurrentDictionary<ulong, HookEvent> Events =
    new ConcurrentDictionary<ulong, HookEvent>();
  private static readonly ManualResetEventSlim Ready = new ManualResetEventSlim(false);
  private static HookProc keyboardProc = KeyboardHook;
  private static HookProc mouseProc = MouseHook;
  private static IntPtr keyboardHook = IntPtr.Zero;
  private static IntPtr mouseHook = IntPtr.Zero;
  private static uint hookThreadId;

  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumWindowsProc cb, IntPtr state);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hwnd);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint pid);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hwnd, out RECT rect);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hwnd);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern uint SendInput(uint count, INPUT[] inputs, int size);
  [DllImport("user32.dll")] private static extern IntPtr GetDC(IntPtr hwnd);
  [DllImport("user32.dll")] private static extern int ReleaseDC(IntPtr hwnd, IntPtr dc);
  [DllImport("gdi32.dll")] private static extern uint GetPixel(IntPtr dc, int x, int y);
  [DllImport("user32.dll")] private static extern IntPtr SetWindowsHookEx(int id, HookProc proc, IntPtr module, uint threadId);
  [DllImport("user32.dll")] private static extern bool UnhookWindowsHookEx(IntPtr hook);
  [DllImport("user32.dll")] private static extern IntPtr CallNextHookEx(IntPtr hook, int code, IntPtr wParam, IntPtr lParam);
  [DllImport("user32.dll")] private static extern sbyte GetMessage(out MSG msg, IntPtr hwnd, uint min, uint max);
  [DllImport("user32.dll")] private static extern bool PostThreadMessage(uint threadId, uint msg, UIntPtr wParam, IntPtr lParam);
  [DllImport("kernel32.dll")] private static extern uint GetCurrentThreadId();
  [DllImport("kernel32.dll")] public static extern bool QueryPerformanceCounter(out long value);
  [DllImport("kernel32.dll")] public static extern bool QueryPerformanceFrequency(out long value);

  public static IntPtr[] VisibleWindowsFor(uint processId) {
    var result = new System.Collections.Generic.List<IntPtr>();
    EnumWindows((hwnd, ignored) => {
      uint pid;
      GetWindowThreadProcessId(hwnd, out pid);
      if (pid == processId && IsWindowVisible(hwnd)) result.Add(hwnd);
      return true;
    }, IntPtr.Zero);
    return result.ToArray();
  }

  public static void StartHooks() {
    var thread = new Thread(() => {
      hookThreadId = GetCurrentThreadId();
      keyboardHook = SetWindowsHookEx(13, keyboardProc, IntPtr.Zero, 0);
      mouseHook = SetWindowsHookEx(14, mouseProc, IntPtr.Zero, 0);
      Ready.Set();
      if (keyboardHook == IntPtr.Zero || mouseHook == IntPtr.Zero) return;
      MSG message;
      while (GetMessage(out message, IntPtr.Zero, 0, 0) > 0) { }
      UnhookWindowsHookEx(keyboardHook);
      UnhookWindowsHookEx(mouseHook);
    });
    thread.IsBackground = true;
    thread.Name = "task-6331-os-input-hook";
    thread.Start();
    if (!Ready.Wait(2000) || keyboardHook == IntPtr.Zero || mouseHook == IntPtr.Zero)
      throw new InvalidOperationException("OS input hook did not bind");
  }

  public static void StopHooks() {
    if (hookThreadId != 0) PostThreadMessage(hookThreadId, 0x0012, UIntPtr.Zero, IntPtr.Zero);
  }

  private static IntPtr KeyboardHook(int code, IntPtr wParam, IntPtr lParam) {
    if (code >= 0) {
      var value = Marshal.PtrToStructure<KBDLLHOOKSTRUCT>(lParam);
      var cookie = value.dwExtraInfo.ToUInt64();
      if (cookie != 0) {
        long qpc; QueryPerformanceCounter(out qpc);
        Events[cookie] = new HookEvent { Cookie=cookie, Qpc=qpc, Kind="keyboard", Injected=(value.flags & 0x10) != 0 };
      }
    }
    return CallNextHookEx(keyboardHook, code, wParam, lParam);
  }

  private static IntPtr MouseHook(int code, IntPtr wParam, IntPtr lParam) {
    if (code >= 0) {
      var value = Marshal.PtrToStructure<MSLLHOOKSTRUCT>(lParam);
      var cookie = value.dwExtraInfo.ToUInt64();
      if (cookie != 0) {
        long qpc; QueryPerformanceCounter(out qpc);
        Events[cookie] = new HookEvent { Cookie=cookie, Qpc=qpc, Kind="pointer", Injected=(value.flags & 0x1) != 0 };
      }
    }
    return CallNextHookEx(mouseHook, code, wParam, lParam);
  }

  public static HookEvent WaitHook(ulong cookie, int timeoutMs) {
    var timer = Stopwatch.StartNew();
    HookEvent value;
    while (timer.ElapsedMilliseconds <= timeoutMs) {
      if (Events.TryRemove(cookie, out value)) return value;
      Thread.Sleep(1);
    }
    return null;
  }

  private const uint INPUT_MOUSE = 0;
  private const uint INPUT_KEYBOARD = 1;
  private const uint KEYEVENTF_KEYUP = 0x0002;
  private const uint MOUSEEVENTF_MOVE = 0x0001;
  private const uint MOUSEEVENTF_LEFTDOWN = 0x0002;
  private const uint MOUSEEVENTF_LEFTUP = 0x0004;
  private const uint MOUSEEVENTF_ABSOLUTE = 0x8000;
  private const uint MOUSEEVENTF_VIRTUALDESK = 0x4000;

  public static bool SendPointer(int normalizedX, int normalizedY, ulong cookie) {
    var move = new INPUT {
      type = INPUT_MOUSE,
      input = new INPUTUNION { mouse = new MOUSEINPUT {
        dx=normalizedX, dy=normalizedY,
        dwFlags=MOUSEEVENTF_MOVE|MOUSEEVENTF_ABSOLUTE|MOUSEEVENTF_VIRTUALDESK,
        dwExtraInfo=new UIntPtr(cookie)
      }}
    };
    var down = new INPUT { type=INPUT_MOUSE, input=new INPUTUNION {
      mouse=new MOUSEINPUT { dwFlags=MOUSEEVENTF_LEFTDOWN, dwExtraInfo=new UIntPtr(cookie) }
    }};
    var up = new INPUT { type=INPUT_MOUSE, input=new INPUTUNION {
      mouse=new MOUSEINPUT { dwFlags=MOUSEEVENTF_LEFTUP, dwExtraInfo=new UIntPtr(cookie) }
    }};
    return SendInput(3, new[] { move, down, up }, Marshal.SizeOf(typeof(INPUT))) == 3;
  }

  public static uint PixelAt(int x, int y) {
    var dc = GetDC(IntPtr.Zero);
    if (dc == IntPtr.Zero) return 0xffffffff;
    try { return GetPixel(dc, x, y); }
    finally { ReleaseDC(IntPtr.Zero, dc); }
  }

  public static bool SendKey(ushort virtualKey, ulong cookie) {
    var down = new INPUT { type=INPUT_KEYBOARD, input=new INPUTUNION {
      keyboard=new KEYBDINPUT { wVk=virtualKey, dwExtraInfo=new UIntPtr(cookie) }
    }};
    var up = new INPUT { type=INPUT_KEYBOARD, input=new INPUTUNION {
      keyboard=new KEYBDINPUT { wVk=virtualKey, dwFlags=KEYEVENTF_KEYUP, dwExtraInfo=new UIntPtr(cookie) }
    }};
    return SendInput(2, new[] { down, up }, Marshal.SizeOf(typeof(INPUT))) == 2;
  }
}
'@

function Qpc-Now {
  $value = [long]0
  if (-not [Task6331Native]::QueryPerformanceCounter([ref]$value)) { throw 'QPC unavailable' }
  $value
}

function Runtime-Id([Windows.Automation.AutomationElement]$Element) {
  [string]::Join(',', @($Element.GetRuntimeId()))
}

function Runtime-Id-Array([Windows.Automation.AutomationElement]$Element) {
  @($Element.GetRuntimeId() | ForEach-Object { [int]$_ })
}

function Exact-Osl-Process {
  $expected = [IO.Path]::GetFullPath($OslExePath)
  $matches = @(Get-Process | Where-Object {
    try { [IO.Path]::GetFullPath($_.Path) -ceq $expected } catch { $false }
  })
  if ($matches.Count -ne 1) { throw 'run=' + $Run + ' exact OSL process is absent or ambiguous' }
  $actual = (Get-FileHash -LiteralPath $expected -Algorithm SHA256).Hash
  if ($actual -cne $OslExeSha256.ToUpperInvariant()) { throw 'run=' + $Run + ' OSL executable SHA-256 mismatch' }
  $matches[0]
}

function Resolve-Surface([Diagnostics.Process]$Process) {
  $automationId = 'osl-protected-receive-surface'
  $condition = [Windows.Automation.PropertyCondition]::new(
    [Windows.Automation.AutomationElement]::AutomationIdProperty,
    $automationId
  )
  $matches = @()
  foreach ($hwnd in [Task6331Native]::VisibleWindowsFor([uint32]$Process.Id)) {
    try {
      $root = [Windows.Automation.AutomationElement]::FromHandle($hwnd)
      $found = @($root.FindAll([Windows.Automation.TreeScope]::Descendants, $condition))
      foreach ($element in $found) {
        if ($element.Current.IsOffscreen) { continue }
        if ([string]$element.Current.Name -cne 'Messages prepared or opened in this OSL panel') { continue }
        $matches += [pscustomobject]@{ Hwnd=$hwnd; Root=$root; Element=$element }
      }
    } catch [Windows.Automation.ElementNotAvailableException] {
    } catch [Runtime.InteropServices.COMException] {
    }
  }
  if ($matches.Count -ne 1) { throw 'run=' + $Run + ' receive surface is absent or ambiguous' }
  $pidValue = [uint32]0
  $threadId = [Task6331Native]::GetWindowThreadProcessId($matches[0].Hwnd, [ref]$pidValue)
  if ($pidValue -ne [uint32]$Process.Id -or $threadId -eq 0) { throw 'run=' + $Run + ' receive surface owner changed' }
  $bounds = $matches[0].Element.Current.BoundingRectangle
  if ($bounds.IsEmpty -or $bounds.Width -lt 80 -or $bounds.Height -lt 80) { throw 'run=' + $Run + ' receive surface is not visibly probeable' }
  [pscustomobject]@{
    Hwnd = ('0x{0:x}' -f $matches[0].Hwnd.ToInt64())
    HwndValue = $matches[0].Hwnd
    ProcessId = [int]$pidValue
    ThreadId = [int]$threadId
    RuntimeId = Runtime-Id $matches[0].Element
    RuntimeIdArray = Runtime-Id-Array $matches[0].Element
    AutomationId = $automationId
    Name = [string]$matches[0].Element.Current.Name
    Generation = 1
    Element = $matches[0].Element
    Bounds = @{ Left=[int]$bounds.Left; Top=[int]$bounds.Top; Width=[int]$bounds.Width; Height=[int]$bounds.Height }
  }
}

function Public-Surface([object]$Surface) {
  [ordered]@{
    hwnd=$Surface.Hwnd
    process_id=$Surface.ProcessId
    thread_id=$Surface.ThreadId
    runtime_id=$Surface.RuntimeIdArray
    generation=$Surface.Generation
    automation_id=$Surface.AutomationId
    name=$Surface.Name
  }
}

function Assert-Same-Surface([object]$Bound, [Diagnostics.Process]$Process) {
  try {
    if ((Runtime-Id $Bound.Element) -cne $Bound.RuntimeId -or
        [string]$Bound.Element.Current.AutomationId -cne $Bound.AutomationId -or
        [int]$Bound.Element.Current.ProcessId -ne $Bound.ProcessId) {
      throw 'stale'
    }
    $fresh = Resolve-Surface $Process
  } catch {
    $fresh = Resolve-Surface $Process
  }
  if ($fresh.Hwnd -cne $Bound.Hwnd -or $fresh.ProcessId -ne $Bound.ProcessId -or
      $fresh.ThreadId -ne $Bound.ThreadId -or $fresh.RuntimeId -cne $Bound.RuntimeId -or
      $fresh.Generation -ne $Bound.Generation) {
    throw ('run={0} surface={1} stale/recreated identity fresh={2}/{3}/{4}/{5}' -f
      $Run,$Bound.AutomationId,$fresh.Hwnd,$fresh.ProcessId,$fresh.ThreadId,$fresh.RuntimeId)
  }
  $fresh
}

function Scroll-Percent([object]$Surface) {
  $pattern = $null
  if (-not $Surface.Element.TryGetCurrentPattern(
    [Windows.Automation.ScrollPattern]::Pattern, [ref]$pattern
  )) { return $null }
  [double]([Windows.Automation.ScrollPattern]$pattern).Current.VerticalScrollPercent
}

function Focus-Runtime-Id {
  try { Runtime-Id ([Windows.Automation.AutomationElement]::FocusedElement) } catch { '' }
}

function Surface-Contains-Focus([object]$Surface) {
  try {
    $focused = [Windows.Automation.AutomationElement]::FocusedElement
    $walker = [Windows.Automation.TreeWalker]::ControlViewWalker
    for ($depth = 0; $null -ne $focused -and $depth -lt 64; $depth += 1) {
      if ((Runtime-Id $focused) -ceq $Surface.RuntimeId) { return $true }
      $focused = $walker.GetParent($focused)
    }
  } catch {}
  $false
}

function Read-New-Provider-Events {
  if (Test-Path -LiteralPath $ProviderEventPath -PathType Leaf) {
    $writeTicks = (Get-Item -LiteralPath $ProviderEventPath).LastWriteTimeUtc.Ticks
    if ($writeTicks -ne $script:providerLastWriteTicks) {
      $script:providerLastWriteTicks = $writeTicks
      foreach ($line in [IO.File]::ReadLines($ProviderEventPath)) {
        if ([string]::IsNullOrWhiteSpace($line)) { continue }
        $event = $line | ConvertFrom-Json -ErrorAction Stop
        if ([string]$event.run -cne $Run) { throw 'provider event run mismatch' }
        $providerId = [string]$event.provider_id
        if ($providerId -and $script:providerLogIds.Add($providerId)) {
          $script:providerPending[$providerId] = $event
        }
      }
    }
  }
  @($script:providerPending.Values)
}

function Find-Marker([object]$Surface, [string]$Text) {
  $condition = [Windows.Automation.PropertyCondition]::new(
    [Windows.Automation.AutomationElement]::NameProperty,
    $Text
  )
  $found = @($Surface.Element.FindAll([Windows.Automation.TreeScope]::Descendants, $condition) |
    Where-Object { -not $_.Current.IsOffscreen -and -not $_.Current.BoundingRectangle.IsEmpty })
  if ($found.Count -eq 1) { return $found[0] }
  if ($found.Count -gt 1) { throw 'marker content is ambiguous on receive surface' }
  $null
}

$frequency = [long]0
if (-not [Task6331Native]::QueryPerformanceFrequency([ref]$frequency) -or $frequency -le 0) {
  throw 'QPC frequency unavailable'
}
$observerProcess = Get-Process -Id $PID
$observerCim = Get-CimInstance Win32_Process -Filter "ProcessId = $PID"
$osl = Exact-Osl-Process
if ($PID -eq $osl.Id -or $PID -eq $HarnessProcessId) { throw 'observer must be outside OSL and harness' }
$surface = Resolve-Surface $osl
$publicSurface = Public-Surface $surface

$probes = [Collections.Generic.List[object]]::new()
$contentChanges = [Collections.Generic.List[object]]::new()
$identityResolutions = [Collections.Generic.List[object]]::new()
$seenProviders = [Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
$script:providerLogIds = [Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
$script:providerPending = @{}
$script:providerLastWriteTicks = [long]0
$boundQpc = Qpc-Now
$startedQpc = $RunStartQpc
$startedUtc = [DateTime]::UtcNow.ToString('o')
$deadlineQpc = $startedQpc + [long]($DurationMilliseconds * $frequency / 1000)
$nextQpc = $startedQpc
$probeIndex = 0
$lastResolveQpc = $startedQpc
$lastResponseQpc = $null
$longestLatencyMs = 0
$longestCompletedGapMs = 0
$identityResolutions.Add([ordered]@{ qpc=$boundQpc; reason='before-workload'; surface=$publicSurface })

if ($boundQpc -ge $startedQpc) { throw 'coordinated run start must follow surface binding' }
if (-not $BindingOutputPath) { $BindingOutputPath = "$OutputPath.binding.json" }
$binding = [ordered]@{
  schema='osl-task-6331-binding-v1'
  run=$Run
  qpc_frequency=$frequency
  bound_qpc=$boundQpc
  run_start_qpc=$startedQpc
  run_end_qpc=$deadlineQpc
  observer_process_id=$PID
  surface=$publicSurface
}
$bindingTemporary = "$BindingOutputPath.partial"
[IO.File]::WriteAllText(
  $bindingTemporary,
  ($binding | ConvertTo-Json -Depth 6),
  [Text.UTF8Encoding]::new($false)
)
[IO.File]::Move($bindingTemporary, $BindingOutputPath)

while ((Qpc-Now) -lt $startedQpc) {
  $remainingMs = [int](($startedQpc - (Qpc-Now)) * 1000 / $frequency)
  if ($remainingMs -gt 1) { Start-Sleep -Milliseconds ([Math]::Min(10, $remainingMs - 1)) }
}

[Task6331Native]::StartHooks()
try {
  if (-not [Task6331Native]::SetForegroundWindow($surface.HwndValue)) {
    throw ('run={0} surface={1} initial foreground was rejected' -f $Run,$surface.AutomationId)
  }
  while ($nextQpc -lt $deadlineQpc) {
    $now = Qpc-Now
    if ($now -lt $nextQpc) {
      $sleepMs = [Math]::Max(0, [int](($nextQpc - $now) * 1000 / $frequency) - 1)
      if ($sleepMs -gt 0) { Start-Sleep -Milliseconds $sleepMs }
      continue
    }
    # Resolve the exact role-log at least once per second, and validate its
    # cached RuntimeId on every probe. Any navigation/recreation becomes red.
    if (($now - $lastResolveQpc) * 1000 / $frequency -ge 1000) {
      $surface = Assert-Same-Surface $surface $osl
      $lastResolveQpc = $now
      $identityResolutions.Add([ordered]@{ qpc=$now; reason='periodic'; surface=(Public-Surface $surface) })
    } else {
      if ((Runtime-Id $surface.Element) -cne $surface.RuntimeId) {
        $surface = Assert-Same-Surface $surface $osl
      }
    }

    $kind = if (($probeIndex % 2) -eq 0) { 'pointer' } else { 'keyboard' }
    $inputId = '{0}-input-{1:d6}' -f $Run,$probeIndex
    $cookie = [uint64](0x6331000000000000L + $probeIndex + 1)
    $beforeFocus = Focus-Runtime-Id
    $beforeScroll = Scroll-Percent $surface
    $sampleX = $surface.Bounds.Left + [Math]::Max(4, $surface.Bounds.Width - 12)
    $sampleY = $surface.Bounds.Top + [Math]::Max(8, [int]($surface.Bounds.Height / 4))
    $beforePixel = [Task6331Native]::PixelAt($sampleX, $sampleY)
    $requestedQpc = Qpc-Now
    if ($kind -ceq 'pointer') {
      $phase = [int](($probeIndex / 2) % 2)
      # The far-right row padding is inside the bound role-log but outside its
      # buttons and text, so the click only performs ordinary row/root focus.
      $x = $sampleX
      $y = if ($phase -eq 0) {
        $surface.Bounds.Top + [Math]::Max(8, [int]($surface.Bounds.Height / 4))
      } else {
        $surface.Bounds.Top + [Math]::Max(12, [int]($surface.Bounds.Height * 3 / 4))
      }
      $virtual = [Windows.Forms.SystemInformation]::VirtualScreen
      $nx = [int](($x - $virtual.Left) * 65535 / [Math]::Max(1, $virtual.Width - 1))
      $ny = [int](($y - $virtual.Top) * 65535 / [Math]::Max(1, $virtual.Height - 1))
      $sent = [Task6331Native]::SendPointer($nx, $ny, $cookie)
    } else {
      # PAGE UP/DOWN is harmless receive-surface navigation. The exact role-log
      # must already own focus from real pointer input; no SetFocus is used.
      if (-not (Surface-Contains-Focus $surface)) {
        throw ('run={0} surface={1} input_id={2} keyboard target lost focus' -f $Run,$surface.AutomationId,$inputId)
      }
      $keyPair = [int][Math]::Floor($probeIndex / 2)
      $key = if (($keyPair % 2) -eq 0) { [uint16]0x21 } else { [uint16]0x22 }
      $sent = [Task6331Native]::SendKey($key, $cookie)
    }
    if (-not $sent) {
      throw ('run={0} surface={1} input_id={2} requested_qpc={3} SendInput rejected' -f $Run,$surface.AutomationId,$inputId,$requestedQpc)
    }
    $hook = [Task6331Native]::WaitHook($cookie, 500)
    if ($null -eq $hook -or -not $hook.Injected -or $hook.Kind -cne $kind) {
      throw ('run={0} surface={1} input_id={2} requested_qpc={3} OS hook unmatched' -f $Run,$surface.AutomationId,$inputId,$requestedQpc)
    }

    $responseKind = ''
    $responseQpc = [long]0
    do {
      $afterFocus = Focus-Runtime-Id
      $afterScroll = Scroll-Percent $surface
      $afterPixel = [Task6331Native]::PixelAt($sampleX, $sampleY)
      if ($afterFocus -cne $beforeFocus -and (Surface-Contains-Focus $surface)) {
        $responseKind = 'focus'
      } elseif ($null -ne $beforeScroll -and $null -ne $afterScroll -and
                [double]$afterScroll -ne [double]$beforeScroll) {
        $responseKind = 'scroll'
      } elseif ($kind -ceq 'pointer' -and $afterPixel -ne $beforePixel) {
        $responseKind = 'pixel'
      }
      if ($responseKind) { $responseQpc = Qpc-Now; break }
      Start-Sleep -Milliseconds 1
    } while (((Qpc-Now) - $hook.Qpc) * 1000 / $frequency -le 500)
    if (-not $responseKind) {
      throw ('run={0} surface={1} input_id={2} injected_qpc={3} response_qpc=missing no visible response' -f $Run,$surface.AutomationId,$inputId,$hook.Qpc)
    }
    $latencyMs = [int][Math]::Ceiling(($responseQpc - $requestedQpc) * 1000.0 / $frequency)
    $completedGapMs = if ($null -eq $lastResponseQpc) { $null } else {
      [int][Math]::Ceiling(($responseQpc - [long]$lastResponseQpc) * 1000.0 / $frequency)
    }
    $longestLatencyMs = [Math]::Max($longestLatencyMs, $latencyMs)
    if ($null -ne $completedGapMs) {
      $longestCompletedGapMs = [Math]::Max($longestCompletedGapMs, $completedGapMs)
    }
    $lastResponseQpc = $responseQpc
    $probes.Add([ordered]@{
      input_id=$inputId
      kind=$kind
      injection_api='SendInput'
      direct_invocation=$false
      sent_qpc=$requestedQpc
      response_qpc=$responseQpc
      latency_ms=$latencyMs
      completed_gap_ms=$completedGapMs
      target_surface=$publicSurface
      os_hook=@{
        input_id=$inputId
        qpc=[long]$hook.Qpc
        hook=if ($kind -ceq 'pointer') { 'WH_MOUSE_LL' } else { 'WH_KEYBOARD_LL' }
        api='SendInput'
        injected_flag=if ($kind -ceq 'pointer') { 'LLMHF_INJECTED' } else { 'LLKHF_INJECTED' }
      }
      visible_response=@{
        kind=$responseKind
        changed=$true
        qpc=$responseQpc
        surface=$publicSurface
      }
    })

    foreach ($provider in Read-New-Provider-Events) {
      $providerId = [string]$provider.provider_id
      if (-not $providerId -or $seenProviders.Contains($providerId)) { continue }
      $row = Find-Marker $surface ([string]$provider.marker_text)
      if ($null -eq $row) { continue }
      $seenProviders.Add($providerId) | Out-Null
      [void]$script:providerPending.Remove($providerId)
      $observedQpc = Qpc-Now
      $providerQpc = [long]$provider.provider_qpc
      $contentChanges.Add([ordered]@{
        provider_id=$providerId
        marker_text=[string]$provider.marker_text
        change_kind='row_addition'
        observed_qpc=$observedQpc
        latency_ms=[int][Math]::Ceiling(($observedQpc - $providerQpc) * 1000.0 / $frequency)
        row_runtime_id=Runtime-Id $row
        witness_input_id=$inputId
        surface=$publicSurface
      })
    }

    $probeIndex += 1
    $nextQpc = $startedQpc + [long]($probeIndex * 50 * $frequency / 1000)
  }
} finally {
  [Task6331Native]::StopHooks()
}

$completedQpc = Qpc-Now
$record = [ordered]@{
  schema='osl-task-6331-observer-v1'
  run=$Run
  observer_process=@{
    pid=$PID
    path=[string]$observerProcess.Path
    role='outside-windows-observer'
  }
  clock=@{
    qpc_frequency=$frequency
    run_start_qpc=$startedQpc
    run_end_qpc=$deadlineQpc
  }
  actual_completed_qpc=$completedQpc
  started_utc=$startedUtc
  completed_utc=[DateTime]::UtcNow.ToString('o')
  surface=$publicSurface
  identity_resolutions=$identityResolutions
  destroyed_runtime_ids=@()
  probes=$probes
  probe_count=$probes.Count
  longest_latency_ms=$longestLatencyMs
  longest_completed_gap_ms=$longestCompletedGapMs
  content_changes=$contentChanges
  content_change_count=$contentChanges.Count
}
$json = $record | ConvertTo-Json -Depth 10
$temporary = "$OutputPath.partial"
[IO.File]::WriteAllText($temporary, $json, [Text.UTF8Encoding]::new($false))
[IO.File]::Move($temporary, $OutputPath)
$json
