param(
  [string]$PlaceExe,
  [string]$OutputPath = "evidence/task-3420-front-window-steal-place/result.json",
  [int]$Attempts = 10,
  [string]$App = "Discord",
  [string]$InitialFront = "OSL-TASK-3420-THIEF",
  [string]$ControlText = "FOCUS-3420",
  [string]$FinalControlText = "FOCUS-3420-AGAIN",
  [int]$StealRepeatCount = 30,
  [int]$StealIntervalMs = 75,
  [switch]$StubPlaceNoop,
  [switch]$FocusThiefChild,
  [string]$StealFlagPath,
  [string]$ThiefTextPath
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

function Write-Utf8NoBom {
  param(
    [Parameter(Mandatory = $true)][string]$Path,
    [Parameter(Mandatory = $true)][string]$Text
  )

  $full = [IO.Path]::GetFullPath($Path)
  $parent = [IO.Path]::GetDirectoryName($full)
  if (-not [string]::IsNullOrWhiteSpace($parent)) {
    New-Item -ItemType Directory -Force -Path $parent | Out-Null
  }
  $utf8NoBom = New-Object Text.UTF8Encoding $false
  [IO.File]::WriteAllText($full, $Text, $utf8NoBom)
}

if ($FocusThiefChild) {
  if ([string]::IsNullOrWhiteSpace($StealFlagPath) -or [string]::IsNullOrWhiteSpace($ThiefTextPath)) {
    throw 'FocusThiefChild needs -StealFlagPath and -ThiefTextPath'
  }

  Add-Type -AssemblyName System.Windows.Forms
  Add-Type -AssemblyName System.Drawing

  $form = [Windows.Forms.Form]::new()
  $form.Text = $InitialFront
  $form.Width = 720
  $form.Height = 180
  $form.StartPosition = [Windows.Forms.FormStartPosition]::CenterScreen
  $form.TopMost = $true

  $box = [Windows.Forms.TextBox]::new()
  $box.Multiline = $false
  $box.Dock = [Windows.Forms.DockStyle]::Fill
  $box.Font = [Drawing.Font]::new('Segoe UI', 18)
  $form.Controls.Add($box)

  $timer = [Windows.Forms.Timer]::new()
  $timer.Interval = [Math]::Max(25, $StealIntervalMs)
  $timer.Add_Tick({
    Write-Utf8NoBom -Path $ThiefTextPath -Text $box.Text
    if ((Test-Path -LiteralPath $StealFlagPath) -and ((Get-Content -LiteralPath $StealFlagPath -Raw) -match '^1')) {
      $form.TopMost = $true
      $form.Activate()
      $box.Focus()
    }
  })
  $form.Add_Shown({
    $timer.Start()
    $form.Activate()
    $box.Focus()
  })
  [Windows.Forms.Application]::Run($form)
  exit 0
}

if ($Attempts -ne 10) {
  throw "Task 3420 is defined as exactly 10 front-window-stealing runs; got $Attempts"
}
if ($StealRepeatCount -lt 1) {
  throw '-StealRepeatCount must be at least 1'
}
if (-not $StubPlaceNoop -and [string]::IsNullOrWhiteSpace($PlaceExe)) {
  throw '-PlaceExe is required unless -StubPlaceNoop is set'
}
if (-not $StubPlaceNoop -and -not (Test-Path -LiteralPath $PlaceExe)) {
  throw "PlaceExe not found: $PlaceExe"
}

$runId = "task3420-$PID-$([DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds())"
$scratch = Join-Path ([IO.Path]::GetTempPath()) $runId
New-Item -ItemType Directory -Force -Path $scratch | Out-Null
$stealFlag = Join-Path $scratch 'steal.flag'
$thiefText = Join-Path $scratch 'thief-text.txt'
Write-Utf8NoBom -Path $stealFlag -Text '0'
Write-Utf8NoBom -Path $thiefText -Text ''

$psExe = (Get-Process -Id $PID).Path
$childArgs = @(
  '-NoProfile',
  '-ExecutionPolicy', 'Bypass',
  '-File', $PSCommandPath,
  '-FocusThiefChild',
  '-InitialFront', $InitialFront,
  '-StealIntervalMs', "$StealIntervalMs",
  '-StealFlagPath', $stealFlag,
  '-ThiefTextPath', $thiefText
)
$thief = Start-Process -FilePath $psExe -ArgumentList $childArgs -PassThru
Start-Sleep -Milliseconds 700

$records = New-Object System.Collections.Generic.List[object]

function Invoke-PlacingJob {
  param(
    [Parameter(Mandatory = $true)][string]$Text,
    [Parameter(Mandatory = $true)][bool]$Steal
  )

  if ($Steal) {
    Write-Utf8NoBom -Path $stealFlag -Text '1'
    Start-Sleep -Milliseconds ([Math]::Min(250, $StealRepeatCount * $StealIntervalMs))
  } else {
    Write-Utf8NoBom -Path $stealFlag -Text '0'
    Start-Sleep -Milliseconds 200
  }
  Write-Utf8NoBom -Path $thiefText -Text ''

  if ($StubPlaceNoop) {
    return [pscustomobject]@{
      exitCode = 0
      stdout = 'stubbed place job did nothing'
      stderr = ''
    }
  }

  $stdoutPath = Join-Path $scratch "place-$Text.stdout.txt"
  $stderrPath = Join-Path $scratch "place-$Text.stderr.txt"
  $process = Start-Process -FilePath $PlaceExe -ArgumentList @(
    '--initial-front', $InitialFront,
    '--app', $App,
    '--text', $Text
  ) -RedirectStandardOutput $stdoutPath -RedirectStandardError $stderrPath -NoNewWindow -Wait -PassThru

  [pscustomobject]@{
    exitCode = $process.ExitCode
    stdout = if (Test-Path -LiteralPath $stdoutPath) { Get-Content -LiteralPath $stdoutPath -Raw } else { '' }
    stderr = if (Test-Path -LiteralPath $stderrPath) { Get-Content -LiteralPath $stderrPath -Raw } else { '' }
  }
}

function Get-RefusalName {
  param(
    [Parameter(Mandatory = $true)][int]$ExitCode,
    [Parameter(Mandatory = $true)][string]$CombinedOutput
  )

  if ($ExitCode -eq 0) {
    return 'missing-exact-placement-proof'
  }
  if ($CombinedOutput -match 'initial front window was not') { return 'initial-front-stolen' }
  if ($CombinedOutput -match 'was already the foreground window') { return 'target-not-behind-initial' }
  if ($CombinedOutput -match 'not found') { return 'target-not-found' }
  if ($CombinedOutput -match 'did not become the foreground window') { return 'target-front-stolen' }
  if ($CombinedOutput -match 'composer not found') { return 'composer-not-found' }
  if ($CombinedOutput -match 'composer ambiguous') { return 'composer-ambiguous' }
  if ($CombinedOutput -match 'composer was not empty before placement') { return 'composer-not-empty' }
  if ($CombinedOutput -match 'composer click point is obscured') { return 'composer-click-obscured' }
  if ($CombinedOutput -match 'composer did not take keyboard focus') { return 'composer-focus-stolen' }
  if ($CombinedOutput -match 'readback did not equal placed mark') { return 'exact-readback-mismatch' }
  if ($CombinedOutput -match 'clipboard') { return 'clipboard-refusal' }
  return 'place-command-refused'
}

function Add-RunRecord {
  param(
    [Parameter(Mandatory = $true)][string]$Label,
    [Parameter(Mandatory = $true)][int]$Index,
    [Parameter(Mandatory = $true)][string]$Text,
    [Parameter(Mandatory = $true)][bool]$Steal
  )

  $job = Invoke-PlacingJob -Text $Text -Steal $Steal
  Start-Sleep -Milliseconds 200
  $thiefValue = if (Test-Path -LiteralPath $thiefText) { Get-Content -LiteralPath $thiefText -Raw } else { '' }
  $combined = "$($job.stdout)`n$($job.stderr)"
  $expectedReadback = 'readback="' + $Text + '"'
  $exact = $job.exitCode -eq 0 -and $combined.Contains('before_readback=""') -and $combined.Contains($expectedReadback)
  $wrongPlace = $thiefValue.Contains($Text)
  $refusal = if ($exact) { $null } else { Get-RefusalName -ExitCode $job.exitCode -CombinedOutput $combined }

  $record = [pscustomobject]@{
    label = $Label
    runIndex = $Index
    text = $Text
    stealEnabled = $Steal
    stealRepeatCount = if ($Steal) { $StealRepeatCount } else { 0 }
    exactPlacement = $exact
    refusalName = $refusal
    wrongPlace = $wrongPlace
    exitCode = $job.exitCode
    stdout = $job.stdout
    stderr = $job.stderr
    thiefText = $thiefValue
  }
  $records.Add($record)
  $record
}

try {
  $firstControl = Add-RunRecord -Label 'control-first' -Index 0 -Text $ControlText -Steal $false
  for ($i = 1; $i -le $Attempts; $i++) {
    Add-RunRecord -Label 'steal' -Index $i -Text "$ControlText-RUN-$i" -Steal $true | Out-Null
  }
  $finalControl = Add-RunRecord -Label 'control-final' -Index ($Attempts + 1) -Text $FinalControlText -Steal $false
} finally {
  Write-Utf8NoBom -Path $stealFlag -Text '0'
  if ($null -ne $thief -and -not $thief.HasExited) {
    Stop-Process -Id $thief.Id -Force -ErrorAction SilentlyContinue
  }
}

$stealRuns = @($records | Where-Object { $_.label -eq 'steal' })
$exactStealCount = @($stealRuns | Where-Object { $_.exactPlacement }).Count
$namedStealRefusals = @($stealRuns | Where-Object { -not $_.exactPlacement -and -not [string]::IsNullOrWhiteSpace($_.refusalName) }).Count
$wrongPlaceCount = @($records | Where-Object { $_.wrongPlace }).Count
$passed = (
  $firstControl.exactPlacement -and
  $stealRuns.Count -eq 10 -and
  $exactStealCount -ge 1 -and
  ($exactStealCount + $namedStealRefusals) -eq 10 -and
  $wrongPlaceCount -eq 0 -and
  $finalControl.exactPlacement
)

$summary = [pscustomobject]@{
  task = 3420
  attempts = $Attempts
  stealRepeatCount = $StealRepeatCount
  controlFirstText = $ControlText
  controlFirstExact = $firstControl.exactPlacement
  stealRunCount = $stealRuns.Count
  exactStealCount = $exactStealCount
  namedStealRefusalCount = $namedStealRefusals
  wrongPlaceCount = $wrongPlaceCount
  finalControlText = $FinalControlText
  finalControlExact = $finalControl.exactPlacement
  stubPlaceNoop = [bool]$StubPlaceNoop
  passed = $passed
  records = @($records)
}

$json = $summary | ConvertTo-Json -Depth 8
Write-Utf8NoBom -Path $OutputPath -Text $json

Write-Output "task3420_control_first_text=$ControlText exact=$($firstControl.exactPlacement)"
Write-Output "task3420_front_window_stealing_runs=$($stealRuns.Count)"
Write-Output "task3420_front_window_steal_repeat_count=$StealRepeatCount"
Write-Output "task3420_exact_steal_placements=$exactStealCount"
Write-Output "task3420_named_steal_refusals=$namedStealRefusals"
Write-Output "task3420_wrong_place_runs=$wrongPlaceCount"
Write-Output "task3420_final_control_text=$FinalControlText exact=$($finalControl.exactPlacement)"
Write-Output "task3420_stub_place_noop=$([bool]$StubPlaceNoop) check_passed=$passed"
Write-Output "task3420_output=$([IO.Path]::GetFullPath($OutputPath))"

if ($passed) {
  exit 0
}
exit 1
