param([switch] $Summary)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName Accessibility
Add-Type -ReferencedAssemblies Accessibility.dll -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;

public static class OslDiscordMsaaProbeNative {
  [StructLayout(LayoutKind.Sequential)]
  public struct Point {
    public int X;
    public int Y;
  }
  [StructLayout(LayoutKind.Sequential)]
  public struct Rect {
    public int Left;
    public int Top;
    public int Right;
    public int Bottom;
  }

  public sealed class ProbeNode {
    public int Depth { get; set; }
    public int Role { get; set; }
    public bool IsMessagePlaceholder { get; set; }
    public bool IsDeckardConversation { get; set; }
    public string ValueClass { get; set; }
    public int ChildCount { get; set; }
    public string Bounds { get; set; }
  }

  public delegate bool EnumWindowsProc(IntPtr hwnd, IntPtr state);

  [DllImport("user32.dll")]
  public static extern bool EnumWindows(EnumWindowsProc callback, IntPtr state);

  [DllImport("user32.dll")]
  public static extern bool IsWindowVisible(IntPtr hwnd);

  [DllImport("user32.dll")]
  private static extern bool GetWindowRect(IntPtr hwnd, out Rect rect);

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

  [DllImport("oleacc.dll")]
  private static extern int AccessibleObjectFromPoint(
    Point point,
    [Out, MarshalAs(UnmanagedType.Interface)] out Accessibility.IAccessible accessible,
    [Out, MarshalAs(UnmanagedType.Struct)] out object childId);

  public static Accessibility.IAccessible GetAccessible(IntPtr hwnd, out int status) {
    var interfaceId = new Guid("618736e0-3c3d-11cf-810c-00aa00389b71");
    object accessible = null;
    status = AccessibleObjectFromWindow(
      hwnd,
      unchecked((uint)-4),
      ref interfaceId,
      ref accessible);
    return accessible as Accessibility.IAccessible;
  }

  public static object[] GetChildren(object accessibleObject) {
    var accessible = accessibleObject as Accessibility.IAccessible;
    if (accessible == null) return new object[0];
    var count = accessible.accChildCount;
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

  private static void ReadNode(
    Accessibility.IAccessible accessible,
    object childId,
    int depth,
    List<ProbeNode> result,
    System.Diagnostics.Stopwatch timer) {
    if (accessible == null || depth > 32 || result.Count >= 2048 ||
        timer.ElapsedMilliseconds > 2000) return;
    string name = null;
    string value = null;
    int role = 0;
    int childCount = 0;
    int left = 0;
    int top = 0;
    int width = 0;
    int height = 0;
    try { name = accessible.get_accName(childId); } catch {}
    try { value = accessible.get_accValue(childId); } catch {}
    try { role = Convert.ToInt32(accessible.get_accRole(childId)); } catch {}
    try { accessible.accLocation(out left, out top, out width, out height, childId); } catch {}
    if (Convert.ToInt32(childId) == 0) {
      try { childCount = accessible.accChildCount; } catch {}
    }
    result.Add(new ProbeNode {
      Depth = depth,
      Role = role,
      IsMessagePlaceholder = !String.IsNullOrWhiteSpace(name) &&
        name.TrimStart().StartsWith("Message @", StringComparison.Ordinal),
      IsDeckardConversation = !String.IsNullOrWhiteSpace(name) &&
        name.IndexOf("Deckard", StringComparison.OrdinalIgnoreCase) >= 0,
      ValueClass = String.IsNullOrEmpty(value) ? "empty" : "nonempty",
      ChildCount = childCount,
      Bounds = String.Format("{0},{1},{2},{3}", left, top, width, height)
    });
    if (Convert.ToInt32(childId) != 0 || childCount <= 0) return;
    foreach (var child in GetChildren(accessible)) {
      if (result.Count >= 2048 || timer.ElapsedMilliseconds > 2000) break;
      var childAccessible = child as Accessibility.IAccessible;
      if (childAccessible != null) {
        ReadNode(childAccessible, 0, depth + 1, result, timer);
      } else {
        int simpleChild;
        try { simpleChild = Convert.ToInt32(child); } catch { continue; }
        ReadNode(accessible, simpleChild, depth + 1, result, timer);
      }
    }
  }

  public static ProbeNode[] Probe(IntPtr hwnd) {
    int status;
    var accessible = GetAccessible(hwnd, out status);
    if (status < 0 || accessible == null) return new ProbeNode[0];
    var result = new List<ProbeNode>();
    var timer = System.Diagnostics.Stopwatch.StartNew();
    ReadNode(accessible, 0, 0, result, timer);
    return result.ToArray();
  }

  public static ProbeNode ProbePoint(int x, int y) {
    Accessibility.IAccessible accessible;
    object childId;
    var status = AccessibleObjectFromPoint(
      new Point { X = x, Y = y },
      out accessible,
      out childId);
    if (status < 0 || accessible == null || childId == null) return null;
    string name = null;
    string value = null;
    int role = 0;
    int left = 0;
    int top = 0;
    int width = 0;
    int height = 0;
    try { name = accessible.get_accName(childId); } catch {}
    try { value = accessible.get_accValue(childId); } catch {}
    try { role = Convert.ToInt32(accessible.get_accRole(childId)); } catch {}
    try { accessible.accLocation(out left, out top, out width, out height, childId); } catch {}
    return new ProbeNode {
      Depth = 0,
      Role = role,
      IsMessagePlaceholder = !String.IsNullOrWhiteSpace(name) &&
        name.TrimStart().StartsWith("Message @", StringComparison.Ordinal),
      IsDeckardConversation = !String.IsNullOrWhiteSpace(name) &&
        name.IndexOf("Deckard", StringComparison.OrdinalIgnoreCase) >= 0,
      ValueClass = String.IsNullOrEmpty(value) ? "empty" : "nonempty",
      ChildCount = 0,
      Bounds = String.Format("{0},{1},{2},{3}", left, top, width, height)
    };
  }

  public static ProbeNode[] ProbeComposerPoints(IntPtr hwnd) {
    Rect rect;
    if (!GetWindowRect(hwnd, out rect)) return new ProbeNode[0];
    var width = rect.Right - rect.Left;
    var height = rect.Bottom - rect.Top;
    if (width <= 0 || height <= 0) return new ProbeNode[0];
    var result = new List<ProbeNode>();
    foreach (var percent in new int[] { 45, 50, 55, 60 }) {
      var node = ProbePoint(
        rect.Left + width * percent / 100,
        rect.Bottom - Math.Min(37, height / 3));
      if (node != null) result.Add(node);
    }
    return result.ToArray();
  }

  public static string WindowClass(IntPtr hwnd) {
    var value = new StringBuilder(256);
    GetClassName(hwnd, value, value.Capacity);
    return value.ToString();
  }

  public static IntPtr[] VisibleDiscordWindows(uint[] processIds) {
    var expected = new HashSet<uint>(processIds);
    var values = new List<IntPtr>();
    EnumWindows((hwnd, ignored) => {
      uint processId;
      GetWindowThreadProcessId(hwnd, out processId);
      if (expected.Contains(processId) && IsWindowVisible(hwnd) &&
          WindowClass(hwnd) == "Chrome_WidgetWin_1") values.Add(hwnd);
      return true;
    }, IntPtr.Zero);
    return values.ToArray();
  }
}
'@

function Read-AccessibleNode {
  param(
    [Parameter(Mandatory)]
    [object] $Accessible,
    [int] $Depth = 0,
    [int] $Budget = 256
  )

  if ($Budget -le 0 -or $Depth -gt 12) { return @() }
  $nodes = @()
  $name = $null
  $value = $null
  $role = $null
  try { $name = [string]$Accessible.get_accName(0) } catch {}
  try { $value = [string]$Accessible.get_accValue(0) } catch {}
  try { $role = $Accessible.get_accRole(0) } catch {}
  $nodes += [pscustomobject]@{
    Depth = $Depth
    Name = $name
    Value = $value
    Role = $role
    Children = [int]$Accessible.accChildCount
  }

  $remaining = $Budget - 1
  $children = @([OslDiscordMsaaProbeNative]::GetChildren($Accessible))
  foreach ($child in $children) {
    if ($remaining -le 0) { break }
    if ($null -ne $child -and $child.GetType().IsCOMObject) {
      $nested = @(Read-AccessibleNode -Accessible $child -Depth ($Depth + 1) -Budget $remaining)
      $nodes += $nested
      $remaining -= $nested.Count
      continue
    }
    $childId = [int]$child
    $childName = $null
    $childValue = $null
    $childRole = $null
    try { $childName = [string]$Accessible.get_accName($childId) } catch {}
    try { $childValue = [string]$Accessible.get_accValue($childId) } catch {}
    try { $childRole = $Accessible.get_accRole($childId) } catch {}
    $nodes += [pscustomobject]@{
      Depth = $Depth + 1
      Name = $childName
      Value = $childValue
      Role = $childRole
      Children = 0
    }
    $remaining--
  }
  return $nodes
}

$processIds = @(
  Get-Process Discord -ErrorAction SilentlyContinue |
    ForEach-Object { [uint32]$_.Id }
)
$result = @()
foreach ($hwnd in @([OslDiscordMsaaProbeNative]::VisibleDiscordWindows($processIds))) {
  $status = 0
  $accessibleObject = [OslDiscordMsaaProbeNative]::GetAccessible($hwnd, [ref]$status)
  $nodes = @([OslDiscordMsaaProbeNative]::Probe($hwnd))
  $reportedNodes = if ($Summary) {
    @($nodes | Where-Object { $_.IsMessagePlaceholder -or $_.IsDeckardConversation })
  } else {
    $nodes
  }
  $pointNodes = @()
  if ($Summary) {
    $pointNodes = @([OslDiscordMsaaProbeNative]::ProbeComposerPoints($hwnd))
  }
  $result += [pscustomobject]@{
    Hwnd = [long]$hwnd
    Status = $status
    NodeCount = $nodes.Count
    Nodes = $reportedNodes
    PointNodes = $pointNodes
  }
}
$result | ConvertTo-Json -Depth 6 -Compress
