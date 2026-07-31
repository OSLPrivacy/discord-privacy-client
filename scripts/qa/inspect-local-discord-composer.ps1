$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;
public static class OslDiscordComposerProbeNative {
  public delegate bool EnumWindowsProc(IntPtr hwnd, IntPtr state);
  [DllImport("user32.dll")]
  public static extern bool EnumWindows(EnumWindowsProc callback, IntPtr state);
  [DllImport("user32.dll")]
  public static extern bool IsWindowVisible(IntPtr hwnd);
  [DllImport("user32.dll")]
  public static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint processId);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)]
  public static extern int GetClassName(IntPtr hwnd, StringBuilder value, int capacity);
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

$processIds = @(
  Get-Process Discord,DiscordPTB,DiscordCanary -ErrorAction SilentlyContinue |
    ForEach-Object { [uint32]$_.Id }
)
$windows = @([OslDiscordComposerProbeNative]::VisibleDiscordWindows($processIds))
$result = @()
foreach ($hwnd in $windows) {
  $root = [Windows.Automation.AutomationElement]::FromHandle($hwnd)
  $allDescendants = @($root.FindAll(
    [Windows.Automation.TreeScope]::Descendants,
    [Windows.Automation.Condition]::TrueCondition
  ))
  $controlTypes = @(
    $allDescendants |
      ForEach-Object { [string]$_.Current.ControlType.ProgrammaticName } |
      Group-Object |
      Sort-Object Count -Descending |
      Select-Object -First 16 Name, Count
  )
  $condition = [Windows.Automation.OrCondition]::new([Windows.Automation.Condition[]]@(
    [Windows.Automation.PropertyCondition]::new(
      [Windows.Automation.AutomationElement]::ControlTypeProperty,
      [Windows.Automation.ControlType]::Document
    ),
    [Windows.Automation.PropertyCondition]::new(
      [Windows.Automation.AutomationElement]::ControlTypeProperty,
      [Windows.Automation.ControlType]::Edit
    )
  ))
  $items = @()
  foreach ($element in @($root.FindAll([Windows.Automation.TreeScope]::Descendants, $condition))) {
    if ($element.Current.IsOffscreen -or $element.Current.BoundingRectangle.IsEmpty) { continue }
    $value = $null
    try {
      $pattern = $element.GetCurrentPattern([Windows.Automation.ValuePattern]::Pattern)
      $value = [string]$pattern.Current.Value
    } catch {}
    $bounds = $element.Current.BoundingRectangle
    $items += [pscustomobject]@{
      Type = [string]$element.Current.ControlType.ProgrammaticName
      Name = [string]$element.Current.Name
      AutomationId = [string]$element.Current.AutomationId
      Value = $value
      Focused = [bool]$element.Current.HasKeyboardFocus
      Bounds = "$([int]$bounds.Left),$([int]$bounds.Top),$([int]$bounds.Width),$([int]$bounds.Height)"
    }
  }
  $rootBounds = $root.Current.BoundingRectangle
  if (
    -not [double]::IsNaN($rootBounds.Left) -and
    -not [double]::IsNaN($rootBounds.Top) -and
    -not [double]::IsNaN($rootBounds.Width) -and
    -not [double]::IsNaN($rootBounds.Height) -and
    -not [double]::IsInfinity($rootBounds.Left) -and
    -not [double]::IsInfinity($rootBounds.Top) -and
    -not [double]::IsInfinity($rootBounds.Width) -and
    -not [double]::IsInfinity($rootBounds.Height) -and
    $rootBounds.Width -gt 0 -and
    $rootBounds.Height -gt 0
  ) {
    foreach ($xPercent in 40, 55, 70) {
      foreach ($yOffset in 18, 32, 46) {
        $x = [int]($rootBounds.Left + ($rootBounds.Width * $xPercent / 100))
        $y = [int]($rootBounds.Bottom - $yOffset)
        try {
          $nativeElement = [Windows.Automation.AutomationElement]::FromPoint(
            [System.Windows.Point]::new($x, $y)
          )
          $name = [string]$nativeElement.Current.Name
          $automationId = [string]$nativeElement.Current.AutomationId
          $controlType = [string]$nativeElement.Current.ControlType.ProgrammaticName
          $value = $null
          try {
            $pattern = $nativeElement.GetCurrentPattern([Windows.Automation.ValuePattern]::Pattern)
            $value = [string]$pattern.Current.Value
          } catch {}
          $nativeBounds = $nativeElement.Current.BoundingRectangle
          $items += [pscustomobject]@{
            Type = $controlType
            Name = $name
            AutomationId = $automationId
            Value = $value
            Focused = [bool]$nativeElement.Current.HasKeyboardFocus
            Bounds = "$([int]$nativeBounds.Left),$([int]$nativeBounds.Top),$([int]$nativeBounds.Width),$([int]$nativeBounds.Height)"
          }
        } catch {}
      }
    }
  }
  $result += [pscustomobject]@{
    Hwnd = [long]$hwnd
    RootName = [string]$root.Current.Name
    DescendantCount = $allDescendants.Count
    ControlTypes = $controlTypes
    Items = $items
  }
}
$result | ConvertTo-Json -Depth 5 -Compress
