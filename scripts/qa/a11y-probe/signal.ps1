<#
Measure the accessibility tiers exposed by a logged-in Signal Desktop window.

The result is metadata-only: it reports booleans and counts, never accessible
names, values, automation IDs, message text, or identities.  Run it from the
active Windows desktop session after opening a conversation.  It sends no input
and does not attempt a pixel/OCR fallback (Signal deliberately blocks capture).
#>
[CmdletBinding()]
param(
  [ValidateRange(1, 10)] [int]$Samples = 3,
  [ValidateRange(100, 5000)] [int]$SampleDelayMilliseconds = 750,
  [switch]$SelfTest
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$ROLE_SYSTEM_TEXT = 42
$ROLE_SYSTEM_LISTITEM = 34

function New-ProbeNode {
  param(
    [Parameter(Mandatory)] [string]$Kind,
    [bool]$Composer = $false,
    [bool]$TranscriptRow = $false,
    [bool]$TextExposed = $false
  )
  [pscustomobject]@{
    Kind = $Kind
    Composer = $Composer
    TranscriptRow = $TranscriptRow
    TextExposed = $TextExposed
  }
}

function Get-TierReport {
  param(
    [Parameter(Mandatory)] [string]$Tier,
    [Parameter(Mandatory)] [object[]]$Snapshots
  )
  if ($Snapshots.Count -eq 0) { throw 'at least one snapshot is required' }

  $rows = @($Snapshots | ForEach-Object {
    $snapshot = if ($_ -is [array]) { $_ } else { @($_) }
    [pscustomobject]@{
      ComposerFound = @($snapshot | Where-Object Composer).Count -gt 0
      TranscriptRowCount = @($snapshot | Where-Object TranscriptRow).Count
      TextExposedRowCount = @($snapshot | Where-Object { $_.TranscriptRow -and $_.TextExposed }).Count
    }
  })

  # The minimum describes what survived every sample, so a transient virtualized
  # row cannot overstate a provider's usable evidence.
  [pscustomobject]@{
    Tier = $Tier
    ComposerFound = @($rows | Where-Object ComposerFound).Count -gt 0
    TranscriptRowCount = [int](@($rows | ForEach-Object TranscriptRowCount | Measure-Object -Minimum).Minimum)
    TextExposedRowCount = [int](@($rows | ForEach-Object TextExposedRowCount | Measure-Object -Minimum).Minimum)
  }
}

function Select-BindingEvidenceTier {
  param([Parameter(Mandatory)] [object[]]$TierReports)
  # A composer plus at least one text-exposed transcript row is the minimum
  # evidence this probe can honestly call a usable a11y tier.  Destination
  # attestation remains a later, separate measurement.
  foreach ($report in $TierReports) {
    if ($report.ComposerFound -and $report.TranscriptRowCount -gt 0 -and $report.TextExposedRowCount -gt 0) {
      return $report.Tier
    }
  }
  return 'none'
}

function Get-SignalProcess {
  $windows = @(Get-Process -Name Signal -ErrorAction SilentlyContinue |
    Where-Object { $_.MainWindowHandle -ne [IntPtr]::Zero })
  if ($windows.Count -ne 1) { throw 'exactly one visible Signal Desktop window is required' }
  return $windows[0]
}

function Get-UiaSnapshot {
  param([Parameter(Mandatory)] [IntPtr]$WindowHandle)
  Add-Type -AssemblyName UIAutomationClient
  Add-Type -AssemblyName UIAutomationTypes
  $root = [Windows.Automation.AutomationElement]::FromHandle($WindowHandle)
  if ($null -eq $root) { throw 'Signal UIA root is unavailable' }
  $nodes = [System.Collections.Generic.List[object]]::new()
  $elements = $root.FindAll([Windows.Automation.TreeScope]::Descendants, [Windows.Automation.Condition]::TrueCondition)
  for ($index = 0; $index -lt $elements.Count; $index++) {
    $element = $elements[$index]
    try {
      $rect = $element.Current.BoundingRectangle
      $visible = -not $element.Current.IsOffscreen -and $rect.Width -gt 0 -and $rect.Height -gt 0
      $type = $element.Current.ControlType.ProgrammaticName
      $textExposed = -not [string]::IsNullOrWhiteSpace($element.Current.Name)
      $valuePattern = $null
      if (-not $textExposed -and $element.TryGetCurrentPattern([Windows.Automation.ValuePattern]::Pattern, [ref]$valuePattern)) {
        $textExposed = -not [string]::IsNullOrWhiteSpace(([Windows.Automation.ValuePattern]$valuePattern).Current.Value)
      }
      [void]$nodes.Add((New-ProbeNode -Kind $type -Composer ($visible -and $type -in 'ControlType.Edit', 'ControlType.Document') -TranscriptRow ($visible -and $type -eq 'ControlType.ListItem') -TextExposed $textExposed))
    } catch {
      # Individual provider failures are evidence of an incomplete tree, not a
      # reason to leak provider diagnostics or fail the entire other tier.
    }
  }
  return ,$nodes.ToArray()
}

function Initialize-MsaaInterop {
  if (-not ('SignalProbe.NativeMethods' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
namespace SignalProbe {
  public static class NativeMethods {
    [DllImport("oleacc.dll")]
    public static extern int AccessibleObjectFromWindow(
      IntPtr hwnd, uint objectId, ref Guid iid,
      [MarshalAs(UnmanagedType.Interface)] out object accessible);
  }
}
'@
  }
}

function Get-MsaaSnapshot {
  param([Parameter(Mandatory)] [IntPtr]$WindowHandle)
  Initialize-MsaaInterop
  $iid = [Guid]'618736e0-3c3d-11cf-810c-00aa00389b71'
  $accessible = $null
  $hr = [SignalProbe.NativeMethods]::AccessibleObjectFromWindow($WindowHandle, 0xFFFFFFFC, [ref]$iid, [ref]$accessible)
  if ($hr -ne 0 -or $null -eq $accessible) { throw 'Signal MSAA client root is unavailable' }

  $nodes = [System.Collections.Generic.List[object]]::new()
  function Visit-MsaaNode {
    param([Parameter(Mandatory)] $Node, [int]$Depth = 0)
    if ($Depth -gt 64) { return }
    try {
      $role = [int]$Node.get_accRole(0)
      $name = [string]$Node.get_accName(0)
      $value = [string]$Node.get_accValue(0)
      $textExposed = -not ([string]::IsNullOrWhiteSpace($name) -and [string]::IsNullOrWhiteSpace($value))
      [void]$nodes.Add((New-ProbeNode -Kind "Role.$role" -Composer ($role -eq $ROLE_SYSTEM_TEXT) -TranscriptRow ($role -eq $ROLE_SYSTEM_LISTITEM) -TextExposed $textExposed))
      $count = [int]$Node.accChildCount
      for ($index = 1; $index -le $count; $index++) {
        $child = $Node.get_accChild($index)
        if ($null -ne $child -and $child -isnot [int]) { Visit-MsaaNode -Node $child -Depth ($Depth + 1) }
      }
    } catch {
      # Electron's MSAA bridge can expose partial branches; retain counts from
      # the branches it does provide.
    }
  }
  Visit-MsaaNode -Node $accessible
  return ,$nodes.ToArray()
}

function Invoke-SignalProbe {
  $signal = Get-SignalProcess
  $uiaSnapshots = @()
  $msaaSnapshots = @()
  for ($sample = 0; $sample -lt $Samples; $sample++) {
    $uiaSnapshots += ,(Get-UiaSnapshot -WindowHandle $signal.MainWindowHandle)
    $msaaSnapshots += ,(Get-MsaaSnapshot -WindowHandle $signal.MainWindowHandle)
    if ($sample + 1 -lt $Samples) { Start-Sleep -Milliseconds $SampleDelayMilliseconds }
  }
  $tiers = @(
    Get-TierReport -Tier 'UIA' -Snapshots $uiaSnapshots
    Get-TierReport -Tier 'MSAA' -Snapshots $msaaSnapshots
  )
  [pscustomobject]@{
    Schema = 'signal-a11y-tier-probe/v1'
    Samples = $Samples
    Tiers = $tiers
    BindingEvidenceTier = Select-BindingEvidenceTier -TierReports $tiers
    NamesReturned = $false
    ValuesReturned = $false
    TextReturned = $false
    InputSent = $false
  }
}

function Invoke-SelfTest {
  $uiaSnapshots = @()
  $uiaSnapshots += ,@(New-ProbeNode -Kind 'ControlType.Edit' -Composer $true; New-ProbeNode -Kind 'ControlType.ListItem' -TranscriptRow $true -TextExposed $true)
  $uiaSnapshots += ,@(New-ProbeNode -Kind 'ControlType.Edit' -Composer $true; New-ProbeNode -Kind 'ControlType.ListItem' -TranscriptRow $true -TextExposed $true)
  $uia = Get-TierReport -Tier 'UIA' -Snapshots $uiaSnapshots
  $msaa = Get-TierReport -Tier 'MSAA' -Snapshots (,@(New-ProbeNode -Kind 'Role.42' -Composer $true))
  if ((Select-BindingEvidenceTier -TierReports @($uia, $msaa)) -ne 'UIA') { throw 'UIA transcript evidence must win the tier selection' }

  $uiaWithoutRows = Get-TierReport -Tier 'UIA' -Snapshots (,@(New-ProbeNode -Kind 'ControlType.Edit' -Composer $true))
  $msaaWithRows = Get-TierReport -Tier 'MSAA' -Snapshots (,@(New-ProbeNode -Kind 'Role.42' -Composer $true; New-ProbeNode -Kind 'Role.34' -TranscriptRow $true -TextExposed $true))
  if ((Select-BindingEvidenceTier -TierReports @($uiaWithoutRows, $msaaWithRows)) -ne 'MSAA') { throw 'MSAA must be selected when UIA lacks readable transcript rows' }
  'signal a11y tier probe self-test passed'
}

if ($SelfTest) {
  Invoke-SelfTest
  exit 0
}

try {
  Invoke-SignalProbe | ConvertTo-Json -Depth 5 -Compress
} catch {
  [pscustomobject]@{
    Schema = 'signal-a11y-tier-probe/v1'
    Verdict = 'inconclusive'
    Error = $_.Exception.Message
  } | ConvertTo-Json -Compress
  exit 1
}
