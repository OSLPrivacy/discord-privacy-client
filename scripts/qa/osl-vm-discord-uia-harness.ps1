param(
  [Parameter(Mandatory = $true)]
  [ValidateSet(
    'Inventory',
    'Inspect',
    'OpenCurrent',
    'PrepareOverlay',
    'OpenVerifiedFriend',
    'SetDeterministic',
    'Send',
    'InspectInbound',
    'ToggleVisibility',
    'ExerciseWindowLifecycle'
  )]
  [string]$Action,

  [Parameter(Mandatory = $true)]
  [string]$OslExePath,

  [Parameter(Mandatory = $true)]
  [string]$OslExeSha256,

  [Parameter(Mandatory = $true)]
  [int]$SessionId,

  [ValidatePattern('^[A-Za-z0-9._-]{1,48}$')]
  [string]$CaseId = 'BASE',

  [ValidateSet('On', 'Off')]
  [string]$Visibility = 'On',

  [ValidateRange(10, 180)]
  [int]$TimeoutSeconds = 60
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type -AssemblyName System.Windows.Forms
Add-Type @"
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;

public static class OslVmDiscordUiaNative {
  public delegate bool EnumWindowsProc(IntPtr hwnd, IntPtr lParam);
  [StructLayout(LayoutKind.Sequential)]
  public struct Rect { public int Left; public int Top; public int Right; public int Bottom; }
  [StructLayout(LayoutKind.Sequential)]
  public struct Point { public int X; public int Y; }

  [DllImport("user32.dll")]
  public static extern bool EnumWindows(EnumWindowsProc callback, IntPtr lParam);
  [DllImport("user32.dll")]
  public static extern bool EnumChildWindows(IntPtr parent, EnumWindowsProc callback, IntPtr lParam);
  [DllImport("user32.dll")]
  public static extern bool IsWindow(IntPtr hwnd);
  [DllImport("user32.dll")]
  public static extern bool IsWindowVisible(IntPtr hwnd);
  [DllImport("user32.dll")]
  public static extern bool IsIconic(IntPtr hwnd);
  [DllImport("user32.dll")]
  public static extern bool IsChild(IntPtr parent, IntPtr hwnd);
  [DllImport("user32.dll")]
  public static extern IntPtr GetParent(IntPtr hwnd);
  [DllImport("user32.dll")]
  public static extern IntPtr GetWindow(IntPtr hwnd, uint command);
  [DllImport("user32.dll")]
  public static extern bool GetClientRect(IntPtr hwnd, out Rect rect);
  [DllImport("user32.dll")]
  public static extern bool GetWindowRect(IntPtr hwnd, out Rect rect);
  [DllImport("user32.dll")]
  public static extern int MapWindowPoints(IntPtr from, IntPtr to, ref Point points, uint count);
  [DllImport("user32.dll")]
  public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")]
  public static extern bool SetForegroundWindow(IntPtr hwnd);
  [DllImport("user32.dll")]
  public static extern bool PostMessage(IntPtr hwnd, uint message, IntPtr wParam, IntPtr lParam);
  [DllImport("user32.dll")]
  public static extern bool ShowWindowAsync(IntPtr hwnd, int command);
  [DllImport("user32.dll")]
  private static extern void keybd_event(byte virtualKey, byte scanCode, uint flags, UIntPtr extraInfo);
  [DllImport("user32.dll")]
  public static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint processId);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)]
  public static extern int GetWindowText(IntPtr hwnd, StringBuilder value, int capacity);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)]
  public static extern int GetClassName(IntPtr hwnd, StringBuilder value, int capacity);

  public static string WindowText(IntPtr hwnd) {
    var value = new StringBuilder(512);
    GetWindowText(hwnd, value, value.Capacity);
    return value.ToString();
  }

  public static string WindowClass(IntPtr hwnd) {
    var value = new StringBuilder(256);
    GetClassName(hwnd, value, value.Capacity);
    return value.ToString();
  }

  public static IntPtr[] VisibleTopLevelWindowsFor(uint expectedProcessId) {
    var values = new List<IntPtr>();
    EnumWindows((hwnd, ignored) => {
      uint processId;
      GetWindowThreadProcessId(hwnd, out processId);
      if (processId == expectedProcessId && IsWindowVisible(hwnd)) values.Add(hwnd);
      return true;
    }, IntPtr.Zero);
    return values.ToArray();
  }

  public static IntPtr[] VisibleRenderersFor(IntPtr root) {
    var values = new List<IntPtr>();
    EnumChildWindows(root, (hwnd, ignored) => {
      if (IsWindowVisible(hwnd) && WindowClass(hwnd) == "Chrome_RenderWidgetHostHWND") {
        values.Add(hwnd);
      }
      return true;
    }, IntPtr.Zero);
    return values.ToArray();
  }

  public static IntPtr[] WindowsFor(uint[] expectedProcessIds) {
    var expected = new HashSet<uint>(expectedProcessIds);
    var values = new HashSet<IntPtr>();
    EnumWindows((root, ignored) => {
      uint processId;
      GetWindowThreadProcessId(root, out processId);
      if (expected.Contains(processId)) values.Add(root);
      EnumChildWindows(root, (hwnd, childIgnored) => {
        GetWindowThreadProcessId(hwnd, out processId);
        if (expected.Contains(processId)) values.Add(hwnd);
        return true;
      }, IntPtr.Zero);
      return true;
    }, IntPtr.Zero);
    var result = new IntPtr[values.Count];
    values.CopyTo(result);
    return result;
  }

  private static IntPtr DirectBranch(IntPtr parent, IntPtr hwnd) {
    var current = hwnd;
    while (current != IntPtr.Zero && GetParent(current) != parent) current = GetParent(current);
    return current;
  }

  public static bool IsBranchAbove(IntPtr parent, IntPtr upper, IntPtr lower) {
    const uint GW_CHILD = 5;
    const uint GW_HWNDNEXT = 2;
    var upperBranch = DirectBranch(parent, upper);
    var lowerBranch = DirectBranch(parent, lower);
    if (upperBranch == IntPtr.Zero || lowerBranch == IntPtr.Zero || upperBranch == lowerBranch) return false;
    for (var current = GetWindow(parent, GW_CHILD); current != IntPtr.Zero; current = GetWindow(current, GW_HWNDNEXT)) {
      if (current == upperBranch) return true;
      if (current == lowerBranch) return false;
    }
    return false;
  }

  public static bool IsRetethered(IntPtr parent, IntPtr child, int verticalReserve, int tolerance) {
    Rect client;
    Rect window;
    if (!GetClientRect(parent, out client) || !GetWindowRect(child, out window)) return false;
    var topLeft = new Point { X = window.Left, Y = window.Top };
    var bottomRight = new Point { X = window.Right, Y = window.Bottom };
    MapWindowPoints(IntPtr.Zero, parent, ref topLeft, 1);
    MapWindowPoints(IntPtr.Zero, parent, ref bottomRight, 1);
    var expectedWidth = Math.Max(1, client.Right - client.Left);
    var expectedHeight = Math.Max(1, client.Bottom - verticalReserve);
    return Math.Abs(topLeft.X) <= tolerance
      && Math.Abs(topLeft.Y - verticalReserve) <= tolerance
      && Math.Abs((bottomRight.X - topLeft.X) - expectedWidth) <= tolerance
      && Math.Abs((bottomRight.Y - topLeft.Y) - expectedHeight) <= tolerance;
  }

  public static bool PressF11(IntPtr renderer) {
    const uint WM_KEYDOWN = 0x0100;
    const uint WM_KEYUP = 0x0101;
    const int VK_F11 = 0x7A;
    // F11's set-1 scan code is 0x57. Chromium/WebView2 expects the same
    // repeat/scan/transition bits Windows places in a real key message.
    var down = (IntPtr)(1 | (0x57 << 16));
    var up = (IntPtr)(1 | (0x57 << 16) | (1 << 30) | unchecked((int)0x80000000));
    return renderer != IntPtr.Zero
      && PostMessage(renderer, WM_KEYDOWN, (IntPtr)VK_F11, down)
      && PostMessage(renderer, WM_KEYUP, (IntPtr)VK_F11, up);
  }

  public static void PressF11Keyboard() {
    const byte VK_F11 = 0x7A;
    const byte SCAN_F11 = 0x57;
    const uint KEYEVENTF_KEYUP = 0x0002;
    keybd_event(VK_F11, SCAN_F11, 0, UIntPtr.Zero);
    keybd_event(VK_F11, SCAN_F11, KEYEVENTF_KEYUP, UIntPtr.Zero);
  }
}
"@

$script:HarnessDeadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
$script:ExpectedOslPath = [IO.Path]::GetFullPath($OslExePath)
$script:ExpectedOslSha = $OslExeSha256.ToLowerInvariant()
if ($script:ExpectedOslSha -cnotmatch '^[0-9a-f]{64}$') { throw 'invalid OSL executable SHA-256' }

function ConvertTo-SafeJson([object]$Value) {
  $Value | ConvertTo-Json -Depth 8 -Compress
}

function Test-SameRuntimeId([Windows.Automation.AutomationElement]$Left, [Windows.Automation.AutomationElement]$Right) {
  try {
    $leftId = @($Left.GetRuntimeId())
    $rightId = @($Right.GetRuntimeId())
    return $leftId.Count -eq $rightId.Count -and [string]::Join(',', $leftId) -ceq [string]::Join(',', $rightId)
  } catch [Windows.Automation.ElementNotAvailableException] {
    return $false
  }
}

function Get-ExactOslProcess {
  $hash = (Get-FileHash -LiteralPath $script:ExpectedOslPath -Algorithm SHA256).Hash.ToLowerInvariant()
  if ($hash -cne $script:ExpectedOslSha) { throw 'exact OSL executable hash mismatch' }

  $allMatches = @(Get-CimInstance Win32_Process | Where-Object {
    $_.ExecutablePath -and
    [string]::Equals(
      [IO.Path]::GetFullPath([string]$_.ExecutablePath),
      $script:ExpectedOslPath,
      [StringComparison]::OrdinalIgnoreCase
    ) -and [int]$_.SessionId -eq $SessionId
  })
  $matches = @($allMatches | Where-Object {
    [string]$_.CommandLine -cnotmatch '(?:^|\s)--osl-borrowed-window-guardian-v1(?:\s|$)'
  })
  if ($matches.Count -eq 0) { throw 'exact non-guardian OSL process identity is unavailable' }
  $windowOwners = @()
  foreach ($candidate in $matches) {
    $mainWindows = @([OslVmDiscordUiaNative]::VisibleTopLevelWindowsFor([uint32]$candidate.ProcessId) | Where-Object {
      [OslVmDiscordUiaNative]::WindowText($_) -ceq 'OSL Privacy' -and
      [OslVmDiscordUiaNative]::WindowClass($_) -ceq 'Tauri Window'
    })
    if ($mainWindows.Count -gt 1) { throw 'exact OSL main window is ambiguous within its process' }
    if ($mainWindows.Count -eq 1) { $windowOwners += $candidate }
  }
  if ($windowOwners.Count -ne 1) { throw 'exact visible OSL process identity is unavailable or ambiguous' }

  $match = $windowOwners[0]
  $process = Get-Process -Id ([int]$match.ProcessId) -ErrorAction Stop
  [pscustomobject]@{
    ProcessId = [int]$match.ProcessId
    SessionId = [int]$match.SessionId
    StartTimeUtcTicks = $process.StartTime.ToUniversalTime().Ticks
    ExecutablePath = [string]$match.ExecutablePath
    GuardianProcessIds = @($allMatches | Where-Object {
      [string]$_.CommandLine -cmatch '(?:^|\s)--osl-borrowed-window-guardian-v1(?:\s|$)'
    } | ForEach-Object { [int]$_.ProcessId } | Sort-Object)
  }
}

function Get-TrustedRendererProcess([int]$ProcessId, [int]$OslProcessId) {
  $process = Get-CimInstance Win32_Process -Filter "ProcessId = $ProcessId"
  if (-not $process -or [int]$process.SessionId -ne $SessionId -or -not $process.ExecutablePath) {
    throw 'renderer process session identity is unavailable'
  }
  if ($ProcessId -ne $OslProcessId) {
    $name = [IO.Path]::GetFileName([string]$process.ExecutablePath)
    if (-not [string]::Equals($name, 'msedgewebview2.exe', [StringComparison]::OrdinalIgnoreCase)) {
      throw 'renderer executable identity is not WebView2'
    }
    $signature = Get-AuthenticodeSignature -LiteralPath ([string]$process.ExecutablePath)
    if ($signature.Status -ne [System.Management.Automation.SignatureStatus]::Valid -or
        -not $signature.SignerCertificate -or
        $signature.SignerCertificate.Subject -notmatch '(?i)Microsoft Corporation') {
      throw 'renderer publisher identity is not trusted'
    }
  }
  [pscustomobject]@{
    ProcessId = [int]$process.ProcessId
    SessionId = [int]$process.SessionId
    ExecutablePath = [string]$process.ExecutablePath
  }
}

function Find-VisibleDiscordRenderer([IntPtr]$RootHwnd, [int]$OslProcessId) {
  $candidates = @()
  foreach ($rendererHwnd in [OslVmDiscordUiaNative]::VisibleRenderersFor($RootHwnd)) {
    try {
      $element = [Windows.Automation.AutomationElement]::FromHandle($rendererHwnd)
      if (-not $element -or $element.Current.IsOffscreen -or $element.Current.BoundingRectangle.IsEmpty) { continue }
      [uint32]$rendererProcessId = 0
      [void][OslVmDiscordUiaNative]::GetWindowThreadProcessId($rendererHwnd, [ref]$rendererProcessId)
      $rendererProcess = Get-CimInstance Win32_Process -Filter "ProcessId = $rendererProcessId"
      if ($rendererProcess -and $rendererProcess.ExecutablePath -and
          [IO.Path]::GetFileName([string]$rendererProcess.ExecutablePath) -cin @('Discord.exe','DiscordPTB.exe','DiscordCanary.exe')) {
        continue
      }
      $identity = Get-TrustedRendererProcess ([int]$rendererProcessId) $OslProcessId
      $candidates += [pscustomobject]@{
        Hwnd = $rendererHwnd
        ProcessId = $identity.ProcessId
        RuntimeId = [string]::Join(',', @($element.GetRuntimeId()))
      }
    } catch [Windows.Automation.ElementNotAvailableException] {
      continue
    }
  }
  if ($candidates.Count -ne 1) { throw 'visible Discord renderer is unavailable or ambiguous' }
  $candidates[0]
}

function Get-FreshSurface([ValidateSet('Main', 'Overlay')][string]$Surface) {
  $identity = Get-ExactOslProcess
  $expectedTitle = if ($Surface -eq 'Main') { 'OSL Privacy' } else { 'OSL private composer' }
  $expectedClass = if ($Surface -eq 'Main') { 'Tauri Window' } else { $null }
  $windows = @()
  foreach ($hwnd in [OslVmDiscordUiaNative]::VisibleTopLevelWindowsFor([uint32]$identity.ProcessId)) {
    if ([OslVmDiscordUiaNative]::WindowText($hwnd) -cne $expectedTitle) { continue }
    if ($expectedClass -and [OslVmDiscordUiaNative]::WindowClass($hwnd) -cne $expectedClass) { continue }
    $windows += $hwnd
  }
  if ($windows.Count -ne 1) { throw "exact $Surface window is unavailable or ambiguous" }
  $hwnd = [IntPtr]$windows[0]
  if (-not [OslVmDiscordUiaNative]::IsWindow($hwnd) -or -not [OslVmDiscordUiaNative]::IsWindowVisible($hwnd)) {
    throw "exact $Surface window is not visible"
  }
  $root = [Windows.Automation.AutomationElement]::FromHandle($hwnd)
  if (-not $root -or $root.Current.IsOffscreen -or $root.Current.BoundingRectangle.IsEmpty) {
    throw "exact $Surface UIA root is not visible"
  }
  $renderer = Find-VisibleDiscordRenderer $hwnd $identity.ProcessId
  [pscustomobject]@{
    Surface = $Surface
    ProcessId = $identity.ProcessId
    ProcessStartTimeUtcTicks = $identity.StartTimeUtcTicks
    Hwnd = $hwnd
    Root = $root
    RootRuntimeId = [string]::Join(',', @($root.GetRuntimeId()))
    RendererHwnd = $renderer.Hwnd
    RendererProcessId = $renderer.ProcessId
    RendererRuntimeId = $renderer.RuntimeId
    Fingerprint = '{0}|{1}|{2}|{3}|{4}' -f $identity.ProcessId,$identity.StartTimeUtcTicks,$hwnd.ToInt64(),$renderer.Hwnd.ToInt64(),$renderer.RuntimeId
  }
}

function Get-FreshMainSurface { Get-FreshSurface 'Main' }
function Get-FreshOverlaySurface { Get-FreshSurface 'Overlay' }

function Get-ControlMatches(
  [Windows.Automation.AutomationElement]$Root,
  [Windows.Automation.ControlType[]]$ControlTypes,
  [string[]]$Names,
  [string]$AutomationId = ''
) {
  $matches = @()
  foreach ($type in $ControlTypes) {
    $conditions = @(
      New-Object Windows.Automation.PropertyCondition(
        [Windows.Automation.AutomationElement]::ControlTypeProperty, $type)
    )
    if ($AutomationId) {
      $conditions += New-Object Windows.Automation.PropertyCondition(
        [Windows.Automation.AutomationElement]::AutomationIdProperty, $AutomationId)
    }
    $condition = if ($conditions.Count -eq 1) {
      $conditions[0]
    } else {
      [Windows.Automation.AndCondition]::new([Windows.Automation.Condition[]]$conditions)
    }
    foreach ($candidate in @($Root.FindAll([Windows.Automation.TreeScope]::Descendants, $condition))) {
      if ($Names.Count -gt 0 -and [string]$candidate.Current.Name -cnotin $Names) { continue }
      if ($candidate.Current.IsOffscreen -or $candidate.Current.BoundingRectangle.IsEmpty) { continue }
      $matches += $candidate
    }
  }
  $matches
}

function Resolve-FreshControl(
  [ValidateSet('Main', 'Overlay')][string]$Surface,
  [Windows.Automation.ControlType[]]$ControlTypes,
  [string[]]$Names,
  [string]$AutomationId = '',
  [bool]$RequireEnabled = $true
) {
  $context = Get-FreshSurface $Surface
  $matches = @(Get-ControlMatches $context.Root $ControlTypes $Names $AutomationId)
  if ($matches.Count -ne 1) { throw "exact $Surface control is unavailable or ambiguous" }
  if ($RequireEnabled -and -not $matches[0].Current.IsEnabled) { throw "exact $Surface control is disabled" }
  [pscustomobject]@{ Context = $context; Element = $matches[0] }
}

function Get-FreshControlCount(
  [ValidateSet('Main', 'Overlay')][string]$Surface,
  [Windows.Automation.ControlType[]]$ControlTypes,
  [string[]]$Names,
  [string]$AutomationId = ''
) {
  try {
    $context = Get-FreshSurface $Surface
    return @(Get-ControlMatches $context.Root $ControlTypes $Names $AutomationId).Count
  } catch {
    return 0
  }
}

function Invoke-WithElementNotAvailableRetry(
  [scriptblock]$Rediscover,
  [scriptblock]$Operation,
  [int]$MaxAttempts = 4
) {
  $AttemptLimit = [Math]::Min([Math]::Max($MaxAttempts, 1), 6)
  $last = $null
  for ($attempt = 1; $attempt -le $AttemptLimit; $attempt++) {
    if ([DateTime]::UtcNow -gt $script:HarnessDeadline) { throw 'harness deadline expired' }
    try {
      $fresh = & $Rediscover
      return & $Operation $fresh
    } catch [Windows.Automation.ElementNotAvailableException] {
      $last = $_
    } catch [System.Runtime.InteropServices.COMException] {
      $last = $_
    }
  }
  if ($last) { throw $last }
  throw 'bounded element replacement retry failed'
}

function Wait-SemanticPostcondition(
  [scriptblock]$Rediscover,
  [string]$Description,
  [int]$StableSamples = 2
) {
  $stable = 0
  $lastKey = $null
  while ([DateTime]::UtcNow -le $script:HarnessDeadline) {
    try {
      $result = & $Rediscover
      if ($result -and $result.Satisfied) {
        $key = [string]$result.Key
        $stable = if ($key -ceq $lastKey) { $stable + 1 } else { 1 }
        $lastKey = $key
        if ($stable -ge $StableSamples) { return $result }
      } else {
        $stable = 0
        $lastKey = $null
      }
    } catch [Windows.Automation.ElementNotAvailableException] {
      $stable = 0
      $lastKey = $null
    } catch [System.Runtime.InteropServices.COMException] {
      $stable = 0
      $lastKey = $null
    }
    Start-Sleep -Milliseconds 100
  }
  throw "semantic postcondition timed out: $Description"
}

function Assert-ForegroundAndFocus(
  [ValidateSet('Main', 'Overlay')][string]$Surface,
  [string]$AutomationId,
  [Windows.Automation.ControlType[]]$ControlTypes,
  [string[]]$Names
) {
  Invoke-WithElementNotAvailableRetry `
    { Resolve-FreshControl $Surface $ControlTypes $Names $AutomationId $true } `
    {
      param($fresh)
      [void][OslVmDiscordUiaNative]::SetForegroundWindow([IntPtr]$fresh.Context.Hwnd)
      $fresh.Element.SetFocus()
    } | Out-Null

  $verified = Wait-SemanticPostcondition {
    $fresh = Resolve-FreshControl $Surface $ControlTypes $Names $AutomationId $true
    $foreground = [OslVmDiscordUiaNative]::GetForegroundWindow()
    $focused = [Windows.Automation.AutomationElement]::FocusedElement
    $hasKeyboardFocus = $fresh.Element.Current.HasKeyboardFocus
    $sameFocusedElement = $focused -and (Test-SameRuntimeId $fresh.Element $focused)
    [pscustomobject]@{
      Satisfied = $foreground -eq [IntPtr]$fresh.Context.Hwnd -and $hasKeyboardFocus -and $sameFocusedElement
      Key = $fresh.Context.Fingerprint + '|' + $fresh.Element.GetRuntimeId().Length
    }
  } "foreground and keyboard focus for $Surface control" 2
  if (-not $verified.Satisfied) { throw 'foreground and keyboard focus were not verified' }
}

function Invoke-FreshControl(
  [ValidateSet('Main', 'Overlay')][string]$Surface,
  [Windows.Automation.ControlType[]]$ControlTypes,
  [string[]]$Names,
  [string]$AutomationId = ''
) {
  $invokedSemantically = Invoke-WithElementNotAvailableRetry `
    { Resolve-FreshControl $Surface $ControlTypes $Names $AutomationId $true } `
    {
      param($fresh)
      $pattern = $null
      if (-not $fresh.Element.TryGetCurrentPattern([Windows.Automation.InvokePattern]::Pattern, [ref]$pattern)) {
        return $false
      }
      ([Windows.Automation.InvokePattern]$pattern).Invoke()
      return $true
    }
  if ($invokedSemantically) { return }

  Assert-ForegroundAndFocus $Surface $AutomationId $ControlTypes $Names
  Invoke-WithElementNotAvailableRetry `
    { Resolve-FreshControl $Surface $ControlTypes $Names $AutomationId $true } `
    {
      param($fresh)
      # Some WebView2 builds expose a semantic Button without InvokePattern.
      # Activate only the already resolved exact element through its standard
      # keyboard action; never fall back to coordinates or broad key input.
      $fresh.Element.SetFocus()
      [Windows.Forms.SendKeys]::SendWait('{ENTER}')
    } | Out-Null
}

function Set-FreshValue(
  [ValidateSet('Main', 'Overlay')][string]$Surface,
  [Windows.Automation.ControlType[]]$ControlTypes,
  [string[]]$Names,
  [string]$AutomationId,
  [string]$Value
) {
  Assert-ForegroundAndFocus $Surface $AutomationId $ControlTypes $Names
  Invoke-WithElementNotAvailableRetry `
    { Resolve-FreshControl $Surface $ControlTypes $Names $AutomationId $true } `
    {
      param($fresh)
      $pattern = $fresh.Element.GetCurrentPattern([Windows.Automation.ValuePattern]::Pattern)
      $pattern.SetValue($Value)
    } | Out-Null
}

function Get-FreshValue(
  [ValidateSet('Main', 'Overlay')][string]$Surface,
  [Windows.Automation.ControlType[]]$ControlTypes,
  [string[]]$Names,
  [string]$AutomationId
) {
  $fresh = Resolve-FreshControl $Surface $ControlTypes $Names $AutomationId $false
  [string]$fresh.Element.GetCurrentPattern([Windows.Automation.ValuePattern]::Pattern).Current.Value
}

function New-DeterministicQaMessage {
  "OSL QA $CaseId`: encrypted relay proof"
}

function Get-MainState {
  $null = Get-FreshMainSurface
  [pscustomobject]@{
    SkipOnboarding = Get-FreshControlCount 'Main' @([Windows.Automation.ControlType]::Button) @() 'skip-onboarding'
    FinishSetup = Get-FreshControlCount 'Main' @([Windows.Automation.ControlType]::Button) @() 'skip-scrub-setup'
    Home = Get-FreshControlCount 'Main' @([Windows.Automation.ControlType]::Button) @() 'home-app-discord'
    DiscordTile = Get-FreshControlCount 'Main' @([Windows.Automation.ControlType]::Button) @() 'home-app-discord'
    Current = Get-FreshControlCount 'Main' @([Windows.Automation.ControlType]::Button) @() 'discord-existing-session'
    Protect = Get-FreshControlCount 'Main' @([Windows.Automation.ControlType]::Button) @() 'local-protected-toggle'
    Friend = Get-FreshControlCount 'Main' @([Windows.Automation.ControlType]::Button) @() 'native-protect-verified-peer'
    Forward = Get-FreshControlCount 'Main' @([Windows.Automation.ControlType]::Button) @() 'native-companion-focus'
  }
}

function Get-VisibleAutomationIdInventory {
  $main = Get-FreshMainSurface
  $allowedTypes = @(
    [Windows.Automation.ControlType]::Button,
    [Windows.Automation.ControlType]::Edit,
    [Windows.Automation.ControlType]::CheckBox,
    [Windows.Automation.ControlType]::RadioButton,
    [Windows.Automation.ControlType]::ComboBox,
    [Windows.Automation.ControlType]::TabItem,
    [Windows.Automation.ControlType]::ListItem
  )
  $condition = [Windows.Automation.OrCondition]::new([Windows.Automation.Condition[]]@(
    $allowedTypes | ForEach-Object {
      [Windows.Automation.PropertyCondition]::new(
        [Windows.Automation.AutomationElement]::ControlTypeProperty, $_)
    }
  ))
  @($main.Root.FindAll([Windows.Automation.TreeScope]::Descendants, $condition) |
    Where-Object {
      -not $_.Current.IsOffscreen -and
      -not $_.Current.BoundingRectangle.IsEmpty -and
      -not [string]::IsNullOrWhiteSpace([string]$_.Current.AutomationId)
    } |
    Select-Object -First 64 |
    ForEach-Object {
      [pscustomobject]@{
        ControlType = $_.Current.ControlType.ProgrammaticName
        AutomationId = if ($_.Current.AutomationId.Length -le 96) {
          $_.Current.AutomationId
        } else {
          $_.Current.AutomationId.Substring(0, 96)
        }
        Enabled = [bool]$_.Current.IsEnabled
      }
    })
}

function Get-HostResult {
  $state = Get-MainState
  if ($state.Forward -eq 1) { return [pscustomobject]@{ Satisfied=$true; Key='hosted'; State='hosted'; Reason='none' } }
  $failures = [ordered]@{
    'OSL could not reopen Discord automatically. Try again'='existingSessionUnavailable'
    'OSL could not safely select the main Discord window'='existingSessionAmbiguous'
    'Discord could not open as a native OSL window'='nativeWindowRejected'
    'Discord opened but could not be shown safely'='presentationRejected'
    'Install Discord first'='appNotInstalled'
  }
  foreach ($entry in $failures.GetEnumerator()) {
    $count = Get-FreshControlCount 'Main' @([Windows.Automation.ControlType]::Text) @([string]$entry.Key)
    if ($count -gt 0) { return [pscustomobject]@{ Satisfied=$true; Key='failed|' + $entry.Value; State='failed'; Reason=$entry.Value } }
  }
  [pscustomobject]@{ Satisfied=$false; Key='waiting'; State='waiting'; Reason='none' }
}

function Get-OverlayReadyState {
  try {
    $surface = Get-FreshOverlaySurface
    $draft = Get-FreshControlCount 'Overlay' @([Windows.Automation.ControlType]::Edit) @() 'protected-draft'
    $send = Get-FreshControlCount 'Overlay' @([Windows.Automation.ControlType]::Button) @() 'prepare-protected'
    [pscustomobject]@{ Satisfied=($draft -eq 1 -and $send -eq 1); Key=$surface.Fingerprint; Fingerprint=$surface.Fingerprint }
  } catch {
    [pscustomobject]@{ Satisfied=$false; Key='waiting'; Fingerprint='' }
  }
}

function Get-ExactPlaintextSnapshot {
  $expected = New-DeterministicQaMessage
  $surface = Get-FreshOverlaySurface
  $list = Resolve-FreshControl 'Overlay' @([Windows.Automation.ControlType]::List) @('Messages prepared or opened in this OSL panel') 'osl-message-list' $false
  $condition = New-Object Windows.Automation.PropertyCondition(
    [Windows.Automation.AutomationElement]::ControlTypeProperty,
    [Windows.Automation.ControlType]::Text
  )
  $exact = 0
  $received = 0
  foreach ($item in @($list.Element.FindAll([Windows.Automation.TreeScope]::Descendants, $condition))) {
    if ($item.Current.IsOffscreen -or $item.Current.BoundingRectangle.IsEmpty) { continue }
    $name = [string]$item.Current.Name
    if ($name -ceq $expected) { $exact++ }
    if ($name -cin @('Received · opened','Received · opened once','Received by OSL')) { $received++ }
  }
  [pscustomobject]@{
    Fingerprint = $surface.Fingerprint
    ExactPlaintextCount = $exact
    ReceivedReceiptCount = $received
    Utf8Bytes = [Text.Encoding]::UTF8.GetByteCount($expected)
    Lines = ([regex]::Matches($expected, "`n")).Count + 1
  }
}

function Get-DecryptVisibilitySnapshot {
  $fresh = Resolve-FreshControl 'Overlay' @([Windows.Automation.ControlType]::CheckBox) @() 'protected-decrypt-display' $false
  $state = $fresh.Element.GetCurrentPattern([Windows.Automation.TogglePattern]::Pattern).Current.ToggleState
  [pscustomobject]@{
    Fingerprint = $fresh.Context.Fingerprint
    State = if ($state -eq [Windows.Automation.ToggleState]::On) { 'On' } else { 'Off' }
    Enabled = [bool]$fresh.Element.Current.IsEnabled
  }
}

function Set-DecryptVisibility([ValidateSet('On','Off')][string]$Requested) {
  Assert-ForegroundAndFocus 'Overlay' 'protected-decrypt-display' @([Windows.Automation.ControlType]::CheckBox) @()
  Invoke-WithElementNotAvailableRetry `
    { Resolve-FreshControl 'Overlay' @([Windows.Automation.ControlType]::CheckBox) @() 'protected-decrypt-display' $true } `
    {
      param($fresh)
      $pattern = $fresh.Element.GetCurrentPattern([Windows.Automation.TogglePattern]::Pattern)
      $current = if ($pattern.Current.ToggleState -eq [Windows.Automation.ToggleState]::On) { 'On' } else { 'Off' }
      if ($current -cne $Requested) { $pattern.Toggle() }
    } | Out-Null
}

function Get-SendPostcondition {
  $surface = Get-FreshOverlaySurface
  $status = Resolve-FreshControl 'Overlay' @([Windows.Automation.ControlType]::Text) @() 'overlay-status' $false
  $name = [string]$status.Element.Current.Name
  $classification = switch ($name) {
    'Sent privately through OSL only. No Discord marker was attempted.' { 'sentOslOnly' }
    'Sent privately through OSL. Discord received only the private-message marker.' { 'sentDiscordMarked' }
    'Sent privately through OSL. Discord changed, so its marker was not sent.' { 'sentMarkerUnavailable' }
    'Protection stopped safely. Your draft is still here.' { 'failedClosed' }
    default { 'waiting' }
  }
  [pscustomobject]@{
    Satisfied = $classification -cne 'waiting'
    Key = $surface.Fingerprint + '|' + $classification
    Classification = $classification
  }
}

function Get-WindowLifecycleState([bool]$ExpectFullscreen) {
  $surface = Get-FreshMainSurface
  $bounds = $surface.Root.Current.BoundingRectangle
  $screen = [System.Windows.Forms.Screen]::FromHandle([IntPtr]$surface.Hwnd).Bounds
  $fullscreen = [Math]::Abs($bounds.Left - $screen.Left) -le 2 -and
    [Math]::Abs($bounds.Top - $screen.Top) -le 2 -and
    [Math]::Abs($bounds.Width - $screen.Width) -le 2 -and
    [Math]::Abs($bounds.Height - $screen.Height) -le 2
  [pscustomobject]@{
    Satisfied = $fullscreen -eq $ExpectFullscreen
    Key = $surface.Fingerprint + '|' + $fullscreen
    Fingerprint = $surface.Fingerprint
    Fullscreen = $fullscreen
    Left = [int]$bounds.Left
    Top = [int]$bounds.Top
    Width = [int]$bounds.Width
    Height = [int]$bounds.Height
  }
}

function Get-InitialDiscordHostIdentity {
  $osl = Get-FreshMainSurface
  $allowedNames = @('Discord.exe','DiscordPTB.exe','DiscordCanary.exe')
  $channelProcesses = @(Get-CimInstance Win32_Process | Where-Object {
    $_.ExecutablePath -and [int]$_.SessionId -eq $SessionId -and
    [IO.Path]::GetFileName([string]$_.ExecutablePath) -cin $allowedNames
  })
  if ($channelProcesses.Count -eq 0 -or $channelProcesses.Count -gt 32) {
    throw 'bounded Discord process inventory is unavailable'
  }
  $channelPids = [uint32[]]@($channelProcesses | ForEach-Object { [uint32]$_.ProcessId })
  $mainWindows = @([OslVmDiscordUiaNative]::WindowsFor($channelPids) | Where-Object {
    [OslVmDiscordUiaNative]::WindowClass($_) -ceq 'Chrome_WidgetWin_1' -and
    [OslVmDiscordUiaNative]::WindowText($_) -ceq 'Discord' -and
    [OslVmDiscordUiaNative]::IsWindowVisible($_)
  })
  if ($mainWindows.Count -ne 1) { throw 'exact visible Discord main HWND is unavailable or ambiguous' }
  $discordHwnd = [IntPtr]$mainWindows[0]
  [uint32]$windowPid = 0
  [void][OslVmDiscordUiaNative]::GetWindowThreadProcessId($discordHwnd, [ref]$windowPid)
  $owner = @($channelProcesses | Where-Object { [uint32]$_.ProcessId -eq $windowPid })
  if ($owner.Count -ne 1) { throw 'exact Discord window process is unavailable' }
  $expectedPath = [IO.Path]::GetFullPath([string]$owner[0].ExecutablePath)
  $processes = @($channelProcesses | Where-Object {
    [string]::Equals([IO.Path]::GetFullPath([string]$_.ExecutablePath), $expectedPath, [StringComparison]::OrdinalIgnoreCase)
  } | Sort-Object ProcessId)
  if ($processes.Count -eq 0 -or $processes.Count -gt 16) { throw 'exact Discord PID set exceeds its evidence bound' }
  $signature = Get-AuthenticodeSignature -LiteralPath $expectedPath
  if ($signature.Status -ne [System.Management.Automation.SignatureStatus]::Valid -or
      -not $signature.SignerCertificate -or $signature.SignerCertificate.Subject -notmatch '(?i)Discord') {
    throw 'Discord publisher identity is not trusted'
  }
  $processKeys = @($processes | ForEach-Object {
    $live = Get-Process -Id ([int]$_.ProcessId) -ErrorAction Stop
    '{0}:{1}' -f ([int]$_.ProcessId),$live.StartTime.ToUniversalTime().Ticks
  })
  [pscustomobject]@{
    OslProcessId = $osl.ProcessId
    OslProcessStartTimeUtcTicks = $osl.ProcessStartTimeUtcTicks
    OslHwnd = $osl.Hwnd.ToInt64()
    DiscordHwnd = $discordHwnd.ToInt64()
    DiscordWindowProcessId = [int]$windowPid
    DiscordExecutablePath = $expectedPath
    DiscordProcessKeys = $processKeys
    DiscordProcessIds = @($processes | ForEach-Object { [int]$_.ProcessId })
    GuardianProcessIds = @(Get-ExactOslProcess).GuardianProcessIds
  }
}

function Get-DiscordContinuityState([object]$Baseline, [bool]$ExpectMinimized = $false) {
  $oslIdentity = Get-ExactOslProcess
  $oslHwnd = [IntPtr]$Baseline.OslHwnd
  $discordHwnd = [IntPtr]$Baseline.DiscordHwnd
  $current = @(Get-CimInstance Win32_Process | Where-Object {
    $_.ExecutablePath -and [int]$_.SessionId -eq $SessionId -and
    [string]::Equals(
      [IO.Path]::GetFullPath([string]$_.ExecutablePath),
      [string]$Baseline.DiscordExecutablePath,
      [StringComparison]::OrdinalIgnoreCase
    )
  } | Sort-Object ProcessId)
  $keys = @($current | ForEach-Object {
    try {
      $live = Get-Process -Id ([int]$_.ProcessId) -ErrorAction Stop
      '{0}:{1}' -f ([int]$_.ProcessId),$live.StartTime.ToUniversalTime().Ticks
    } catch { 'changed' }
  })
  $pidSetStable = [string]::Join('|', $keys) -ceq [string]::Join('|', @($Baseline.DiscordProcessKeys))
  $windowStable = [OslVmDiscordUiaNative]::IsWindow($discordHwnd) -and
    [OslVmDiscordUiaNative]::WindowClass($discordHwnd) -ceq 'Chrome_WidgetWin_1' -and
    [OslVmDiscordUiaNative]::WindowText($discordHwnd) -ceq 'Discord'
  [uint32]$windowPid = 0
  if ($windowStable) { [void][OslVmDiscordUiaNative]::GetWindowThreadProcessId($discordHwnd, [ref]$windowPid) }
  $mainStable = $oslIdentity.ProcessId -eq $Baseline.OslProcessId -and
    $oslIdentity.StartTimeUtcTicks -eq $Baseline.OslProcessStartTimeUtcTicks -and
    [OslVmDiscordUiaNative]::IsWindow($oslHwnd)
  $minimized = [OslVmDiscordUiaNative]::IsIconic($oslHwnd)
  $visible = [OslVmDiscordUiaNative]::IsWindowVisible($discordHwnd)
  $parentStable = [OslVmDiscordUiaNative]::GetParent($discordHwnd) -eq $oslHwnd
  $geometryRetethered = $parentStable -and [OslVmDiscordUiaNative]::IsRetethered($oslHwnd, $discordHwnd, 98, 2)
  $aboveBackground = $false
  if (-not $ExpectMinimized -and $mainStable -and $windowStable -and $visible) {
    try {
      $fresh = Get-FreshMainSurface
      $aboveBackground = [OslVmDiscordUiaNative]::IsBranchAbove($oslHwnd, $discordHwnd, [IntPtr]$fresh.RendererHwnd)
    } catch { $aboveBackground = $false }
  }
  $presentationOk = if ($ExpectMinimized) { $minimized } else { -not $minimized -and $visible -and $parentStable -and $geometryRetethered -and $aboveBackground }
  [pscustomobject]@{
    Satisfied = $mainStable -and $pidSetStable -and $windowStable -and
      [int]$windowPid -eq $Baseline.DiscordWindowProcessId -and $presentationOk
    Key = '{0}|{1}|{2}|{3}|{4}' -f $pidSetStable,$windowStable,$minimized,$visible,$aboveBackground
    PidSetStable = $pidSetStable
    HwndStable = $windowStable
    Visible = $visible
    Minimized = $minimized
    ParentStable = $parentStable
    GeometryRetethered = $geometryRetethered
    StackedAboveOslBackground = $aboveBackground
  }
}

function Wait-DiscordContinuity([object]$Baseline, [string]$Description, [bool]$ExpectMinimized = $false) {
  Wait-SemanticPostcondition { Get-DiscordContinuityState $Baseline $ExpectMinimized } $Description 2
}

function Send-SessionRecoverySequence([IntPtr]$Hwnd) {
  [uint32]$WM_ACTIVATE = 0x0006
  [uint32]$WM_WTSSESSION_CHANGE = 0x02B1
  [int]$WTS_REMOTE_CONNECT = 0x3
  [int]$WTS_REMOTE_DISCONNECT = 0x4
  [void][OslVmDiscordUiaNative]::PostMessage($Hwnd, $WM_ACTIVATE, [IntPtr]::Zero, [IntPtr]::Zero)
  [void][OslVmDiscordUiaNative]::PostMessage($Hwnd, $WM_WTSSESSION_CHANGE, [IntPtr]$WTS_REMOTE_DISCONNECT, [IntPtr]$SessionId)
  Start-Sleep -Milliseconds 150
  [void][OslVmDiscordUiaNative]::PostMessage($Hwnd, $WM_WTSSESSION_CHANGE, [IntPtr]$WTS_REMOTE_CONNECT, [IntPtr]$SessionId)
  [void][OslVmDiscordUiaNative]::PostMessage($Hwnd, $WM_ACTIVATE, [IntPtr]1, [IntPtr]::Zero)
  [void][OslVmDiscordUiaNative]::SetForegroundWindow($Hwnd)
}

function Invoke-FullscreenFresh {
  # Invoke the real OSL Tauri fullscreen path through its route-independent
  # semantic titlebar control. This remains available while native controls
  # are blocked for a hosted app so transitions can realign the harness.
  Invoke-FreshControl 'Main' @([Windows.Automation.ControlType]::Button) @() 'window-fullscreen'
}

function Exit-FullscreenFresh {
  # In true fullscreen the custom titlebar is intentionally offscreen. Verify
  # the exact signed OSL HWND is foreground, then use the same F11 handler a
  # person uses to leave fullscreen.
  Invoke-WithElementNotAvailableRetry `
    { Get-FreshMainSurface } `
    {
      param($fresh)
      [void][OslVmDiscordUiaNative]::SetForegroundWindow([IntPtr]$fresh.Hwnd)
    } | Out-Null
  $null = Wait-SemanticPostcondition {
    $fresh = Get-FreshMainSurface
    $foreground = [OslVmDiscordUiaNative]::GetForegroundWindow()
    [pscustomobject]@{
      Satisfied = $foreground -eq [IntPtr]$fresh.Hwnd
      Key = $fresh.Fingerprint + '|' + $foreground
    }
  } 'exact fullscreen OSL foreground' 2
  [OslVmDiscordUiaNative]::PressF11Keyboard()
}

function Invoke-WindowTransformFresh {
  Invoke-WithElementNotAvailableRetry `
    { Get-FreshMainSurface } `
    {
      param($fresh)
      $window = $fresh.Root.GetCurrentPattern([Windows.Automation.WindowPattern]::Pattern)
      $window.SetWindowVisualState([Windows.Automation.WindowVisualState]::Normal)
    } | Out-Null
  $normal = Wait-SemanticPostcondition {
    $fresh = Get-FreshMainSurface
    $window = $fresh.Root.GetCurrentPattern([Windows.Automation.WindowPattern]::Pattern)
    [pscustomobject]@{ Satisfied=($window.Current.WindowVisualState -eq [Windows.Automation.WindowVisualState]::Normal); Key=$fresh.Fingerprint }
  } 'normal window state' 2
  if (-not $normal.Satisfied) { throw 'normal window state was not verified' }

  Invoke-WithElementNotAvailableRetry `
    { Get-FreshMainSurface } `
    {
      param($fresh)
      $bounds = $fresh.Root.Current.BoundingRectangle
      $transform = $fresh.Root.GetCurrentPattern([Windows.Automation.TransformPattern]::Pattern)
      if (-not $transform.Current.CanMove -or -not $transform.Current.CanResize) { throw 'window transform is unavailable' }
      $width = [Math]::Max(800, [Math]::Min(1200, $bounds.Width - 80))
      $height = [Math]::Max(600, [Math]::Min(900, $bounds.Height - 60))
      $transform.Resize($width, $height)
      $transform.Move([Math]::Max(0, $bounds.Left + 24), [Math]::Max(0, $bounds.Top + 24))
    } | Out-Null
}

try {
  $result = switch ($Action) {
    'Inventory' {
      $main = Get-FreshMainSurface
      $buttons = @($main.Root.FindAll(
        [Windows.Automation.TreeScope]::Descendants,
        [Windows.Automation.PropertyCondition]::new(
          [Windows.Automation.AutomationElement]::ControlTypeProperty,
          [Windows.Automation.ControlType]::Button
        )
      ) | Where-Object {
        -not $_.Current.IsOffscreen -and -not $_.Current.BoundingRectangle.IsEmpty
      } | Select-Object -First 48 | ForEach-Object {
        [pscustomobject]@{
          Name = if ($_.Current.Name.Length -le 80) { $_.Current.Name } else { $_.Current.Name.Substring(0, 80) }
          AutomationId = if ($_.Current.AutomationId.Length -le 80) { $_.Current.AutomationId } else { $_.Current.AutomationId.Substring(0, 80) }
          Enabled = [bool]$_.Current.IsEnabled
        }
      })
      [pscustomobject]@{
        Ok = $true
        Action = 'Inventory'
        MainFingerprint = $main.Fingerprint
        VisibleButtonCount = $buttons.Count
        Buttons = $buttons
      }
    }

    'Inspect' {
      $main = Get-FreshMainSurface
      $state = Get-MainState
      $automationIds = @(Get-VisibleAutomationIdInventory)
      $overlay = try { (Get-FreshOverlaySurface).Fingerprint } catch { '' }
      [pscustomobject]@{
        Ok = $true; Action = 'Inspect'; MainFingerprint = $main.Fingerprint;
        OverlayPresent = [bool]$overlay; OverlayFingerprint = $overlay; MainState = $state;
        VisibleAutomationIds = $automationIds
      }
    }

    'OpenCurrent' {
      $null = Get-FreshMainSurface
      $state = Get-MainState
      if ($state.SkipOnboarding -eq 1) {
        Invoke-FreshControl 'Main' @([Windows.Automation.ControlType]::Button) @() 'skip-onboarding'
        $null = Wait-SemanticPostcondition {
          $next = Get-MainState
          [pscustomobject]@{
            Satisfied=($next.FinishSetup -eq 1 -or $next.Home -eq 1 -or $next.DiscordTile -eq 1 -or $next.Current -eq 1 -or $next.Forward -eq 1)
            Key=($next | ConvertTo-Json -Compress)
          }
        } 'post-onboarding home or persisted account route' 2
        $state = Get-MainState
      }
      if ($state.FinishSetup -eq 1) {
        Invoke-FreshControl 'Main' @([Windows.Automation.ControlType]::Button) @() 'skip-scrub-setup'
        $null = Wait-SemanticPostcondition {
          $next = Get-MainState
          [pscustomobject]@{
            Satisfied=($next.Home -eq 1 -or $next.DiscordTile -eq 1 -or $next.Current -eq 1 -or $next.Forward -eq 1)
            Key=($next | ConvertTo-Json -Compress)
          }
        } 'completed setup home or persisted account route' 2
        $state = Get-MainState
      }
      if ($state.Forward -ne 1 -and $state.Current -ne 1) {
        if ($state.DiscordTile -ne 1) {
          throw 'home-navigation AutomationId is unavailable'
          $null = Wait-SemanticPostcondition {
            $next = Get-MainState
            [pscustomobject]@{ Satisfied=($next.DiscordTile -eq 1 -or $next.Current -eq 1 -or $next.Forward -eq 1); Key=($next | ConvertTo-Json -Compress) }
          } 'Discord tile or persisted account route' 2
        }
        $state = Get-MainState
        if ($state.DiscordTile -eq 1) {
          Invoke-FreshControl 'Main' @([Windows.Automation.ControlType]::Button) @() 'home-app-discord'
          $null = Wait-SemanticPostcondition {
            $next = Get-MainState
            [pscustomobject]@{ Satisfied=($next.Current -eq 1 -or $next.Forward -eq 1); Key=($next | ConvertTo-Json -Compress) }
          } 'current account route or persisted auto-open' 2
        }
      }
      $state = Get-MainState
      if ($state.Forward -ne 1) {
        Invoke-FreshControl 'Main' @([Windows.Automation.ControlType]::Button) @() 'discord-existing-session'
      }
      $hostResult = Wait-SemanticPostcondition { Get-HostResult } 'current Discord native host result' 2
      [pscustomobject]@{ Ok=($hostResult.State -eq 'hosted'); Action='OpenCurrent'; State=$hostResult.State; Reason=$hostResult.Reason }
    }

    'PrepareOverlay' {
      $null = Get-FreshMainSurface
      $ready = Get-OverlayReadyState
      if (-not $ready.Satisfied) {
        $state = Get-MainState
        if ($state.Protect -eq 1) {
          Invoke-FreshControl 'Main' @([Windows.Automation.ControlType]::Button) @() 'local-protected-toggle'
        }
      }
      $post = Wait-SemanticPostcondition {
        $overlayReady = Get-OverlayReadyState
        if ($overlayReady.Satisfied) { return $overlayReady }
        $main = Get-MainState
        [pscustomobject]@{ Satisfied=($main.Friend -eq 1); Key='friend|' + $main.Friend; Fingerprint='' }
      } 'verified friend picker or protected overlay' 2
      [pscustomobject]@{ Ok=$true; Action='PrepareOverlay'; OverlayReady=[bool]$post.Fingerprint; FriendPickerReady=(-not [bool]$post.Fingerprint) }
    }

    'OpenVerifiedFriend' {
      $null = Get-FreshMainSurface
      $ready = Get-OverlayReadyState
      if (-not $ready.Satisfied) {
        $state = Get-MainState
        if ($state.Friend -ne 1) {
          if ($state.Protect -ne 1) { throw 'Protect is unavailable' }
          Invoke-FreshControl 'Main' @([Windows.Automation.ControlType]::Button) @() 'local-protected-toggle'
          $null = Wait-SemanticPostcondition {
            $next = Get-MainState
            $overlay = Get-OverlayReadyState
            [pscustomobject]@{ Satisfied=($next.Friend -eq 1 -or $overlay.Satisfied); Key=$next.Friend.ToString() + '|' + $overlay.Key }
          } 'verified friend picker' 2
        }
        $ready = Get-OverlayReadyState
        if (-not $ready.Satisfied) {
          Invoke-FreshControl 'Main' @([Windows.Automation.ControlType]::Button) @() 'native-protect-verified-peer'
        }
      }
      $opened = Wait-SemanticPostcondition { Get-OverlayReadyState } 'verified friend protected overlay' 2
      [pscustomobject]@{ Ok=$true; Action='OpenVerifiedFriend'; OverlayFingerprint=$opened.Fingerprint }
    }

    'SetDeterministic' {
      $null = Get-FreshOverlaySurface
      $message = New-DeterministicQaMessage
      Set-FreshValue 'Overlay' @([Windows.Automation.ControlType]::Edit) @() 'protected-draft' $message
      $set = Wait-SemanticPostcondition {
        $freshValue = Get-FreshValue 'Overlay' @([Windows.Automation.ControlType]::Edit) @() 'protected-draft'
        $surface = Get-FreshOverlaySurface
        [pscustomobject]@{ Satisfied=($freshValue -ceq (New-DeterministicQaMessage)); Key=$surface.Fingerprint + '|' + $freshValue.Length }
      } 'deterministic protected draft' 2
      [pscustomobject]@{
        Ok=$set.Satisfied; Action='SetDeterministic'; Utf8Bytes=[Text.Encoding]::UTF8.GetByteCount($message);
        Lines=([regex]::Matches($message, "`n")).Count + 1
      }
    }

    'Send' {
      $null = Get-FreshOverlaySurface
      $expected = New-DeterministicQaMessage
      $value = Get-FreshValue 'Overlay' @([Windows.Automation.ControlType]::Edit) @() 'protected-draft'
      if ($value -cne $expected) { throw 'deterministic protected draft is not present' }
      Invoke-FreshControl 'Overlay' @([Windows.Automation.ControlType]::Button) @() 'prepare-protected'
      $sent = Wait-SemanticPostcondition { Get-SendPostcondition } 'protected send result' 2
      $protectedSendSucceeded = $sent.Classification -cin @('sentOslOnly','sentDiscordMarked','sentMarkerUnavailable')
      [pscustomobject]@{
        Ok=$protectedSendSucceeded
        Action='Send'
        Classification=$sent.Classification
        ProtectedSendSucceeded=$protectedSendSucceeded
        DiscordCarrierProven=($sent.Classification -ceq 'sentDiscordMarked')
        FullDiscordProofPassed=$false
      }
    }

    'InspectInbound' {
      $null = Get-FreshOverlaySurface
      $plain = Wait-SemanticPostcondition {
        $snapshot = Get-ExactPlaintextSnapshot
        [pscustomobject]@{
          Satisfied=($snapshot.ExactPlaintextCount -eq 1 -and $snapshot.ReceivedReceiptCount -ge 1)
          Key=$snapshot.Fingerprint + '|' + $snapshot.ExactPlaintextCount + '|' + $snapshot.ReceivedReceiptCount
          Snapshot=$snapshot
        }
      } 'exact inbound plaintext and receive receipt' 2
      [pscustomobject]@{
        Ok=$plain.Satisfied; Action='InspectInbound'; ExactPlaintextCount=$plain.Snapshot.ExactPlaintextCount;
        ReceivedReceiptCount=$plain.Snapshot.ReceivedReceiptCount; Utf8Bytes=$plain.Snapshot.Utf8Bytes; Lines=$plain.Snapshot.Lines
      }
    }

    'ToggleVisibility' {
      $null = Get-FreshOverlaySurface
      Set-DecryptVisibility $Visibility
      $saved = Wait-SemanticPostcondition {
        $visibilityState = Get-DecryptVisibilitySnapshot
        $plain = Get-ExactPlaintextSnapshot
        $contentMatches = if ($Visibility -eq 'On') { $plain.ExactPlaintextCount -eq 1 } else { $plain.ExactPlaintextCount -eq 0 }
        [pscustomobject]@{
          Satisfied=($visibilityState.State -ceq $Visibility -and $contentMatches)
          Key=$visibilityState.Fingerprint + '|' + $visibilityState.State + '|' + $plain.ExactPlaintextCount
          State=$visibilityState.State
          ExactPlaintextCount=$plain.ExactPlaintextCount
        }
      } 'decrypt display visibility and plaintext rendering' 2
      [pscustomobject]@{ Ok=$saved.Satisfied; Action='ToggleVisibility'; State=$saved.State; ExactPlaintextCount=$saved.ExactPlaintextCount }
    }

    'ExerciseWindowLifecycle' {
      $null = Get-FreshMainSurface
      $state = Get-MainState
      if ($state.Forward -ne 1) { throw 'Discord native route is not active' }
      $baseline = Get-InitialDiscordHostIdentity
      $evidence = @()
      $initial = Wait-DiscordContinuity $baseline 'initial exact Discord containment'
      $evidence += [pscustomobject]@{ Phase='initial'; Visible=$initial.Visible; Parent=$initial.ParentStable; Above=$initial.StackedAboveOslBackground }

      for ($focusAttempt = 1; $focusAttempt -le 3; $focusAttempt++) {
        Assert-ForegroundAndFocus 'Main' 'window-fullscreen' @([Windows.Automation.ControlType]::Button) @()
        $focused = Wait-DiscordContinuity $baseline "trusted header focus $focusAttempt"
        $evidence += [pscustomobject]@{ Phase="headerFocus$focusAttempt"; Visible=$focused.Visible; Parent=$focused.ParentStable; Above=$focused.StackedAboveOslBackground }
      }

      $beforeTransform = Get-WindowLifecycleState $false
      Invoke-WindowTransformFresh
      $transformed = Wait-SemanticPostcondition {
        $candidate = Get-WindowLifecycleState $false
        $changed = $candidate.Left -ne $beforeTransform.Left -or $candidate.Top -ne $beforeTransform.Top -or
          $candidate.Width -ne $beforeTransform.Width -or $candidate.Height -ne $beforeTransform.Height
        [pscustomobject]@{
          Satisfied=$candidate.Satisfied -and $changed
          Key=$candidate.Key + '|' + $candidate.Left + '|' + $candidate.Top + '|' + $candidate.Width + '|' + $candidate.Height
        }
      } 'moved and resized windowed surface' 2
      $afterTransform = Wait-DiscordContinuity $baseline 'Discord continuity after move and resize'
      $evidence += [pscustomobject]@{ Phase='moveResize'; Visible=$afterTransform.Visible; Parent=$afterTransform.ParentStable; Above=$afterTransform.StackedAboveOslBackground }

      $oslHwnd = [IntPtr]$baseline.OslHwnd
      if (-not [OslVmDiscordUiaNative]::ShowWindowAsync($oslHwnd, 6)) { throw 'exact OSL minimize was rejected' }
      $minimized = Wait-DiscordContinuity $baseline 'exact Discord identity while minimized' $true
      $evidence += [pscustomobject]@{ Phase='minimized'; Visible=$minimized.Visible; Parent=$minimized.ParentStable; Above=$false }
      if (-not [OslVmDiscordUiaNative]::ShowWindowAsync($oslHwnd, 9)) { throw 'exact OSL restore was rejected' }
      [void][OslVmDiscordUiaNative]::SetForegroundWindow($oslHwnd)
      $afterRestore = Wait-DiscordContinuity $baseline 'automatic Discord retether after restore'
      $evidence += [pscustomobject]@{ Phase='restored'; Visible=$afterRestore.Visible; Parent=$afterRestore.ParentStable; Above=$afterRestore.StackedAboveOslBackground }

      Invoke-FullscreenFresh
      $null = Wait-SemanticPostcondition { Get-WindowLifecycleState $true } 'fullscreen entry' 2
      $null = Wait-DiscordContinuity $baseline 'Discord continuity after trusted header fullscreen click'
      Exit-FullscreenFresh
      $null = Wait-SemanticPostcondition { Get-WindowLifecycleState $false } 'fullscreen exit' 2
      $afterFullscreen = Wait-DiscordContinuity $baseline 'Discord continuity after fullscreen restore'
      $evidence += [pscustomobject]@{ Phase='fullscreenRoundTrip'; Visible=$afterFullscreen.Visible; Parent=$afterFullscreen.ParentStable; Above=$afterFullscreen.StackedAboveOslBackground }

      Send-SessionRecoverySequence $oslHwnd
      $recovered = Wait-DiscordContinuity $baseline 'automatic Discord retether after session recovery sequence'
      $evidence += [pscustomobject]@{ Phase='sessionRecovery'; Visible=$recovered.Visible; Parent=$recovered.ParentStable; Above=$recovered.StackedAboveOslBackground }

      [pscustomobject]@{
        Ok=$recovered.Satisfied
        Action='ExerciseWindowLifecycle'
        OslProcessId=$baseline.OslProcessId
        OslHwnd=$baseline.OslHwnd
        GuardianProcessCount=@($baseline.GuardianProcessIds).Count
        DiscordProcessIds=@($baseline.DiscordProcessIds)
        DiscordHwnd=$baseline.DiscordHwnd
        ExactPidSetSurvived=$recovered.PidSetStable
        ExactHwndSurvived=$recovered.HwndStable
        AutoRetethered=($recovered.Visible -and $recovered.ParentStable -and $recovered.GeometryRetethered -and $recovered.StackedAboveOslBackground)
        ManualBringForwardInvoked=$false
        Evidence=@($evidence | Select-Object -First 10)
      }
    }
  }
  ConvertTo-SafeJson $result
} catch {
  $exception = $_.Exception
  $safeType = $exception.GetType().Name
  ConvertTo-SafeJson ([pscustomobject]@{
    Ok = $false
    Action = $Action
    Error = 'uia-harness-failed-closed'
    ExceptionType = $safeType
    Detail = if ($exception.Message.Length -le 160) { $exception.Message } else { $exception.Message.Substring(0, 160) }
  })
  exit 1
}
