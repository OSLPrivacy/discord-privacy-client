<#
.SYNOPSIS
  Three-machine UIA proof that a new Space member cannot read pre-join history.

.DESCRIPTION
  The corpus is sent before the joiner is admitted, then the run waits beyond
  the five-minute sender-key distribution re-emit before admitting the joiner.
  This makes a stale pre-join root leak observable rather than testing a join
  that occurred before there was any protected history.
#>
[CmdletBinding()]
param(
  [Parameter(Mandatory)] [ValidateNotNullOrEmpty()] [string]$SenderVm,
  [Parameter(Mandatory)] [ValidateNotNullOrEmpty()] [string]$AdminVm,
  [Parameter(Mandatory)] [ValidateNotNullOrEmpty()] [string]$JoinerVm,
  [Parameter(Mandatory)] [ValidateNotNullOrEmpty()] [string]$SpaceId,
  [string]$ResultDirectory = 'C:\OSL\out',
  [ValidateRange(300, 900)] [int]$ReemitWaitSeconds = 310
)

$ErrorActionPreference = 'Stop'
$members = @($SenderVm, $AdminVm, $JoinerVm)
if (($members | Select-Object -Unique).Count -ne 3) { throw 'SPACE_JOIN_HISTORY_REQUIRES_THREE_DISTINCT_VMS' }
$before = 'space-before-' + [guid]::NewGuid().ToString('N')
$after = 'space-after-' + [guid]::NewGuid().ToString('N')
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

try {
  $sessions = @($members | ForEach-Object { New-PSSession -ComputerName $_ })
  $sender, $admin, $joiner = $sessions
  # Sabotage guard: this corpus must exist before membership is changed.
  Invoke-SpaceUia $sender @{ Mode = 'send'; SpaceId = $SpaceId; Text = $before }
  Start-Sleep -Seconds $ReemitWaitSeconds
  Invoke-SpaceUia $admin @{ Mode = 'add-member'; SpaceId = $SpaceId; MemberRole = 'joiner' }
  Invoke-SpaceUia $sender @{ Mode = 'send'; SpaceId = $SpaceId; Text = $after }
  $log = Invoke-Command -Session $joiner -ScriptBlock {
    param($id, $out)
    $bridge = 'C:\OSL\space.ps1'
    if (-not (Test-Path -LiteralPath $bridge)) { throw "SPACE_UIA_BRIDGE_MISSING:$bridge" }
    & $bridge -Mode refresh -SpaceId $id -OutFile $out
    if ($LASTEXITCODE -ne 0) { throw "SPACE_UIA_BRIDGE_FAILED:$LASTEXITCODE" }
    if (-not (Test-Path -LiteralPath $out)) { throw "SPACE_UIA_TRANSCRIPT_MISSING:$out" }
    Get-Content -LiteralPath $out -Raw
  } -ArgumentList $SpaceId, (Join-Path $ResultDirectory 'space-join-history.txt')
  if ($log -match [regex]::Escape($before)) { throw "JOINER_READ_PREJOIN_MESSAGE:$before" }
  if ([regex]::Matches($log, [regex]::Escape($after)).Count -ne 1) {
    throw "JOINER_DID_NOT_RENDER_EXACTLY_ONE_POSTJOIN_MESSAGE:$after"
  }
  Write-Host "PASS SPACE_JOIN_HISTORY_EXACTLY_ONE_POSTJOIN nonce=$after"
} finally {
  if ($sessions) { $sessions | Remove-PSSession -ErrorAction SilentlyContinue }
}
