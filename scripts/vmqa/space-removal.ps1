<#
.SYNOPSIS
  Three-machine UI Automation proof that Space removal stops an idle sender.

.DESCRIPTION
  The removal is deliberately followed by a send from a *different*, previously
  idle member. Sending from the remover would test only the easy path: sender
  key rotation is per sender, so that scenario can pass while the removed member
  can still decrypt another member's next message.
#>
[CmdletBinding()]
param(
  [Parameter(Mandatory)] [ValidateNotNullOrEmpty()] [string]$RemoverVm,
  [Parameter(Mandatory)] [ValidateNotNullOrEmpty()] [string]$IdleSenderVm,
  [Parameter(Mandatory)] [ValidateNotNullOrEmpty()] [string]$RemovedVm,
  [Parameter(Mandatory)] [ValidateNotNullOrEmpty()] [string]$SpaceId,
  [string]$ResultDirectory = 'C:\OSL\out'
)

$ErrorActionPreference = 'Stop'
$members = @($RemoverVm, $IdleSenderVm, $RemovedVm)
if (($members | Select-Object -Unique).Count -ne 3) { throw 'SPACE_REMOVAL_REQUIRES_THREE_DISTINCT_VMS' }
$nonce = 'space-removal-idle-' + [guid]::NewGuid().ToString('N')
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
  $remover, $idleSender, $removed = $sessions
  Invoke-SpaceUia $remover @{ Mode = 'remove-member'; SpaceId = $SpaceId; MemberRole = 'removed' }
  # Sabotage guard: the next message must be from the member idle during removal.
  Invoke-SpaceUia $idleSender @{ Mode = 'send'; SpaceId = $SpaceId; Text = $nonce }
  $removedLog = Invoke-Command -Session $removed -ScriptBlock {
    param($id, $text, $out)
    $bridge = 'C:\OSL\space.ps1'
    if (-not (Test-Path -LiteralPath $bridge)) { throw "SPACE_UIA_BRIDGE_MISSING:$bridge" }
    & $bridge -Mode refresh -SpaceId $id -ExpectAbsent $text -OutFile $out
    if ($LASTEXITCODE -ne 0) { throw "SPACE_UIA_BRIDGE_FAILED:$LASTEXITCODE" }
    if (-not (Test-Path -LiteralPath $out)) { throw "SPACE_UIA_TRANSCRIPT_MISSING:$out" }
    Get-Content -LiteralPath $out -Raw
  } -ArgumentList $SpaceId, $nonce, (Join-Path $ResultDirectory 'space-removal-removed.txt')
  if ($removedLog -match [regex]::Escape($nonce)) { throw "REMOVED_MEMBER_DECRYPTED_IDLE_SENDER_MESSAGE:$nonce" }
  Write-Host "PASS SPACE_REMOVAL_IDLE_SENDER_BLOCKED nonce=$nonce"
} finally {
  if ($sessions) { $sessions | Remove-PSSession -ErrorAction SilentlyContinue }
}
