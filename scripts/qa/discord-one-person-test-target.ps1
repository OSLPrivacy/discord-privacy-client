param(
  [Parameter(Mandatory = $true)]
  [ValidatePattern('^[A-Za-z0-9._#@-]{1,64}$')]
  [string]$Conversation,

  [Parameter(Mandatory = $true)]
  [ValidatePattern('^[A-Z0-9._-]{1,64}$')]
  [string]$Message
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$modulePath = Join-Path $PSScriptRoot 'discord-one-person-test-target.psm1'
Import-Module $modulePath -Force

$result = Invoke-OnePersonDiscordQaPlacement -Conversation $Conversation -Message $Message
$result | ConvertTo-Json -Compress
exit ([int]$result.ExitCode)
