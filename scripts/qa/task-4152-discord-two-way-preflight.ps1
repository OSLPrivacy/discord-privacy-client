[CmdletBinding()]
param(
  [Parameter(Mandatory = $true)]
  [string]$OutputPath,

  # Test-only fault injection.  A run with this switch must never progress to
  # sender work: it models either endpoint's receiving job being unavailable.
  [switch]$StubCanaryReceiver,
  [switch]$StubPtbReceiver
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

# 4152 may send only after both independently signed-in release channels have
# visible, readable receiver surfaces.  The 4150 census owns the live process,
# account, store and caller-profile checks.  This wrapper deliberately has no
# composer or synthetic-keyboard code: a failed receiver preflight is a terminal result,
# never permission to attempt a blind Discord send.
$preflight = Join-Path $PSScriptRoot 'task-4150-live-receiver-preflight.ps1'
if (-not (Test-Path -LiteralPath $preflight -PathType Leaf)) {
  throw 'TASK4152_PREFLIGHT_MISSING task-4150-live-receiver-preflight.ps1'
}

$absoluteOutput = [IO.Path]::GetFullPath($OutputPath)
$directory = Split-Path -Path $absoluteOutput -Parent
[void](New-Item -ItemType Directory -Path $directory -Force)
$censusPath = Join-Path $directory 'task4152-release-census.json'

& $preflight -OutputPath $censusPath
$censusExit = $LASTEXITCODE
$census = Get-Content -LiteralPath $censusPath -Raw | ConvertFrom-Json

$endpoints = @($census.endpoints)
$canary = @($endpoints | Where-Object { $_.release_channel -ceq 'Canary' })
$ptb = @($endpoints | Where-Object { $_.release_channel -ceq 'PTB' })
$blockers = [Collections.Generic.List[string]]::new()
if ($canary.Count -ne 1 -or $ptb.Count -ne 1) {
  $blockers.Add('endpoint roster is not exactly Canary and PTB')
} else {
  if ([int]$canary[0].interactive_windows -ne 1) { $blockers.Add('Canary receiver window is not exactly one') }
  if ([int]$ptb[0].interactive_windows -ne 1) { $blockers.Add('PTB receiver window is not exactly one') }
}
if (-not [bool]$census.gates.distinct_accounts) { $blockers.Add('account isolation failed') }
if (-not [bool]$census.gates.distinct_stores) { $blockers.Add('store isolation failed') }
if (-not [bool]$census.gates.no_caller_user_data_dir) { $blockers.Add('caller supplied --user-data-dir') }
if ($StubCanaryReceiver) { $blockers.Add('Canary receiving job stubbed') }
if ($StubPtbReceiver) { $blockers.Add('PTB receiving job stubbed') }

$record = [ordered]@{
  schema = 'osl.task4152.two-way-discord-preflight.v1'
  captured_at_utc = [DateTime]::UtcNow.ToString('o')
  channels = @('Canary', 'PTB')
  endpoints = $endpoints
  independent_osl_data_paths = @(
    (Join-Path $env:LOCALAPPDATA 'OSL Privacy\qa\discord-release-channels\canary'),
    (Join-Path $env:LOCALAPPDATA 'OSL Privacy\qa\discord-release-channels\ptb')
  )
  receiver_jobs = [ordered]@{
    canary = [ordered]@{ stubbed = [bool]$StubCanaryReceiver; ready = ($canary.Count -eq 1 -and [int]$canary[0].interactive_windows -eq 1 -and -not $StubCanaryReceiver) }
    ptb = [ordered]@{ stubbed = [bool]$StubPtbReceiver; ready = ($ptb.Count -eq 1 -and [int]$ptb[0].interactive_windows -eq 1 -and -not $StubPtbReceiver) }
  }
  send_attempts = 0
  opened_private_messages_before = $null
  opened_private_messages_after = $null
  ready_for_two_way_send = ($censusExit -eq 0 -and $blockers.Count -eq 0)
  blockers = @($blockers)
}
$record | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $absoluteOutput -Encoding utf8
$record | ConvertTo-Json -Depth 8

if (-not $record.ready_for_two_way_send) { exit 1 }
