param(
  [ValidateSet('disposable_discord_accounts_load_from_key_vault_without_logging_secrets')]
  [string]$SelfTest,

  [ValidateSet(1, 2)]
  [int]$ClientNumber,

  [string]$OslExePath,

  [string]$OslExeSha256,

  [int]$SessionId
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

function Get-OslObjectProperty {
  param(
    [Parameter(Mandatory = $true)]$Value,
    [Parameter(Mandatory = $true)][string[]]$Names
  )
  if ($null -eq $Value -or $null -eq $Value.PSObject) { return $null }
  foreach ($name in $Names) {
    $property = $Value.PSObject.Properties[$name]
    if ($null -ne $property) { return $property.Value }
  }
  return $null
}

function Assert-OslDisposableDiscordSecretName {
  param([Parameter(Mandatory = $true)][string]$SecretName)
  if ($SecretName -cnotmatch '^osl-test-discord-[0-9]{2}$') {
    throw 'disposable Discord account manifest selected an invalid secret'
  }
  return $SecretName
}

function Read-OslManifestClientNumber {
  param($Value)
  $raw = Get-OslObjectProperty $Value @(
    'clientNumber',
    'client_number',
    'client',
    'vmClientNumber'
  )
  if ($null -eq $raw) { return $null }
  $text = [string]$raw
  if ($text -cmatch '^(?:OSL-Azure-Client-)?([12])$') {
    return [int]$Matches[1]
  }
  return $null
}

function Read-OslDiscordSecretNameFromNode {
  param($Value)
  if ($Value -is [string]) {
    return Assert-OslDisposableDiscordSecretName -SecretName $Value
  }
  $secret = Get-OslObjectProperty $Value @(
    'discordSecretName',
    'discord_secret_name',
    'discordKeyVaultSecret',
    'keyVaultSecretName',
    'secretName',
    'secret'
  )
  if ($null -eq $secret) { return $null }
  return Assert-OslDisposableDiscordSecretName -SecretName ([string]$secret)
}

function Resolve-OslDisposableDiscordSecretName {
  param(
    [Parameter(Mandatory = $true)]$Manifest,
    [Parameter(Mandatory = $true)][ValidateSet(1, 2)][int]$ClientNumber
  )

  foreach ($name in @(
      "client$ClientNumber",
      "client-$ClientNumber",
      "Client$ClientNumber",
      "OSL-Azure-Client-$ClientNumber",
      [string]$ClientNumber
    )) {
    $node = Get-OslObjectProperty $Manifest @($name)
    if ($null -ne $node) {
      $resolved = Read-OslDiscordSecretNameFromNode $node
      if ($null -ne $resolved) { return $resolved }
    }
  }

  foreach ($collectionName in @('clients', 'Clients', 'accounts', 'Accounts')) {
    $collection = Get-OslObjectProperty $Manifest @($collectionName)
    foreach ($entry in @($collection)) {
      if ((Read-OslManifestClientNumber $entry) -eq $ClientNumber) {
        $service = Get-OslObjectProperty $entry @('service', 'Service')
        if ($null -eq $service -or [string]$service -ceq 'discord') {
          $resolved = Read-OslDiscordSecretNameFromNode $entry
          if ($null -ne $resolved) { return $resolved }
        }
      }
    }
  }

  foreach ($containerName in @('discord', 'Discord', 'assignments', 'Assignments')) {
    $container = Get-OslObjectProperty $Manifest @($containerName)
    if ($null -ne $container) {
      $resolved = Resolve-OslDisposableDiscordSecretName `
        -Manifest $container `
        -ClientNumber $ClientNumber
      if ($null -ne $resolved) { return $resolved }
    }
  }

  throw 'disposable Discord account manifest does not assign this client'
}

function Get-OslKeyVaultAccessToken {
  $tokenUri = 'http://169.254.169.254/metadata/identity/oauth2/token' +
    '?api-version=2019-08-01&resource=https%3A%2F%2Fvault.azure.net'
  $token = Invoke-RestMethod -Method Get -Uri $tokenUri -Headers @{ Metadata = 'true' } -TimeoutSec 10
  if ([string]::IsNullOrWhiteSpace([string]$token.access_token)) {
    throw 'managed identity unavailable'
  }
  return [string]$token.access_token
}

function Get-OslKeyVaultSecretValue {
  param(
    [Parameter(Mandatory = $true)][string]$VaultName,
    [Parameter(Mandatory = $true)][string]$SecretName,
    [Parameter(Mandatory = $true)][string]$AccessToken
  )
  $encodedSecretName = [Uri]::EscapeDataString($SecretName)
  $secretUri = "https://$VaultName.vault.azure.net/secrets/$encodedSecretName`?api-version=7.4"
  try {
    $secretRecord = Invoke-RestMethod -Method Get -Uri $secretUri -Headers @{
      Authorization = "Bearer $AccessToken"
    } -TimeoutSec 15
  } catch {
    throw 'Key Vault secret retrieval failed'
  }
  $value = [string]$secretRecord.value
  $secretRecord = $null
  return $value
}

function Invoke-OslDisposableDiscordAccountLoad {
  param(
    [Parameter(Mandatory = $true)][string]$VaultName,
    [Parameter(Mandatory = $true)][ValidateSet(1, 2)][int]$ClientNumber,
    [Parameter(Mandatory = $true)][string]$AccessToken,
    [scriptblock]$SecretFetcher
  )
  if ($null -eq $SecretFetcher) {
    $SecretFetcher = {
      param([string]$Name)
      Get-OslKeyVaultSecretValue `
        -VaultName $VaultName `
        -SecretName $Name `
        -AccessToken $AccessToken
    }
  }

  $manifestText = [string](& $SecretFetcher 'osl-test-account-manifest')
  if ([string]::IsNullOrWhiteSpace($manifestText) -or $manifestText.Length -gt 16384) {
    throw 'disposable Discord account manifest is unavailable or invalid'
  }
  try {
    $manifest = $manifestText | ConvertFrom-Json -ErrorAction Stop
  } catch {
    throw 'disposable Discord account manifest is invalid JSON'
  }
  $manifestText = $null

  $secretName = Resolve-OslDisposableDiscordSecretName `
    -Manifest $manifest `
    -ClientNumber $ClientNumber
  $credential = [string](& $SecretFetcher $secretName)
  try {
    if ([string]::IsNullOrWhiteSpace($credential) -or $credential.Length -gt 8192) {
      throw 'disposable Discord account credential is unavailable or invalid'
    }
    return [pscustomobject]@{
      Loaded = $true
      ClientNumber = $ClientNumber
    }
  } finally {
    $credential = $null
    $secretName = $null
  }
}

function disposable_discord_accounts_load_from_key_vault_without_logging_secrets {
  $requested = [System.Collections.Generic.List[string]]::new()
  $fixtureCredential = 'fixture-discord-credential-not-real'
  $fixtureHandle = 'fixture-user@example.invalid'
  $fetcher = {
    param([string]$Name)
    [void]$requested.Add($Name)
    if ($Name -ceq 'osl-test-account-manifest') {
      return (@{
        clients = @(
          @{
            clientNumber = 1
            service = 'discord'
            discordSecretName = 'osl-test-discord-02'
            publicLabel = $fixtureHandle
          },
          @{
            clientNumber = 2
            service = 'discord'
            discordSecretName = 'osl-test-discord-03'
          }
        )
      } | ConvertTo-Json -Compress -Depth 8)
    }
    if ($Name -ceq 'osl-test-discord-02') {
      return $fixtureCredential
    }
    throw 'unexpected secret request'
  }

  $output = @(
    Invoke-OslDisposableDiscordAccountLoad `
      -VaultName 'osl-test-secrets-a7d5d9' `
      -ClientNumber 1 `
      -AccessToken 'fixture-token' `
      -SecretFetcher $fetcher
  )
  $public = $output | ConvertTo-Json -Compress -Depth 8
  if (($requested -join '|') -cne 'osl-test-account-manifest|osl-test-discord-02') {
    throw 'disposable Discord account loader did not use the manifest-selected Key Vault secret'
  }
  if ($public -cmatch [regex]::Escape($fixtureCredential)) {
    throw 'disposable Discord credential was emitted'
  }
  if ($public -cmatch [regex]::Escape($fixtureHandle)) {
    throw 'disposable Discord account identifier was emitted'
  }
  if ($public -cmatch 'osl-test-discord-02') {
    throw 'disposable Discord Key Vault secret name was emitted'
  }
  if (@($output).Count -ne 1 -or $output[0].Loaded -ne $true -or [int]$output[0].ClientNumber -ne 1) {
    throw 'disposable Discord account load did not return the bounded public result'
  }

  $blankFetcher = {
    param([string]$Name)
    if ($Name -ceq 'osl-test-account-manifest') {
      return '{"client1":"osl-test-discord-02"}'
    }
    return ''
  }
  try {
    Invoke-OslDisposableDiscordAccountLoad `
      -VaultName 'osl-test-secrets-a7d5d9' `
      -ClientNumber 1 `
      -AccessToken 'fixture-token' `
      -SecretFetcher $blankFetcher | Out-Null
    throw 'blank disposable Discord credential was accepted'
  } catch {
    if ($_.Exception.Message -ceq 'blank disposable Discord credential was accepted') {
      throw
    }
    if ($_.Exception.Message -cnotmatch 'credential is unavailable or invalid') {
      throw
    }
  }
}

function Invoke-OslUnlockSelfTest {
  param([Parameter(Mandatory = $true)][string]$Name)
  switch ($Name) {
    'disposable_discord_accounts_load_from_key_vault_without_logging_secrets' {
      disposable_discord_accounts_load_from_key_vault_without_logging_secrets
      Write-Output "ok - $Name"
      return
    }
  }
}

if (-not [string]::IsNullOrWhiteSpace($SelfTest)) {
  Invoke-OslUnlockSelfTest -Name $SelfTest
  return
}

$missing = @()
if (-not $PSBoundParameters.ContainsKey('ClientNumber')) { $missing += 'ClientNumber' }
if (-not $PSBoundParameters.ContainsKey('OslExePath') -or [string]::IsNullOrWhiteSpace($OslExePath)) {
  $missing += 'OslExePath'
}
if (-not $PSBoundParameters.ContainsKey('OslExeSha256') -or [string]::IsNullOrWhiteSpace($OslExeSha256)) {
  $missing += 'OslExeSha256'
}
if (-not $PSBoundParameters.ContainsKey('SessionId')) { $missing += 'SessionId' }
if ($missing.Count -ne 0) {
  throw ('missing required parameter(s): ' + ($missing -join ', '))
}

$expectedPath = [IO.Path]::GetFullPath($OslExePath)
$expectedSha = $OslExeSha256.ToLowerInvariant()
if ($expectedSha -cnotmatch '^[0-9a-f]{64}$') { throw 'invalid OSL executable SHA-256' }
if (-not (Test-Path -LiteralPath $expectedPath -PathType Leaf)) { throw 'exact OSL executable is missing' }
if ((Get-FileHash -LiteralPath $expectedPath -Algorithm SHA256).Hash.ToLowerInvariant() -cne $expectedSha) {
  throw 'exact OSL executable hash mismatch'
}

$processes = @(Get-CimInstance Win32_Process -Filter "Name = 'OSL Privacy.exe'" | Where-Object {
  [int]$_.SessionId -eq $SessionId -and
  [IO.Path]::GetFullPath([string]$_.ExecutablePath) -ceq $expectedPath -and
  [string]$_.CommandLine -cnotmatch '(?:^|\s)--osl-borrowed-window-guardian-v1(?:\s|$)'
})
if ($processes.Count -ne 1) { throw 'exact running OSL process is unavailable or ambiguous' }

$explorers = @(Get-CimInstance Win32_Process -Filter "Name = 'explorer.exe'" | Where-Object {
  [int]$_.SessionId -eq $SessionId
})
if ($explorers.Count -ne 1) { throw 'interactive Explorer session is unavailable or ambiguous' }
$owner = Invoke-CimMethod -InputObject $explorers[0] -MethodName GetOwner
if ($owner.ReturnValue -ne 0 -or $owner.User -cne 'osltest' -or -not $owner.Domain) {
  throw 'interactive session owner is not exact osltest identity'
}
$interactiveUser = "$($owner.Domain)\$($owner.User)"

$vaultName = 'osl-test-secrets-a7d5d9'
$secretName = if ($ClientNumber -eq 1) {
  'osl-client-1-primary-password'
} else {
  'osl-client-2-primary-password'
}
$token = Get-OslKeyVaultAccessToken
$credential = Get-OslKeyVaultSecretValue `
  -VaultName $vaultName `
  -SecretName $secretName `
  -AccessToken $token
$disposableDiscordAccountLoad = Invoke-OslDisposableDiscordAccountLoad `
  -VaultName $vaultName `
  -ClientNumber $ClientNumber `
  -AccessToken $token
$disposableDiscordAccountLoaded = [bool]$disposableDiscordAccountLoad.Loaded
$disposableDiscordAccountLoad = $null
$token = $null
if ([string]::IsNullOrWhiteSpace($credential) -or $credential.Length -gt 1024) {
  throw 'unlock credential is unavailable or invalid'
}

$root = 'C:\ProgramData\OSL-QA\unlock'
[void](New-Item -ItemType Directory -Path $root -Force)
$nonce = [Guid]::NewGuid().ToString('N')
$pipeName = "OSL-QA-Unlock-$nonce"
$taskName = "OSL-QA-Unlock-$nonce"
$scriptPath = Join-Path $root "$nonce.ps1"
$resultPath = Join-Path $root "$nonce.json"
$escapedPath = $expectedPath.Replace("'", "''")
$userScript = @"
param([string]`$PipeName,[string]`$ResultPath)
`$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
`$secretBytes = `$null
`$secret = `$null
try {
  `$pipe = [IO.Pipes.NamedPipeClientStream]::new('.', `$PipeName, [IO.Pipes.PipeDirection]::In)
  try {
    `$pipe.Connect(15000)
    `$reader = [IO.BinaryReader]::new(`$pipe, [Text.Encoding]::UTF8, `$true)
    `$length = `$reader.ReadInt32()
    if (`$length -lt 1 -or `$length -gt 4096) { throw 'credential handoff rejected' }
    `$secretBytes = `$reader.ReadBytes(`$length)
    if (`$secretBytes.Length -ne `$length) { throw 'credential handoff incomplete' }
    `$secret = [Text.Encoding]::UTF8.GetString(`$secretBytes)
    `$reader.Dispose()
  } finally {
    `$pipe.Dispose()
  }

  `$mainCandidates = @(Get-CimInstance Win32_Process -Filter "Name = 'OSL Privacy.exe'" | Where-Object {
    [int]`$_.SessionId -eq $SessionId -and `$_.ExecutablePath -and
    [IO.Path]::GetFullPath([string]`$_.ExecutablePath) -ceq '$escapedPath' -and
    [string]`$_.CommandLine -cnotmatch '(?:^|\s)--osl-borrowed-window-guardian-v1(?:\s|$)'
  })
  `$matches = @(`$mainCandidates | ForEach-Object { Get-Process -Id ([int]`$_.ProcessId) -ErrorAction Stop } | Where-Object {
    `$_.MainWindowHandle -ne 0
  })
  if (`$matches.Count -ne 1) { throw 'exact interactive OSL window is unavailable or ambiguous' }
  `$actualSha = (Get-FileHash -LiteralPath `$matches[0].Path -Algorithm SHA256).Hash.ToLowerInvariant()
  if (`$actualSha -cne '$expectedSha') { throw 'interactive OSL hash mismatch' }
  `$rootElement = [Windows.Automation.AutomationElement]::FromHandle(`$matches[0].MainWindowHandle)
  if (-not `$rootElement) { throw 'exact OSL automation root unavailable' }

  `$passwordCondition = [Windows.Automation.AndCondition]::new([Windows.Automation.Condition[]]@(
    [Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::ControlTypeProperty, [Windows.Automation.ControlType]::Edit),
    [Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::AutomationIdProperty, 'identity-password')
  ))
  `$unlockCondition = [Windows.Automation.AndCondition]::new([Windows.Automation.Condition[]]@(
    [Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::ControlTypeProperty, [Windows.Automation.ControlType]::Button),
    [Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::AutomationIdProperty, 'identity-password-submit')
  ))
  `$passwords = @(`$rootElement.FindAll([Windows.Automation.TreeScope]::Descendants, `$passwordCondition) | Where-Object {
    -not `$_.Current.IsOffscreen -and -not `$_.Current.BoundingRectangle.IsEmpty
  })
  `$unlocks = @(`$rootElement.FindAll([Windows.Automation.TreeScope]::Descendants, `$unlockCondition) | Where-Object {
    -not `$_.Current.IsOffscreen -and -not `$_.Current.BoundingRectangle.IsEmpty
  })
  if (`$passwords.Count -ne 1 -or `$unlocks.Count -ne 1) { throw 'exact unlock controls are unavailable or ambiguous' }
  if (-not `$passwords[0].Current.IsEnabled) { throw 'exact password control is disabled' }
  `$passwords[0].GetCurrentPattern([Windows.Automation.ValuePattern]::Pattern).SetValue(`$secret)
  `$secret = `$null
  [Array]::Clear(`$secretBytes, 0, `$secretBytes.Length)
  `$secretBytes = `$null
  `$enableDeadline = [DateTime]::UtcNow.AddSeconds(5)
  do {
    Start-Sleep -Milliseconds 50
    `$unlocks = @(`$rootElement.FindAll([Windows.Automation.TreeScope]::Descendants, `$unlockCondition) | Where-Object {
      -not `$_.Current.IsOffscreen -and -not `$_.Current.BoundingRectangle.IsEmpty
    })
    if (`$unlocks.Count -eq 1 -and `$unlocks[0].Current.IsEnabled) { break }
  } while ([DateTime]::UtcNow -lt `$enableDeadline)
  if (`$unlocks.Count -ne 1 -or -not `$unlocks[0].Current.IsEnabled) {
    throw 'exact unlock action did not enable'
  }
  `$unlocks[0].GetCurrentPattern([Windows.Automation.InvokePattern]::Pattern).Invoke()

  `$readyIds = @('skip-onboarding','skip-scrub-setup','home-app-discord','discord-existing-session','native-companion-focus')
  `$deadline = [DateTime]::UtcNow.AddSeconds(20)
  `$stableReadySamples = 0
  `$readyId = ''
  do {
    Start-Sleep -Milliseconds 100
    `$remaining = @(`$rootElement.FindAll([Windows.Automation.TreeScope]::Descendants, `$unlockCondition)).Count
    `$visibleReady = @()
    foreach (`$candidateId in `$readyIds) {
      `$condition = [Windows.Automation.PropertyCondition]::new(
        [Windows.Automation.AutomationElement]::AutomationIdProperty, `$candidateId)
      `$visibleReady += @(`$rootElement.FindAll([Windows.Automation.TreeScope]::Descendants, `$condition) | Where-Object {
        -not `$_.Current.IsOffscreen -and -not `$_.Current.BoundingRectangle.IsEmpty -and `$_.Current.IsEnabled
      })
    }
    if (`$remaining -eq 0 -and `$visibleReady.Count -eq 1) {
      `$candidateReadyId = [string]`$visibleReady[0].Current.AutomationId
      if (`$candidateReadyId -ceq `$readyId) { `$stableReadySamples++ } else { `$readyId = `$candidateReadyId; `$stableReadySamples = 1 }
      if (`$stableReadySamples -ge 2) { break }
    } else {
      `$stableReadySamples = 0
      `$readyId = ''
    }
  } while ([DateTime]::UtcNow -lt `$deadline)
  if (`$remaining -ne 0 -or `$stableReadySamples -lt 2) { throw 'unlock transition did not reach a stable ready route' }
  `$result = @{ Status = 'unlocked'; ReadyAutomationId = `$readyId; StableReadySamples = `$stableReadySamples }
} catch {
  `$result = @{ Status = 'failed'; Detail = if (`$_.Exception.Message.Length -le 120) { `$_.Exception.Message } else { `$_.Exception.Message.Substring(0,120) } }
} finally {
  `$secret = `$null
  if (`$secretBytes) { [Array]::Clear(`$secretBytes, 0, `$secretBytes.Length) }
}
[IO.File]::WriteAllText(`$ResultPath, (`$result | ConvertTo-Json -Compress), [Text.UTF8Encoding]::new(`$false))
"@
[IO.File]::WriteAllText($scriptPath, $userScript, [Text.UTF8Encoding]::new($false))

$server = [IO.Pipes.NamedPipeServerStream]::new(
  $pipeName, [IO.Pipes.PipeDirection]::Out, 1,
  [IO.Pipes.PipeTransmissionMode]::Byte, [IO.Pipes.PipeOptions]::Asynchronous)
$taskAction = New-ScheduledTaskAction -Execute 'powershell.exe' -Argument (
  '-NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "{0}" -PipeName "{1}" -ResultPath "{2}"' -f
    $scriptPath, $pipeName, $resultPath
)
$principal = New-ScheduledTaskPrincipal -UserId $interactiveUser -LogonType Interactive -RunLevel Limited
$settings = New-ScheduledTaskSettingsSet -ExecutionTimeLimit ([TimeSpan]::FromMinutes(1)) -MultipleInstances IgnoreNew
Register-ScheduledTask -TaskName $taskName -Action $taskAction -Principal $principal -Settings $settings | Out-Null

$credentialBytes = $null
try {
  Start-ScheduledTask -TaskName $taskName
  $connection = $server.BeginWaitForConnection($null, $null)
  try {
    if (-not $connection.AsyncWaitHandle.WaitOne(15000)) {
      throw 'interactive unlock task did not connect to the credential pipe'
    }
    $server.EndWaitForConnection($connection)
  } finally {
    $connection.AsyncWaitHandle.Close()
  }
  $writer = [IO.BinaryWriter]::new($server, [Text.Encoding]::UTF8, $true)
  $credentialBytes = [Text.Encoding]::UTF8.GetBytes($credential)
  $credential = $null
  $writer.Write([int]$credentialBytes.Length)
  $writer.Write($credentialBytes)
  $writer.Flush()
  $writer.Dispose()
  [Array]::Clear($credentialBytes, 0, $credentialBytes.Length)
  $credentialBytes = $null
  $server.Dispose()

  $deadline = [DateTime]::UtcNow.AddSeconds(30)
  while (-not (Test-Path -LiteralPath $resultPath -PathType Leaf)) {
    if ([DateTime]::UtcNow -gt $deadline) { throw 'interactive unlock timed out' }
    Start-Sleep -Milliseconds 200
  }
  $result = Get-Content -LiteralPath $resultPath -Raw | ConvertFrom-Json -ErrorAction Stop
  if ($result.Status -cne 'unlocked') { throw "interactive unlock failed: $($result.Detail)" }
  [pscustomobject]@{
    Status = 'unlocked'
    ClientNumber = $ClientNumber
    ProfilesPreserved = $true
    DisposableDiscordAccountLoaded = $disposableDiscordAccountLoaded
  } |
    ConvertTo-Json -Compress
} finally {
  $credential = $null
  if ($credentialBytes) { [Array]::Clear($credentialBytes, 0, $credentialBytes.Length) }
  $server.Dispose()
  Unregister-ScheduledTask -TaskName $taskName -Confirm:$false -ErrorAction SilentlyContinue
  Remove-Item -LiteralPath $scriptPath,$resultPath -Force -ErrorAction SilentlyContinue
}
