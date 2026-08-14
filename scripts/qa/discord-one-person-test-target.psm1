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

function New-OnePersonDiscordQaFailure {
  param(
    [Parameter(Mandatory = $true)]
    [string]$Conversation,

    [Parameter(Mandatory = $true)]
    [string]$Error
  )

  # Failure objects intentionally never include supplied text.  The caller can
  # only receive text after task 3406 has read a non-composer message node from
  # Discord's accessibility tree.
  [pscustomobject]@{
    Ok = $false
    ExitCode = 1
    Conversation = $Conversation
    ConversationReported = ''
    DiscordProcessId = 0
    AccessibilityWaitMs = 0
    Error = $Error
    PlacedText = ''
    ReadBack = ''
    PlacedCharacters = 0
  }
}

function Resolve-OnePersonDiscordQaPlacementJob {
  param([string]$PlacementJob)

  $candidates = @()
  if (-not [string]::IsNullOrWhiteSpace($PlacementJob)) { $candidates += $PlacementJob }
  if (-not [string]::IsNullOrWhiteSpace($env:OSL_TASK_3406_PLACE_TEXT)) {
    $candidates += $env:OSL_TASK_3406_PLACE_TEXT
  }
  $candidates += (Join-Path $PSScriptRoot 'task-3406-place-text.exe')
  foreach ($candidate in $candidates) {
    if (Test-Path -LiteralPath $candidate -PathType Leaf) {
      return (Resolve-Path -LiteralPath $candidate).Path
    }
  }
  foreach ($name in @('task-3406-place-text.exe', 'task_3406_place_text.exe')) {
    $command = Get-Command $name -CommandType Application -ErrorAction SilentlyContinue |
      Select-Object -First 1
    if ($null -ne $command) { return $command.Source }
  }
  throw 'Discord placement job 3406 is absent; set OSL_TASK_3406_PLACE_TEXT to its shipped executable.'
}

function Get-OnePersonDiscordQaJobField {
  param(
    [Parameter(Mandatory = $true)]
    [string[]]$Lines,

    [Parameter(Mandatory = $true)]
    [string]$Name
  )

  $prefix = "$Name="
  $matches = @($Lines | Where-Object { $_.StartsWith($prefix, [StringComparison]::Ordinal) } |
    ForEach-Object { $_.Substring($prefix.Length) })
  if ($matches.Count -ne 1 -or [string]::IsNullOrWhiteSpace([string]$matches[0])) {
    throw "Discord placement job 3406 did not report exactly one $Name from Discord's accessibility tree."
  }
  [string]$matches[0]
}

function Invoke-OnePersonDiscordQaPlacement {
  param(
    [Parameter(Mandatory = $true)]
    [string]$Conversation,

    [Parameter(Mandatory = $true)]
    [ValidatePattern('^[A-Z0-9._-]{1,64}$')]
    [string]$Message,

    [string]$PlacementJob
  )

  try {
    $guard = Assert-OnePersonDiscordQaConversation -Conversation $Conversation
  } catch {
    return New-OnePersonDiscordQaFailure -Conversation $Conversation -Error $_.Exception.Message
  }

  $discord = @(Get-Process -Name 'Discord' -ErrorAction SilentlyContinue |
    Where-Object { $_.Id -gt 0 } | Sort-Object Id)
  if ($discord.Count -eq 0) {
    return New-OnePersonDiscordQaFailure -Conversation $guard.Conversation -Error 'Discord process is absent.'
  }

  try {
    $job = Resolve-OnePersonDiscordQaPlacementJob -PlacementJob $PlacementJob
    # task 3406 performs the cross-process UIA read.  No monitor, mutex, or
    # UI-thread lock is held while this external process waits on Discord.
    $jobOutput = @(& $job --app Discord --composer-name ("Message @{0}" -f $guard.Conversation) `
      --text $Message --send-message 2>&1 | ForEach-Object { [string]$_ })
    $jobExitCode = $LASTEXITCODE
    if ($jobExitCode -ne 0) {
      $detail = [string]::Join(' | ', $jobOutput)
      throw "Discord placement job 3406 refused (exit $jobExitCode): $detail"
    }

    $reportedPid = [int](Get-OnePersonDiscordQaJobField -Lines $jobOutput -Name 'discord_process_id')
    if ($reportedPid -le 0 -or -not ($discord.Id -contains $reportedPid)) {
      throw "Discord placement job 3406 reported absent or unexpected Discord process id $reportedPid."
    }
    $reportedConversation = Get-OnePersonDiscordQaJobField -Lines $jobOutput -Name 'conversation_tree_reported'
    if ($reportedConversation -cne $guard.Conversation) {
      throw "Discord accessibility tree reported conversation '$reportedConversation'; expected '$($guard.Conversation)'."
    }
    $waitMs = [int](Get-OnePersonDiscordQaJobField -Lines $jobOutput -Name 'accessibility_wait_ms')
    if ($waitMs -lt 0) {
      throw "Discord placement job 3406 reported an invalid accessibility wait $waitMs ms."
    }
    # This value is emitted only by task 3406 after it finds a non-composer
    # message element in Discord's accessibility tree after Enter was sent.
    $readBack = Get-OnePersonDiscordQaJobField -Lines $jobOutput -Name 'sent_message_readback'
    if ($readBack -cne $Message) {
      throw "Discord accessibility tree read '$readBack', not the placed mark."
    }

    [pscustomobject]@{
      Ok = $true
      ExitCode = 0
      Conversation = $guard.Conversation
      ConversationReported = $reportedConversation
      DiscordProcessId = $reportedPid
      AccessibilityWaitMs = $waitMs
      PlacedText = $readBack
      ReadBack = $readBack
      PlacedCharacters = $readBack.Length
    }
  } catch {
    New-OnePersonDiscordQaFailure -Conversation $guard.Conversation -Error $_.Exception.Message
  }
}

Export-ModuleMember `
  -Function `
    Get-OnePersonDiscordQaConversation, `
    ConvertFrom-DiscordQaComposerName, `
    Assert-OnePersonDiscordQaConversation, `
    Invoke-OnePersonDiscordQaPlacement
