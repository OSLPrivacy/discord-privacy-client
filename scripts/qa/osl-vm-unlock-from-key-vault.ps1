param(
  [Parameter(Mandatory = $true, ParameterSetName = 'Run')]
  [ValidateSet(1, 2)]
  [int]$ClientNumber,

  [Parameter(Mandatory = $true, ParameterSetName = 'Run')]
  [string]$OslExePath,

  [Parameter(Mandatory = $true, ParameterSetName = 'Run')]
  [string]$OslExeSha256,

  [Parameter(Mandatory = $true, ParameterSetName = 'Run')]
  [int]$SessionId,

  [Parameter(Mandatory = $true, ParameterSetName = 'SelfTest')]
  [switch]$RunInternalSelfTest
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

function Assert-OslTestSecretsVault([string]$VaultName) {
  if ([string]::IsNullOrWhiteSpace($VaultName) -or
      $VaultName -cnotmatch '^osl-test-secrets-[a-z0-9]+$') {
    throw 'test credential vault is outside the OSL test-secrets boundary'
  }
}

function Get-OslKeyVaultSecretValue {
  param(
    [Parameter(Mandatory = $true)][string]$VaultName,
    [Parameter(Mandatory = $true)][string]$SecretName,
    [Parameter(Mandatory = $true)][string]$AccessToken
  )

  Assert-OslTestSecretsVault $VaultName
  if ($SecretName -cnotmatch '^(osl-client-[12]-primary-password|osl-test-account-manifest|osl-test-discord-0[1-3])$') {
    throw 'requested test secret is outside the OSL disposable account allow-list'
  }
  $secretUri = "https://$VaultName.vault.azure.net/secrets/$SecretName`?api-version=7.4"
  try {
    $secretRecord = Invoke-RestMethod -Method Get -Uri $secretUri -Headers @{
      Authorization = "Bearer $AccessToken"
    } -TimeoutSec 15
    return [string]$secretRecord.value
  } catch {
    throw 'Key Vault test secret retrieval failed'
  } finally {
    $secretRecord = $null
  }
}

function Resolve-OslDisposableDiscordSecretName {
  param(
    [Parameter(Mandatory = $true)][ValidateSet(1, 2)][int]$ClientNumber,
    [Parameter(Mandatory = $true)][string]$ManifestJson
  )

  try {
    $manifest = $ManifestJson | ConvertFrom-Json -ErrorAction Stop
  } catch {
    throw 'disposable account manifest is invalid'
  }

  $clientKey = [string]$ClientNumber
  $candidate = $null
  if ($manifest.PSObject.Properties.Name -contains 'clients' -and $manifest.clients) {
    $clientProperty = $manifest.clients.PSObject.Properties[$clientKey]
    if ($clientProperty) { $candidate = [string]$clientProperty.Value }
  }
  if (-not $candidate -and $manifest.PSObject.Properties.Name -contains 'discord' -and $manifest.discord) {
    $discordProperty = $manifest.discord.PSObject.Properties[$clientKey]
    if ($discordProperty) { $candidate = [string]$discordProperty.Value }
  }
  if (-not $candidate -and $manifest.PSObject.Properties.Name -contains 'assignments') {
    foreach ($assignment in @($manifest.assignments)) {
      $service = if ($assignment.PSObject.Properties.Name -contains 'service') { [string]$assignment.service } else { 'discord' }
      $assignedClient = $null
      foreach ($field in @('clientNumber', 'clientId', 'client')) {
        if ($assignment.PSObject.Properties.Name -contains $field) {
          $assignedClient = [string]$assignment.$field
          break
        }
      }
      if ($service -cne 'discord' -or $assignedClient -cne $clientKey) { continue }
      foreach ($field in @('secretName', 'accountSecretName', 'keyVaultSecretName', 'secret')) {
        if ($assignment.PSObject.Properties.Name -contains $field) {
          $candidate = [string]$assignment.$field
          break
        }
      }
      if ($candidate) { break }
    }
  }

  if ($candidate -cnotmatch '^osl-test-discord-0[1-3]$') {
    throw 'disposable Discord account assignment is unavailable'
  }
  return $candidate
}

function Invoke-WithOslDisposableDiscordCredential {
  param(
    [Parameter(Mandatory = $true)][ValidateSet(1, 2)][int]$ClientNumber,
    [Parameter(Mandatory = $true)][string]$VaultName,
    [Parameter(Mandatory = $true)][scriptblock]$GetSecret,
    [Parameter(Mandatory = $true)][scriptblock]$UseCredential
  )

  Assert-OslTestSecretsVault $VaultName
  $manifestJson = [string](& $GetSecret $VaultName 'osl-test-account-manifest')
  if ([string]::IsNullOrWhiteSpace($manifestJson) -or $manifestJson.Length -gt 8192) {
    throw 'disposable account manifest is unavailable or invalid'
  }
  $accountSecretName = Resolve-OslDisposableDiscordSecretName -ClientNumber $ClientNumber -ManifestJson $manifestJson
  $credential = [string](& $GetSecret $VaultName $accountSecretName)
  try {
    if ([string]::IsNullOrWhiteSpace($credential) -or $credential.Length -gt 4096) {
      throw 'disposable Discord credential is unavailable or invalid'
    }
    $consumerAccepted = & $UseCredential $credential
    if ($consumerAccepted -eq $false) {
      throw 'disposable Discord credential consumer refused'
    }
    return [pscustomobject]@{
      Status = 'loaded'
      Service = 'discord'
      ClientNumber = $ClientNumber
      SecretValueExposed = $false
    }
  } finally {
    $credential = $null
    $accountSecretName = $null
    $manifestJson = $null
  }
}

function disposable_discord_accounts_load_from_key_vault_without_logging_secrets {
  $vault = 'osl-test-secrets-a7d5d9'
  $manifest = '{"clients":{"1":"osl-test-discord-01","2":"osl-test-discord-02"}}'
  $secretValue = 'discord-login@example.invalid|not-a-real-password'
  $requested = New-Object System.Collections.Generic.List[string]
  $observedCredential = $null
  $getSecret = {
    param([string]$RequestedVault, [string]$RequestedSecret)
    if ($RequestedVault -cne $vault) { throw 'wrong vault' }
    [void]$requested.Add($RequestedSecret)
    switch -CaseSensitive ($RequestedSecret) {
      'osl-test-account-manifest' { return $manifest }
      'osl-test-discord-01' { return $secretValue }
      default { throw 'unexpected secret request' }
    }
  }
  $receipt = Invoke-WithOslDisposableDiscordCredential `
    -ClientNumber 1 `
    -VaultName $vault `
    -GetSecret $getSecret `
    -UseCredential { param([string]$Credential) $script:__oslVmUnlockSelfTestCredential = $Credential; return $true }
  $rendered = $receipt | ConvertTo-Json -Compress
  $observedCredential = $script:__oslVmUnlockSelfTestCredential
  $script:__oslVmUnlockSelfTestCredential = $null
  if ($observedCredential -cne $secretValue) { throw 'disposable credential was not handed to the consumer' }
  if (($requested -join ',') -cne 'osl-test-account-manifest,osl-test-discord-01') {
    throw 'disposable account manifest and assigned secret were not both loaded'
  }
  if ($rendered.Contains($secretValue) -or $rendered.Contains('osl-test-discord-01')) {
    throw 'disposable account receipt exposed secret material'
  }

  $badManifest = '{"clients":{"1":"personal-discord-account"}}'
  $badGetSecret = {
    param([string]$RequestedVault, [string]$RequestedSecret)
    if ($RequestedSecret -ceq 'osl-test-account-manifest') { return $badManifest }
    throw 'bad manifest must not fetch an account credential'
  }
  $refused = $false
  try {
    Invoke-WithOslDisposableDiscordCredential `
      -ClientNumber 1 `
      -VaultName $vault `
      -GetSecret $badGetSecret `
      -UseCredential { param([string]$Credential) throw 'bad manifest reached credential consumer' } | Out-Null
  } catch {
    $refused = $_.Exception.Message -ceq 'disposable Discord account assignment is unavailable'
  }
  if (-not $refused) { throw 'non-disposable Discord account assignment was not refused' }
  return $true
}

if ($RunInternalSelfTest) {
  [void](disposable_discord_accounts_load_from_key_vault_without_logging_secrets)
  [pscustomobject]@{ Status = 'passed'; Test = 'disposable_discord_accounts_load_from_key_vault_without_logging_secrets' } |
    ConvertTo-Json -Compress
  return
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
$tokenUri = 'http://169.254.169.254/metadata/identity/oauth2/token' +
  '?api-version=2019-08-01&resource=https%3A%2F%2Fvault.azure.net'
$token = Invoke-RestMethod -Method Get -Uri $tokenUri -Headers @{ Metadata = 'true' } -TimeoutSec 10
if ([string]::IsNullOrWhiteSpace([string]$token.access_token)) { throw 'managed identity unavailable' }
$accessToken = [string]$token.access_token
$token = $null
$credential = Get-OslKeyVaultSecretValue -VaultName $vaultName -SecretName $secretName -AccessToken $accessToken
if ([string]::IsNullOrWhiteSpace($credential) -or $credential.Length -gt 1024) {
  throw 'unlock credential is unavailable or invalid'
}
$discordLoadReceipt = Invoke-WithOslDisposableDiscordCredential `
  -ClientNumber $ClientNumber `
  -VaultName $vaultName `
  -GetSecret { param([string]$RequestedVault, [string]$RequestedSecret) Get-OslKeyVaultSecretValue -VaultName $RequestedVault -SecretName $RequestedSecret -AccessToken $accessToken } `
  -UseCredential { param([string]$Credential) return ($Credential.Length -gt 0) }
$accessToken = $null
if ($discordLoadReceipt.Status -cne 'loaded') {
  throw 'disposable Discord credential did not load'
}
$discordLoadReceipt = $null

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
  [pscustomobject]@{ Status = 'unlocked'; ClientNumber = $ClientNumber; ProfilesPreserved = $true } |
    ConvertTo-Json -Compress
} finally {
  $credential = $null
  if ($credentialBytes) { [Array]::Clear($credentialBytes, 0, $credentialBytes.Length) }
  $server.Dispose()
  Unregister-ScheduledTask -TaskName $taskName -Confirm:$false -ErrorAction SilentlyContinue
  Remove-Item -LiteralPath $scriptPath,$resultPath -Force -ErrorAction SilentlyContinue
}
