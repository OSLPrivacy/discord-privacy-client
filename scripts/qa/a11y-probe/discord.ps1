<#
Metadata-only measurement of Discord's native UI Automation tree.

Run this from the logged-in desktop session (for example, through a scheduled
task armed by VM orchestration).  It neither reads UIA names/values/text nor
sends input.  A populated result means only that native UIA has a usable-sized
tree; selector work still needs separate evidence.
#>
param(
  [ValidateRange(1, 10)]
  [int]$Samples = 5,

  [ValidateRange(100, 5000)]
  [int]$SampleDelayMilliseconds = 1000
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes

$discord = @(Get-Process -Name Discord,DiscordPTB,DiscordCanary -ErrorAction SilentlyContinue |
  Where-Object { $_.MainWindowHandle -ne [IntPtr]::Zero })
if ($discord.Count -ne 1) { throw 'exact visible Discord process is unavailable or ambiguous' }

$root = [Windows.Automation.AutomationElement]::FromHandle($discord[0].MainWindowHandle)
if ($null -eq $root) { throw 'Discord UIA root is unavailable' }

$condition = [Windows.Automation.Condition]::TrueCondition
$observations = @()
for ($sample = 0; $sample -lt $Samples; $sample++) {
  $nodes = @($root.FindAll([Windows.Automation.TreeScope]::Descendants, $condition))
  # Retain only metadata: node count and control-type distribution.  Never emit
  # Name, Value, HelpText, AutomationId, or any provider-supplied content.
  $types = [ordered]@{}
  foreach ($node in $nodes) {
    $type = $node.Current.ControlType.ProgrammaticName
    if (-not $types.Contains($type)) { $types[$type] = 0 }
    $types[$type]++
  }
  $observations += [ordered]@{
    DescendantCount = $nodes.Count
    ControlTypeCounts = $types
  }
  if ($sample + 1 -lt $Samples) { Start-Sleep -Milliseconds $SampleDelayMilliseconds }
}

$maximum = @($observations | ForEach-Object { [int]$_.DescendantCount } | Measure-Object -Maximum).Maximum
[ordered]@{
  Schema = 'discord-uia-probe/v1'
  ProcessId = $discord[0].Id
  Samples = $observations
  MaximumDescendantCount = [int]$maximum
  Verdict = if ($maximum -gt 20) { 'populated' } else { 'emptyOrShellOnly' }
  NamesReturned = $false
  ValuesReturned = $false
  TextReturned = $false
  InputSent = $false
} | ConvertTo-Json -Depth 5 -Compress
