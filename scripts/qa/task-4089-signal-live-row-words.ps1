<#
  TASK 4089 -- live, read-only Signal transcript row-words measurement.

  This mirrors the shipping Rust route in
  apps/osl-hub/src/native_signal_row_words.rs: it walks the live, already-open
  Signal window, finds every visible transcript `ListItem` row, and fills each
  row's words with exactly the TASK 4088 winning route,
  `AutomationElement.Current.Name`. It reads Name, ControlType, IsOffscreen
  and BoundingRectangle only. It never focuses, clicks, types, scrolls, or
  sends, and it persists no UI tree (SavedUiTrees is always zero).

  -StarveRoute simulates removing the shipping winning text route: every
  row's name is blanked before the same trim/drop mapping runs, so every row
  is starved of its words -- exactly what apps/osl-hub/src/
  native_signal_row_words.rs's `removing_the_winning_route_starves_every_
  row_of_its_words` test proves in Rust.
#>
[CmdletBinding()]
param(
  [string] $MarkerPrefix = '',
  [switch] $StarveRoute,
  [string] $OutputPath
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type -AssemblyName System.Drawing

function Get-ExactSignalProcess {
  $all = @(Get-Process -Name Signal -ErrorAction SilentlyContinue |
    Where-Object { $_.MainWindowHandle -ne [IntPtr]::Zero -and $_.MainWindowTitle -ceq 'Signal' })
  if ($all.Count -ne 1) { throw "exact visible Signal window count=$($all.Count)" }
  return $all[0]
}

function Get-Visible([Windows.Automation.AutomationElement] $Element) {
  try {
    $r = $Element.Current.BoundingRectangle
    return -not $Element.Current.IsOffscreen -and $r.Width -gt 0 -and $r.Height -gt 0
  } catch { return $false }
}

function Get-BoundedDescendants {
  param(
    [Parameter(Mandatory)] [Windows.Automation.AutomationElement] $Root,
    [ValidateRange(1, 4096)] [int] $NodeBudget = 4096,
    [ValidateRange(100, 20000)] [int] $TimeoutMilliseconds = 8000
  )
  $clock = [Diagnostics.Stopwatch]::StartNew()
  $all = @($Root.FindAll([Windows.Automation.TreeScope]::Descendants, [Windows.Automation.Condition]::TrueCondition))
  $truncated = $all.Count -gt $NodeBudget
  $nodes = if ($truncated) { @($all | Select-Object -First $NodeBudget) } else { $all }
  return [pscustomobject]@{ nodes = @($nodes); truncated = $truncated; elapsed_ms = [int]$clock.ElapsedMilliseconds }
}

function Get-RowWords {
  param([Parameter(Mandatory)] [Windows.Automation.AutomationElement] $Root, [bool] $Starve)
  $walk = Get-BoundedDescendants -Root $Root
  $nodes = @($walk.nodes)
  $rows = [System.Collections.Generic.List[object]]::new()
  $rowIndex = 0
  for ($i = 0; $i -lt $nodes.Count; $i++) {
    $node = $nodes[$i]
    try {
      if (-not ((Get-Visible $node) -and $node.Current.ControlType -eq [Windows.Automation.ControlType]::ListItem)) { continue }
    } catch { continue }
    # TASK 4088 winning route: AutomationElement.Current.Name. -StarveRoute
    # simulates that route being removed by blanking the captured value
    # before the shared trim/drop mapping runs, same as the Rust starve test.
    $rawName = if ($Starve) { '' } else { [string]$node.Current.Name }
    $text = $rawName.Trim()
    $bounds = $node.Current.BoundingRectangle
    [void]$rows.Add([ordered]@{
      row_index = $rowIndex
      raw_name = $rawName
      text = $text
      bounds = [ordered]@{ x = [int]$bounds.X; y = [int]$bounds.Y; width = [int]$bounds.Width; height = [int]$bounds.Height }
    })
    $rowIndex++
  }
  return [pscustomobject]@{ rows = @($rows); walk_truncated = $walk.truncated; walk_elapsed_ms = $walk.elapsed_ms }
}

function Get-CopyFromScreenEvidence {
  # Anchored at the always-present window chrome (title bar / conversation
  # header), not a transcript row, so this positive control works even when
  # the open conversation has zero rows (the "before" receipt).
  param([Parameter(Mandatory)] [Windows.Automation.AutomationElement] $WindowRoot)
  $winRect = $WindowRoot.Current.BoundingRectangle
  $width = 128
  $height = 128
  $bitmap = [Drawing.Bitmap]::new($width, $height); $graphics = [Drawing.Graphics]::FromImage($bitmap)
  try {
    $left = [int][Math]::Floor($winRect.X)
    $top = [int][Math]::Floor($winRect.Y)
    $graphics.CopyFromScreen($left, $top, 0, 0, $bitmap.Size)
    $colours = [System.Collections.Generic.HashSet[int]]::new()
    for ($y = 0; $y -lt $height; $y++) { for ($x = 0; $x -lt $width; $x++) { [void]$colours.Add($bitmap.GetPixel($x, $y).ToArgb()) } }
    return [ordered]@{ method = 'Windows.PowerShell.CopyFromScreen'; left = $left; top = $top; width = $width; height = $height; distinct_colours = $colours.Count; brightness_judgments = 'forbidden-and-not-used' }
  } finally { $graphics.Dispose(); $bitmap.Dispose() }
}

try {
  $process = Get-ExactSignalProcess
  $root = [Windows.Automation.AutomationElement]::FromHandle($process.MainWindowHandle)
  if ($null -eq $root) { throw 'Signal UIA root unavailable' }
  $discovery = Get-RowWords -Root $root -Starve:([bool]$StarveRoute)
  $allRows = @($discovery.rows)
  $words = $allRows.Where({ $_.text -ne '' })
  $matched = @(if ($MarkerPrefix) { $words.Where({ $_.text -like "*$MarkerPrefix*" }) } else { $words })
  $emptyCount = $allRows.Where({ $_.text -eq '' }).Count

  $result = [ordered]@{
    schema = 'task-4089-signal-live-row-words/v1'
    task = 4089
    source = 'live Windows UI Automation'
    tasklist_process = 'Signal.exe'
    powershell = $PSVersionTable.PSVersion.ToString()
    signal_pid = $process.Id
    winner = 'row.name'
    starved = [bool]$StarveRoute
    marker_prefix = $MarkerPrefix
    row_count_seen = $allRows.Count
    words_count = $words.Count
    empty_count = $emptyCount
    matched_count = $matched.Count
    row_walk_truncated = $discovery.walk_truncated
    row_walk_elapsed_ms = $discovery.walk_elapsed_ms
    saved_ui_trees = 0
    key_press_count = 0
    send_count = 0
    messages = @($matched | ForEach-Object { [ordered]@{ row_index = $_.row_index; text = $_.text } })
    capture = Get-CopyFromScreenEvidence $root
  }
  $json = $result | ConvertTo-Json -Depth 12 -Compress
  if ($OutputPath) { [IO.File]::WriteAllText($OutputPath, $json + [Environment]::NewLine, [Text.UTF8Encoding]::new($false)) }
  $json
  exit 0
} catch {
  [ordered]@{ schema = 'task-4089-signal-live-row-words/v1'; task = 4089; status = 'failed'; error = $_.Exception.Message } | ConvertTo-Json -Compress
  exit 1
}
