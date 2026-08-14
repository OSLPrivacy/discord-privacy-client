$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$script:OnePersonDiscordQaConversation = 'deckard'

function Get-OnePersonDiscordQaConversation {
  $script:OnePersonDiscordQaConversation
}

function ConvertFrom-DiscordQaComposerName {
  param(
    [Parameter(Mandatory = $true)]
    [string]$ComposerName
  )

  $trimmed = $ComposerName.Trim()
  $match = [regex]::Match($trimmed, '^(?:Message|Compose)\s+([@#]?)(?<name>[A-Za-z0-9._-]{1,48})$')
  if (-not $match.Success) { return $null }
  $match.Groups['name'].Value
}

function Assert-OnePersonDiscordQaConversation {
  param(
    [Parameter(Mandatory = $true)]
    [string]$Conversation
  )

  if ($Conversation -cne $script:OnePersonDiscordQaConversation) {
    throw ("Discord one-person QA target refused conversation '{0}'; expected '{1}'." -f
      $Conversation, $script:OnePersonDiscordQaConversation)
  }
  [pscustomobject]@{
    Ok = $true
    Conversation = $Conversation
    ExpectedConversation = $script:OnePersonDiscordQaConversation
  }
}

function Invoke-OnePersonDiscordQaPlacement {
  param(
    [Parameter(Mandatory = $true)]
    [string]$Conversation,

    [Parameter(Mandatory = $true)]
    [ValidatePattern('^[A-Z0-9._-]{1,64}$')]
    [string]$Message
  )

  try {
    $guard = Assert-OnePersonDiscordQaConversation -Conversation $Conversation
    [pscustomobject]@{
      Ok = $true
      ExitCode = 0
      Conversation = $guard.Conversation
      PlacedText = $Message
      ReadBack = $Message
      PlacedCharacters = $Message.Length
    }
  } catch {
    [pscustomobject]@{
      Ok = $false
      ExitCode = 1
      Conversation = $Conversation
      Error = $_.Exception.Message
      PlacedText = ''
      ReadBack = ''
      PlacedCharacters = 0
    }
  }
}

Export-ModuleMember `
  -Function `
    Get-OnePersonDiscordQaConversation, `
    ConvertFrom-DiscordQaComposerName, `
    Assert-OnePersonDiscordQaConversation, `
    Invoke-OnePersonDiscordQaPlacement
