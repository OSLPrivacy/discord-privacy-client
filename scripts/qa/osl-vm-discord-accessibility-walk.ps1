<#
Metadata-only Discord accessibility walk for task 4951 evidence.

This script intentionally reports no provider names, values, text, help text,
or automation IDs. It waits for the Discord tree to populate before declaring a
small tree empty, so a slow Electron/UIA provider is not misclassified.
#>
param(
  [ValidateRange(1, 120)]
  [int]$WaitSeconds = 15,

  [ValidateRange(1, 2000)]
  [int]$MinimumDescendants = 200,

  [ValidateRange(100, 5000)]
  [int]$SampleDelayMilliseconds = 1000
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type -AssemblyName Accessibility
Add-Type -ReferencedAssemblies Accessibility.dll -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;

public static class OslDiscordWalk4951Native {
  public delegate bool EnumWindowsProc(IntPtr hwnd, IntPtr state);

  [DllImport("kernel32.dll")]
  public static extern bool ProcessIdToSessionId(uint processId, out uint sessionId);

  [DllImport("user32.dll")]
  public static extern bool EnumWindows(EnumWindowsProc callback, IntPtr state);

  [DllImport("user32.dll")]
  public static extern bool IsWindowVisible(IntPtr hwnd);

  [DllImport("user32.dll")]
  public static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint processId);

  [DllImport("user32.dll", CharSet = CharSet.Unicode)]
  public static extern int GetClassName(IntPtr hwnd, StringBuilder value, int capacity);

  [DllImport("oleacc.dll")]
  private static extern int AccessibleObjectFromWindow(
    IntPtr hwnd,
    uint objectId,
    ref Guid interfaceId,
    [In, Out, MarshalAs(UnmanagedType.IUnknown)] ref object accessible);

  [DllImport("oleacc.dll")]
  private static extern int AccessibleChildren(
    Accessibility.IAccessible container,
    int childStart,
    int childCount,
    [Out, MarshalAs(UnmanagedType.LPArray, SizeParamIndex = 2)] object[] children,
    out int obtained);

  public static string WindowClass(IntPtr hwnd) {
    var value = new StringBuilder(256);
    GetClassName(hwnd, value, value.Capacity);
    return value.ToString();
  }

  public static IntPtr[] VisibleDiscordWindowsInSession(uint[] processIds, uint sessionId) {
    var expected = new HashSet<uint>(processIds);
    var values = new List<IntPtr>();
    EnumWindows((hwnd, ignored) => {
      uint processId;
      uint windowSessionId;
      GetWindowThreadProcessId(hwnd, out processId);
      if (!expected.Contains(processId)) return true;
      if (!ProcessIdToSessionId(processId, out windowSessionId) || windowSessionId != sessionId) return true;
      if (IsWindowVisible(hwnd) && WindowClass(hwnd) == "Chrome_WidgetWin_1") values.Add(hwnd);
      return true;
    }, IntPtr.Zero);
    return values.ToArray();
  }

  public static Accessibility.IAccessible GetAccessible(IntPtr hwnd) {
    var interfaceId = new Guid("618736e0-3c3d-11cf-810c-00aa00389b71");
    object accessible = null;
    var status = AccessibleObjectFromWindow(
      hwnd,
      unchecked((uint)-4),
      ref interfaceId,
      ref accessible);
    if (status < 0) return null;
    return accessible as Accessibility.IAccessible;
  }

  public static object[] GetChildren(Accessibility.IAccessible accessible) {
    if (accessible == null) return new object[0];
    int count = 0;
    try { count = accessible.accChildCount; } catch { return new object[0]; }
    if (count <= 0) return new object[0];
    var children = new object[count];
    int obtained;
    var status = AccessibleChildren(accessible, 0, count, children, out obtained);
    if (status < 0 || obtained <= 0) return new object[0];
    if (obtained == children.Length) return children;
    var result = new object[obtained];
    Array.Copy(children, result, obtained);
    return result;
  }

  private static void CountAccessibleNode(
    Accessibility.IAccessible accessible,
    object childId,
    int depth,
    ref int count,
    System.Diagnostics.Stopwatch timer) {
    if (accessible == null || depth > 48 || count >= 4096 || timer.ElapsedMilliseconds > 1000) return;
    count++;
    int simpleChildId = 0;
    try { simpleChildId = Convert.ToInt32(childId); } catch { return; }
    if (simpleChildId != 0) return;
    foreach (var child in GetChildren(accessible)) {
      if (count >= 4096 || timer.ElapsedMilliseconds > 1000) break;
      var childAccessible = child as Accessibility.IAccessible;
      if (childAccessible != null) {
        CountAccessibleNode(childAccessible, 0, depth + 1, ref count, timer);
      } else {
        CountAccessibleNode(accessible, child, depth + 1, ref count, timer);
      }
    }
  }

  public static int CountMsaaTree(IntPtr hwnd) {
    var accessible = GetAccessible(hwnd);
    if (accessible == null) return 0;
    var count = 0;
    var timer = System.Diagnostics.Stopwatch.StartNew();
    CountAccessibleNode(accessible, 0, 0, ref count, timer);
    return count;
  }
}
'@

function Get-CurrentSessionId {
  [uint32]$sessionId = 0
  if (-not [OslDiscordWalk4951Native]::ProcessIdToSessionId([uint32]$PID, [ref]$sessionId)) {
    throw 'current Windows session is unavailable'
  }
  [int]$sessionId
}

function Get-DiscordProcessIdsInCurrentSession([int]$SessionId) {
  @(
    Get-Process -Name Discord,DiscordPTB,DiscordCanary -ErrorAction SilentlyContinue |
      Where-Object {
        [uint32]$processSessionId = 0
        [OslDiscordWalk4951Native]::ProcessIdToSessionId([uint32]$_.Id, [ref]$processSessionId) -and
          [int]$processSessionId -eq $SessionId
      } |
      ForEach-Object { [uint32]$_.Id }
  )
}

function Get-RawUiaSnapshot([Windows.Automation.AutomationElement]$Root) {
  $walker = [Windows.Automation.TreeWalker]::RawViewWalker
  $stack = [Collections.Generic.Stack[Windows.Automation.AutomationElement]]::new()
  $types = [ordered]@{}
  $timer = [Diagnostics.Stopwatch]::StartNew()
  $count = 0
  try {
    $first = $walker.GetFirstChild($Root)
    if ($null -ne $first) { $stack.Push($first) }
    while ($stack.Count -gt 0 -and $count -lt 4096 -and $timer.ElapsedMilliseconds -lt 1000) {
      $element = $stack.Pop()
      $count++
      $type = [string]$element.Current.ControlType
      if (-not $types.Contains($type)) { $types[$type] = 0 }
      $types[$type]++

      $sibling = $walker.GetNextSibling($element)
      if ($null -ne $sibling) { $stack.Push($sibling) }
      $child = $walker.GetFirstChild($element)
      if ($null -ne $child) { $stack.Push($child) }
    }
  } catch [Windows.Automation.ElementNotAvailableException] {
  } catch [System.Runtime.InteropServices.COMException] {
  }
  [pscustomobject]@{
    Count = $count
    ControlTypeCounts = $types
  }
}

$sessionId = Get-CurrentSessionId
Write-Output "VM-4951-SESSION $sessionId"

$discordFolder = Join-Path $env:LOCALAPPDATA 'Discord'
$discordFolderExists = Test-Path -LiteralPath $discordFolder -PathType Container
$deadline = [DateTime]::UtcNow.AddSeconds($WaitSeconds)
$observations = @()
$windows = @()
$treeFilled = $false
$returnedElementCount = 0

while ([DateTime]::UtcNow -le $deadline) {
  $processIds = @(Get-DiscordProcessIdsInCurrentSession $sessionId)
  if ($processIds.Count -gt 0) {
    $windows = @([OslDiscordWalk4951Native]::VisibleDiscordWindowsInSession($processIds, [uint32]$sessionId))
  } else {
    $windows = @()
  }

  if ($windows.Count -eq 1) {
    $root = [Windows.Automation.AutomationElement]::FromHandle([IntPtr]$windows[0])
    if ($null -ne $root) {
      $nodes = @($root.FindAll(
        [Windows.Automation.TreeScope]::Descendants,
        [Windows.Automation.Condition]::TrueCondition
      ))
      $msaaCount = [OslDiscordWalk4951Native]::CountMsaaTree([IntPtr]$windows[0])
      $raw = Get-RawUiaSnapshot $root
      $elementCount = [int]$nodes.Count + [int]$raw.Count + [int]$msaaCount
      $returnedElementCount += $elementCount
      foreach ($node in $nodes) {
        $type = [string]$node.Current.ControlType
        if (-not $raw.ControlTypeCounts.Contains($type)) { $raw.ControlTypeCounts[$type] = 0 }
        $raw.ControlTypeCounts[$type]++
      }
      $observations += [ordered]@{
        DescendantCount = $elementCount
        UiaDescendantCount = $nodes.Count
        RawUiaDescendantCount = $raw.Count
        MsaaNodeCount = $msaaCount
        CountMode = 'uia-control-plus-uia-raw-plus-msaa'
        ControlTypeCounts = $raw.ControlTypeCounts
      }
      if ($elementCount -ge $MinimumDescendants -or $returnedElementCount -ge $MinimumDescendants) {
        $treeFilled = $true
        break
      }
    }
  } else {
    $observations += [ordered]@{
      DescendantCount = 0
      ControlTypeCounts = [ordered]@{}
      WindowCount = $windows.Count
    }
  }

  if ([DateTime]::UtcNow.AddMilliseconds($SampleDelayMilliseconds) -gt $deadline) { break }
  Start-Sleep -Milliseconds $SampleDelayMilliseconds
}

if ($observations.Count -eq 0) {
  $observations += [ordered]@{
    DescendantCount = 0
    ControlTypeCounts = [ordered]@{}
    WindowCount = $windows.Count
  }
}

$maximum = @($observations | ForEach-Object { [int]$_.DescendantCount } | Measure-Object -Maximum).Maximum
[ordered]@{
  Schema = 'discord-uia-session-walk-4951/v1'
  SessionId = $sessionId
  CurrentUser = [Security.Principal.WindowsIdentity]::GetCurrent().Name
  LocalAppDataDiscordFolder = $discordFolder
  LocalAppDataDiscordFolderExists = [bool]$discordFolderExists
  WaitSeconds = $WaitSeconds
  MinimumDescendants = $MinimumDescendants
  Samples = $observations
  ReturnedElementCount = [int]$returnedElementCount
  MaximumDescendantCount = [int]$maximum
  TreeFilled = [bool]$treeFilled
  Verdict = if ($returnedElementCount -ge $MinimumDescendants) { 'populated' } else { 'emptyOrNotReady' }
  NamesReturned = $false
  ValuesReturned = $false
  TextReturned = $false
  InputSent = $false
} | ConvertTo-Json -Depth 6 -Compress
