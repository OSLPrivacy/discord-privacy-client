<#
.SYNOPSIS
  Three-machine UIA proof for offline Space history, queueing, local burn, and
  stale-roster refusal after reconnect.

.DESCRIPTION
  The adapter is disabled before every offline assertion. The queued message is
  intentionally retained until reconnect so the stale-roster case proves that
  a previously offline member cannot drain a send against an obsolete roster.
#>
[CmdletBinding()]
param(
  [Parameter(Mandatory)] [ValidateNotNullOrEmpty()] [string]$OfflineVm,
  [Parameter(Mandatory)] [ValidateNotNullOrEmpty()] [string]$AdminVm,
  [Parameter(Mandatory)] [ValidateNotNullOrEmpty()] [string]$IdleMemberVm,
  [Parameter(Mandatory)] [ValidateNotNullOrEmpty()] [string]$SpaceId,
  [Parameter(Mandatory)] [ValidateNotNullOrEmpty()] [string]$AdapterName,
  [string]$ResultDirectory = 'C:\OSL\out'
)

$ErrorActionPreference = 'Stop'
$members = @($OfflineVm, $AdminVm, $IdleMemberVm)
if (($members | Select-Object -Unique).Count -ne 3) { throw 'SPACE_OFFLINE_REQUIRES_THREE_DISTINCT_VMS' }
$queued = 'space-offline-queued-' + [guid]::NewGuid().ToString('N')
$burn = 'space-offline-burn-' + [guid]::NewGuid().ToString('N')
$sessions = @()

function Invoke-SpaceUia {
  param($Session, [hashtable]$Arguments)
  Invoke-Command -Session $Session -ScriptBlock {
    param($args)
    $bridge = 'C:\OSL\space.ps1'
    if (-not (Test-Path -LiteralPath $bridge)) { throw "SPACE_UIA_BRIDGE_MISSING:$bridge" }
    & $bridge @args
    if ($LASTEXITCODE -ne 0) { throw "SPACE_UIA_BRIDGE_FAILED:$LASTEXITCODE" }
  } -ArgumentList $Arguments
}

function Read-SpaceUiaTranscript {
  param($Session, [hashtable]$Arguments, [string]$OutFile)
  Invoke-Command -Session $Session -ScriptBlock {
    param($args, $out)
    $bridge = 'C:\OSL\space.ps1'
    if (-not (Test-Path -LiteralPath $bridge)) { throw "SPACE_UIA_BRIDGE_MISSING:$bridge" }
    & $bridge @args -OutFile $out
    if ($LASTEXITCODE -ne 0) { throw "SPACE_UIA_BRIDGE_FAILED:$LASTEXITCODE" }
    if (-not (Test-Path -LiteralPath $out)) { throw "SPACE_UIA_TRANSCRIPT_MISSING:$out" }
    Get-Content -LiteralPath $out -Raw
  } -ArgumentList $Arguments, $OutFile
}

try {
  $sessions = @($members | ForEach-Object { New-PSSession -ComputerName $_ })
  $offline, $admin, $idle = $sessions
  Invoke-Command -Session $offline -ScriptBlock {
    param($adapter)
    Disable-NetAdapter -Name $adapter -Confirm:$false
    if ((Get-NetAdapter -Name $adapter).Status -ne 'Disabled') { throw 'ADAPTER_STILL_UP' }
  } -ArgumentList $AdapterName

  $history = Read-SpaceUiaTranscript $offline @{ Mode = 'offline-history'; SpaceId = $SpaceId; ExpectHistoryReadable = $true } (Join-Path $ResultDirectory 'space-offline-history.txt')
  if ($history -notmatch 'history readable') { throw 'OFFLINE_HISTORY_NOT_READABLE' }

  $queueResult = Read-SpaceUiaTranscript $offline @{ Mode = 'offline-send'; SpaceId = $SpaceId; Text = $queued; ExpectQueued = $true } (Join-Path $ResultDirectory 'space-offline-queued.txt')
  if ($queueResult -notmatch '(?i)queued' -or $queueResult -match '(?i)sent') { throw 'OFFLINE_COMPOSE_DID_NOT_STAY_QUEUED' }

  $burnResult = Read-SpaceUiaTranscript $offline @{ Mode = 'local-burn'; SpaceId = $SpaceId; Text = $burn; ExpectImmediate = $true } (Join-Path $ResultDirectory 'space-offline-burn.txt')
  if ($burnResult -notmatch '(?i)burned|deleted' -or $burnResult -match '(?i)queued|pending') { throw 'LOCAL_BURN_WAS_NOT_IMMEDIATE' }

  # While offline's queued send waits, a third member must be the one that
  # drives the stale-roster condition. Never leave the adapter up as sabotage.
  Invoke-SpaceUia $admin @{ Mode = 'remove-member'; SpaceId = $SpaceId; MemberRole = 'removed' }
  Invoke-Command -Session $offline -ScriptBlock { param($adapter) Enable-NetAdapter -Name $adapter -Confirm:$false } -ArgumentList $AdapterName
  $drain = Read-SpaceUiaTranscript $offline @{ Mode = 'drain'; SpaceId = $SpaceId; ExpectStaleRosterRefusal = $true } (Join-Path $ResultDirectory 'space-offline-stale-roster.txt')
  if ($drain -notmatch '(?i)stale roster|refresh membership|not sent' -or $drain -match '(?i)delivered') { throw 'STALE_OFFLINE_SENDER_WAS_NOT_REFUSED' }
  Write-Host "PASS SPACE_OFFLINE_QUEUE_BURN_AND_STALE_ROSTER nonce=$queued"
} finally {
  if ($sessions) { $sessions | Remove-PSSession -ErrorAction SilentlyContinue }
}
