param(
  [ValidateRange(1, 60)]
  [int]$PollSeconds = 3,

  [switch]$RunOnce,

  [string]$LocalBootstrapManifestPath
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
Add-Type -AssemblyName System.Net.Http

$storageAccount = 'osltestartifactsa7d5'
$targetSessionId = 2
$interactiveUserName = 'osltest'
$oslExePath = 'C:\Users\osltest\Desktop\OSL Privacy\OSL Privacy.exe'
$installRoot = [IO.Path]::GetDirectoryName($oslExePath)
$loaderPath = Join-Path $installRoot 'WebView2Loader.dll'
$root = 'C:\ProgramData\OSL-QA\fast-updater'
$configPath = Join-Path $root 'config.json'
$bootstrapManifestPath = Join-Path $root 'bootstrap-manifest.json'
$statePath = Join-Path $root 'state.json'
$journalPath = Join-Path $root 'transaction.json'
$resultsRoot = Join-Path $root 'results'
$manifestMaxBytes = 16384
$discordNames = @('Discord.exe', 'DiscordPTB.exe', 'DiscordCanary.exe')

function Assert-RelativeBlobName([string]$Name, [string]$RequiredPrefix) {
  if ($Name -cnotmatch '^[a-z0-9][a-z0-9._/-]{1,199}$' -or
      $Name.Contains('..') -or
      $Name.Contains('\') -or
      $Name.Contains('?') -or
      -not $Name.StartsWith($RequiredPrefix, [StringComparison]::Ordinal)) {
    throw 'relative blob name is invalid'
  }
}

function Get-Sha256([string]$Path) {
  return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
}

function Get-BytesSha256([byte[]]$Bytes) {
  $sha = [Security.Cryptography.SHA256]::Create()
  try {
    return ([BitConverter]::ToString($sha.ComputeHash($Bytes))).Replace('-', '').ToLowerInvariant()
  } finally {
    $sha.Dispose()
  }
}

function Write-AtomicJson([string]$Path, [object]$Value) {
  $temporary = "$Path.tmp"
  $json = $Value | ConvertTo-Json -Depth 12 -Compress
  [IO.File]::WriteAllText($temporary, $json, [Text.UTF8Encoding]::new($false))
  if (Test-Path -LiteralPath $Path -PathType Leaf) {
    [IO.File]::Replace($temporary, $Path, $null, $true)
  } else {
    [IO.File]::Move($temporary, $Path)
  }
}

function Read-JsonFile([string]$Path) {
  return Get-Content -LiteralPath $Path -Raw | ConvertFrom-Json -ErrorAction Stop
}

function Get-ClosedClientMapping([string]$MachineName) {
  switch -CaseSensitive ($MachineName) {
    'OSL-Azure-Client-1' {
      return [pscustomobject]@{
        schemaVersion = 1
        clientId = 1
        storageAccount = 'osltestartifactsa7d5'
        storageContainer = 'osl-discord-qa-client1'
        targetMachine = 'OSL-Azure-Client-1'
        manifestBlobName = 'manifests/c1-fast-qa.json'
        launcherTaskName = 'OSL-QA-FastUpdater-Launch-C1'
        pollerTaskName = 'OSL-QA-FastUpdater-Poller-C1'
        resultPrefix = 'results/fast-updater/c1/'
        sessionId = 2
        interactiveUserName = 'osltest'
        oslExePath = 'C:\Users\osltest\Desktop\OSL Privacy\OSL Privacy.exe'
      }
    }
    'OSL-Azure-Client-2' {
      return [pscustomobject]@{
        schemaVersion = 1
        clientId = 2
        storageAccount = 'osltestartifactsa7d5'
        storageContainer = 'osl-discord-qa-client2'
        targetMachine = 'OSL-Azure-Client-2'
        manifestBlobName = 'manifests/c2-fast-qa.json'
        launcherTaskName = 'OSL-QA-FastUpdater-Launch-C2'
        pollerTaskName = 'OSL-QA-FastUpdater-Poller-C2'
        resultPrefix = 'results/fast-updater/c2/'
        sessionId = 2
        interactiveUserName = 'osltest'
        oslExePath = 'C:\Users\osltest\Desktop\OSL Privacy\OSL Privacy.exe'
      }
    }
    default { throw 'Azure VM name is outside the exact closed client mapping' }
  }
}

function Get-ValidatedClientConfig {
  if (-not (Test-Path -LiteralPath $configPath -PathType Leaf)) {
    throw 'protected updater config is absent'
  }
  $config = Read-JsonFile $configPath
  $propertyNames = @($config.PSObject.Properties.Name | Sort-Object)
  $requiredNames = @(
    'clientId',
    'interactiveUserName',
    'launcherTaskName',
    'manifestBlobName',
    'oslExePath',
    'pollerTaskName',
    'resultPrefix',
    'schemaVersion',
    'sessionId',
    'storageAccount',
    'storageContainer',
    'targetMachine'
  ) | Sort-Object
  if ($propertyNames.Count -ne $requiredNames.Count -or
      (Compare-Object -ReferenceObject $requiredNames -DifferenceObject $propertyNames)) {
    throw 'protected updater config shape is invalid'
  }
  $expected = Get-ClosedClientMapping ([string]$config.targetMachine)
  foreach ($name in $requiredNames) {
    if ([string]$config.$name -cne [string]$expected.$name) {
      throw "protected updater config mapping mismatch: $name"
    }
  }
  return $expected
}

$clientConfig = Get-ValidatedClientConfig
$storageContainer = [string]$clientConfig.storageContainer
$targetMachine = [string]$clientConfig.targetMachine
$ManifestBlobName = [string]$clientConfig.manifestBlobName
$launcherTaskName = [string]$clientConfig.launcherTaskName
$resultPrefix = [string]$clientConfig.resultPrefix
Assert-RelativeBlobName $ManifestBlobName 'manifests/'
Assert-RelativeBlobName $resultPrefix 'results/fast-updater/'
$artifactHost = "$storageAccount.blob.core.windows.net"
$containerRoot = "https://$artifactHost/$storageContainer"
$manifestUri = "$containerRoot/$ManifestBlobName"

function Get-StorageToken {
  $tokenUri = 'http://169.254.169.254/metadata/identity/oauth2/token' +
    '?api-version=2018-02-01&resource=https%3A%2F%2Fstorage.azure.com%2F'
  $token = Invoke-RestMethod -Method Get -Uri $tokenUri -Headers @{ Metadata = 'true' } -TimeoutSec 10
  if ([string]::IsNullOrWhiteSpace([string]$token.access_token)) {
    throw 'managed identity storage token unavailable'
  }
  return [string]$token.access_token
}

function New-StorageClient {
  $client = [Net.Http.HttpClient]::new()
  $client.Timeout = [TimeSpan]::FromSeconds(30)
  $client.DefaultRequestHeaders.Authorization =
    [Net.Http.Headers.AuthenticationHeaderValue]::new('Bearer', (Get-StorageToken))
  $client.DefaultRequestHeaders.Add('x-ms-version', '2023-11-03')
  return $client
}

function Get-Manifest([string]$PriorEtag) {
  $client = New-StorageClient
  try {
    $request = [Net.Http.HttpRequestMessage]::new([Net.Http.HttpMethod]::Get, $manifestUri)
    if (-not [string]::IsNullOrWhiteSpace($PriorEtag)) {
      [void]$request.Headers.TryAddWithoutValidation('If-None-Match', $PriorEtag)
    }
    $response = $client.SendAsync($request).GetAwaiter().GetResult()
    if ([int]$response.StatusCode -eq 304) {
      return [pscustomobject]@{ NotModified = $true }
    }
    if (-not $response.IsSuccessStatusCode) {
      throw "manifest fetch failed with HTTP $([int]$response.StatusCode)"
    }
    if ($null -eq $response.Content.Headers.ContentLength) {
      throw 'manifest response size is absent'
    }
    if ([int64]$response.Content.Headers.ContentLength -gt $manifestMaxBytes) {
      throw 'manifest exceeds the bounded size'
    }
    $bytes = $response.Content.ReadAsByteArrayAsync().GetAwaiter().GetResult()
    if ($bytes.Length -eq 0 -or $bytes.Length -gt $manifestMaxBytes) {
      throw 'manifest size is invalid'
    }
    $etag = [string]$response.Headers.ETag
    if ([string]::IsNullOrWhiteSpace($etag)) { throw 'manifest ETag is absent' }
    return [pscustomobject]@{
      NotModified = $false
      Etag = $etag
      Bytes = $bytes
      Sha256 = Get-BytesSha256 $bytes
    }
  } finally {
    $client.Dispose()
  }
}

function Save-Artifact([string]$BlobName, [string]$Destination, [int64]$ExpectedSize, [string]$ExpectedSha256) {
  Assert-RelativeBlobName $BlobName 'builds/qa/'
  $uri = "$containerRoot/$BlobName"
  $temporary = "$Destination.download"
  $client = New-StorageClient
  try {
    $response = $client.GetAsync($uri).GetAwaiter().GetResult()
    if (-not $response.IsSuccessStatusCode) {
      throw "artifact fetch failed with HTTP $([int]$response.StatusCode)"
    }
    if ($null -eq $response.Content.Headers.ContentLength) {
      throw 'artifact response size is absent'
    }
    if ([int64]$response.Content.Headers.ContentLength -ne $ExpectedSize) {
      throw 'artifact response size mismatch'
    }
    $bytes = $response.Content.ReadAsByteArrayAsync().GetAwaiter().GetResult()
    if ($bytes.LongLength -ne $ExpectedSize) { throw 'downloaded artifact size mismatch' }
    [IO.File]::WriteAllBytes($temporary, $bytes)
    if ((Get-Sha256 $temporary) -cne $ExpectedSha256) {
      throw 'downloaded artifact hash mismatch'
    }
    [IO.File]::Move($temporary, $Destination)
  } finally {
    $client.Dispose()
    if (Test-Path -LiteralPath $temporary) {
      Remove-Item -LiteralPath $temporary -Force
    }
  }
}

function Upload-Result([string]$BlobName, [string]$LocalPath) {
  Assert-RelativeBlobName $BlobName $resultPrefix
  $client = New-StorageClient
  try {
    $client.DefaultRequestHeaders.Add('x-ms-blob-type', 'BlockBlob')
    $bytes = [IO.File]::ReadAllBytes($LocalPath)
    $content = [Net.Http.ByteArrayContent]::new($bytes)
    $content.Headers.ContentType = [Net.Http.Headers.MediaTypeHeaderValue]::new('application/json')
    $response = $client.PutAsync("$containerRoot/$BlobName", $content).GetAwaiter().GetResult()
    if (-not $response.IsSuccessStatusCode) {
      throw "result upload failed with HTTP $([int]$response.StatusCode)"
    }
  } finally {
    $client.Dispose()
  }
}

function Get-LocalVmName {
  return ([string](Invoke-RestMethod -Method Get `
    -Uri 'http://169.254.169.254/metadata/instance/compute/name?api-version=2021-02-01&format=text' `
    -Headers @{ Metadata = 'true' } -TimeoutSec 10)).Trim()
}

function Get-ExactOslProcesses {
  return @(Get-CimInstance Win32_Process -Filter "Name = 'OSL Privacy.exe'" | Where-Object {
    [int]$_.SessionId -eq $targetSessionId -and
    $_.ExecutablePath -and
    [string]::Equals(
      [IO.Path]::GetFullPath([string]$_.ExecutablePath),
      $oslExePath,
      [StringComparison]::OrdinalIgnoreCase
    )
  })
}

function Get-DiscordFingerprint {
  $rows = @(Get-CimInstance Win32_Process | Where-Object {
    $discordNames -ccontains [string]$_.Name
  } | ForEach-Object {
    if ([string]::IsNullOrWhiteSpace([string]$_.ExecutablePath) -or $null -eq $_.CreationDate) {
      throw 'Discord process fingerprint is incomplete'
    }
    [pscustomobject]@{
      Name = [string]$_.Name
      Pid = [int]$_.ProcessId
      Path = [IO.Path]::GetFullPath([string]$_.ExecutablePath)
      SessionId = [int]$_.SessionId
      Start = ([DateTime]$_.CreationDate).ToUniversalTime().ToString('o')
    }
  } | Sort-Object Name, Path, SessionId, Pid)
  return $rows
}

function Get-FingerprintKey([object[]]$Fingerprint) {
  if ($Fingerprint.Count -eq 0) { return '[]' }
  return ($Fingerprint | ConvertTo-Json -Depth 4 -Compress)
}

function Assert-LauncherTask {
  $task = Get-ScheduledTask -TaskName $launcherTaskName -ErrorAction Stop
  $actions = @($task.Actions)
  if ($actions.Count -ne 1 -or
      -not [string]::Equals([string]$actions[0].Execute, $oslExePath, [StringComparison]::OrdinalIgnoreCase) -or
      -not [string]::Equals([string]$actions[0].WorkingDirectory, $installRoot, [StringComparison]::OrdinalIgnoreCase) -or
      [string]$task.Principal.LogonType -cne 'Interactive' -or
      [string]$task.Principal.RunLevel -cne 'Limited' -or
      -not ([string]$task.Principal.UserId).EndsWith("\$interactiveUserName", [StringComparison]::OrdinalIgnoreCase)) {
    throw 'fixed Interactive/Limited OSL launcher task identity mismatch'
  }
}

function Assert-InteractiveSession {
  $explorers = @(Get-CimInstance Win32_Process -Filter "Name = 'explorer.exe'" | Where-Object {
    [int]$_.SessionId -eq $targetSessionId
  })
  if ($explorers.Count -ne 1) { throw 'interactive Explorer session is unavailable or ambiguous' }
  $owner = Invoke-CimMethod -InputObject $explorers[0] -MethodName GetOwner
  if ($owner.ReturnValue -ne 0 -or $owner.User -cne $interactiveUserName -or -not $owner.Domain) {
    throw 'interactive session owner is not exact osltest identity'
  }
}

function Start-ExactOsl([string]$ExpectedSha256, [int]$TimeoutSeconds = 45) {
  if ((Get-Sha256 $oslExePath) -cne $ExpectedSha256) {
    throw 'launch executable hash mismatch'
  }
  Assert-LauncherTask
  $readyDeadline = [DateTime]::UtcNow.AddSeconds(5)
  while ([string](Get-ScheduledTask -TaskName $launcherTaskName).State -eq 'Running' -and
         [DateTime]::UtcNow -lt $readyDeadline) {
    Start-Sleep -Milliseconds 200
  }
  if ([string](Get-ScheduledTask -TaskName $launcherTaskName).State -eq 'Running') {
    throw 'fixed OSL launcher task did not return to a launchable state'
  }
  Start-ScheduledTask -TaskName $launcherTaskName
  $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
  do {
    $running = @(Get-ExactOslProcesses)
    if ($running.Count -eq 1) {
      if ((Get-Sha256 $oslExePath) -cne $ExpectedSha256) {
        throw 'running executable bytes changed after launch'
      }
      return $running[0]
    }
    if ($running.Count -gt 1) { throw 'exact OSL process launch is ambiguous' }
    Start-Sleep -Milliseconds 250
  } while ([DateTime]::UtcNow -lt $deadline)
  throw 'exact OSL process did not launch within the bounded deadline'
}

function Stop-ExactOsl([object]$ExpectedProcess, [int]$TimeoutSeconds = 20) {
  $pidValue = [int]$ExpectedProcess.ProcessId
  $current = @(Get-CimInstance Win32_Process -Filter "ProcessId = $pidValue" | Where-Object {
    [int]$_.SessionId -eq $targetSessionId -and
    $_.ExecutablePath -and
    [string]::Equals([IO.Path]::GetFullPath([string]$_.ExecutablePath), $oslExePath, [StringComparison]::OrdinalIgnoreCase) -and
    ([DateTime]$_.CreationDate).ToUniversalTime() -eq ([DateTime]$ExpectedProcess.CreationDate).ToUniversalTime()
  })
  if ($current.Count -ne 1) { throw 'exact OSL PID changed before stop' }
  Stop-Process -Id $pidValue -Force -ErrorAction Stop
  $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
  while (@(Get-ExactOslProcesses).Count -ne 0 -and [DateTime]::UtcNow -lt $deadline) {
    Start-Sleep -Milliseconds 200
  }
  if (@(Get-ExactOslProcesses).Count -ne 0) {
    throw 'exact OSL process did not stop within the bounded deadline'
  }
}

function Assert-HexSha256([string]$Value, [string]$Label) {
  if ($Value -cnotmatch '^[0-9a-f]{64}$') { throw "$Label SHA-256 is invalid" }
}

function ConvertTo-ValidatedManifest([byte[]]$Bytes, [int64]$LastGeneration) {
  $raw = [Text.UTF8Encoding]::new($false, $true).GetString($Bytes)
  $manifest = $raw | ConvertFrom-Json -ErrorAction Stop
  if ([int]$manifest.schemaVersion -ne 1) { throw 'manifest schema version is invalid' }
  if ([string]$manifest.targetMachine -cne $targetMachine) { throw 'manifest target machine mismatch' }
  if ([int]$manifest.sessionId -ne $targetSessionId) { throw 'manifest target session mismatch' }
  if ([int64]$manifest.generation -le $LastGeneration) { throw 'manifest generation is not monotonic' }
  if ([string]$manifest.invocationId -cnotmatch '^[a-z0-9][a-z0-9-]{7,63}$') {
    throw 'manifest invocation ID is invalid'
  }
  foreach ($pair in @(
    @([string]$manifest.expectedCurrent.exeSha256, 'expected executable'),
    @([string]$manifest.expectedCurrent.loaderSha256, 'expected loader'),
    @([string]$manifest.desired.exe.sha256, 'desired executable'),
    @([string]$manifest.desired.loader.sha256, 'desired loader')
  )) {
    Assert-HexSha256 $pair[0] $pair[1]
  }
  if ([int64]$manifest.desired.exe.size -lt 1048576 -or
      [int64]$manifest.desired.exe.size -gt 536870912) {
    throw 'desired executable size is outside bounds'
  }
  if ([int64]$manifest.desired.loader.size -lt 1024 -or
      [int64]$manifest.desired.loader.size -gt 52428800) {
    throw 'desired loader size is outside bounds'
  }
  Assert-RelativeBlobName ([string]$manifest.desired.exe.blobName) 'builds/qa/'
  Assert-RelativeBlobName ([string]$manifest.desired.loader.blobName) 'builds/qa/'
  return $manifest
}

function New-DefaultState {
  return [ordered]@{
    schemaVersion = 1
    lastGeneration = 0
    lastTerminalEtag = $null
    lastManifestSha256 = $null
    lastResultPath = $null
    lastResultBlobName = $null
    pendingResultPath = $null
    pendingResultBlobName = $null
  }
}

function Get-State {
  if (-not (Test-Path -LiteralPath $statePath -PathType Leaf)) { return New-DefaultState }
  $state = Read-JsonFile $statePath
  if ([int]$state.schemaVersion -ne 1 -or [int64]$state.lastGeneration -lt 0) {
    throw 'updater state is invalid'
  }
  return $state
}

function Set-JournalPhase([object]$Journal, [string]$Phase) {
  $Journal.phase = $Phase
  $Journal.updatedAtUtc = [DateTime]::UtcNow.ToString('o')
  Write-AtomicJson $journalPath $Journal
}

function Restore-Backup([string]$BackupPath, [string]$TargetPath) {
  if (-not (Test-Path -LiteralPath $BackupPath -PathType Leaf)) {
    throw 'transaction backup is absent'
  }
  if (Test-Path -LiteralPath $TargetPath -PathType Leaf) {
    [IO.File]::Replace($BackupPath, $TargetPath, $null, $true)
  } else {
    [IO.File]::Move($BackupPath, $TargetPath)
  }
}

function Invoke-Rollback([object]$Journal) {
  $running = @(Get-ExactOslProcesses)
  if ($running.Count -gt 1) { throw 'rollback OSL process set is ambiguous' }
  $exeChanged = (Get-Sha256 $oslExePath) -cne [string]$Journal.oldExeSha256
  $loaderChanged = (Get-Sha256 $loaderPath) -cne [string]$Journal.oldLoaderSha256
  if (($exeChanged -or $loaderChanged) -and $running.Count -eq 1) {
    Stop-ExactOsl $running[0]
    $running = @()
  }
  if ($exeChanged) {
    Restore-Backup ([string]$Journal.exeBackup) $oslExePath
  }
  if ($loaderChanged) {
    Restore-Backup ([string]$Journal.loaderBackup) $loaderPath
  }
  if ((Get-Sha256 $oslExePath) -cne [string]$Journal.oldExeSha256 -or
      (Get-Sha256 $loaderPath) -cne [string]$Journal.oldLoaderSha256) {
    throw 'rollback hash verification failed'
  }
  if (@(Get-ExactOslProcesses).Count -eq 0) {
    [void](Start-ExactOsl ([string]$Journal.oldExeSha256))
  }
}

function Get-ResultIdentity([object]$Manifest, [string]$ManifestSha256) {
  if ($null -ne $Manifest -and
      [string]$Manifest.invocationId -cmatch '^[a-z0-9][a-z0-9-]{7,63}$' -and
      [int64]$Manifest.generation -gt 0) {
    $base = "$([int64]$Manifest.generation)-$([string]$Manifest.invocationId)-$ManifestSha256"
  } else {
    $base = "rejected-$ManifestSha256"
  }
  return [pscustomobject]@{
    LocalPath = Join-Path $resultsRoot "$base.json"
    BlobName = "$resultPrefix$base.json"
  }
}

function Complete-Terminal(
  [object]$State,
  [string]$Etag,
  [string]$ManifestSha256,
  [object]$Manifest,
  [object]$Terminal
) {
  $identity = Get-ResultIdentity $Manifest $ManifestSha256
  Write-AtomicJson $identity.LocalPath $Terminal
  $State.lastTerminalEtag = $Etag
  $State.lastManifestSha256 = $ManifestSha256
  $State.lastResultPath = $identity.LocalPath
  $State.lastResultBlobName = $identity.BlobName
  if ($null -ne $Manifest -and [int64]$Manifest.generation -gt [int64]$State.lastGeneration) {
    $State.lastGeneration = [int64]$Manifest.generation
  }
  $State.pendingResultPath = $identity.LocalPath
  $State.pendingResultBlobName = $identity.BlobName
  Write-AtomicJson $statePath $State
  try {
    Upload-Result $identity.BlobName $identity.LocalPath
    $State.pendingResultPath = $null
    $State.pendingResultBlobName = $null
    Write-AtomicJson $statePath $State
  } catch {
    # The terminal record and one-ETag checkpoint are already durable locally.
  }
}

function Retry-PendingResult([object]$State) {
  if ([string]::IsNullOrWhiteSpace([string]$State.pendingResultPath) -or
      [string]::IsNullOrWhiteSpace([string]$State.pendingResultBlobName)) {
    return
  }
  if (-not (Test-Path -LiteralPath ([string]$State.pendingResultPath) -PathType Leaf)) {
    throw 'pending terminal result is absent'
  }
  try {
    Upload-Result ([string]$State.pendingResultBlobName) ([string]$State.pendingResultPath)
    $State.pendingResultPath = $null
    $State.pendingResultBlobName = $null
    Write-AtomicJson $statePath $State
  } catch {
    # Retry is bounded to once per poll and never re-applies the manifest.
  }
}

function Recover-IncompleteTransaction([object]$State) {
  if (-not (Test-Path -LiteralPath $journalPath -PathType Leaf)) { return }
  $journal = Read-JsonFile $journalPath
  if ([int]$journal.schemaVersion -ne 1 -or
      [string]$journal.targetMachine -cne $targetMachine -or
      [int]$journal.sessionId -ne $targetSessionId) {
    throw 'transaction journal identity mismatch'
  }
  if ([string]$State.lastTerminalEtag -ceq [string]$journal.etag -and
      [string]$journal.phase -in @('launched', 'committed')) {
    foreach ($path in @(
      [string]$journal.exeStage,
      [string]$journal.loaderStage,
      [string]$journal.exeBackup,
      [string]$journal.loaderBackup
    )) {
      if (-not [string]::IsNullOrWhiteSpace($path) -and
          (Test-Path -LiteralPath $path -PathType Leaf)) {
        Remove-Item -LiteralPath $path -Force
      }
    }
    Remove-Item -LiteralPath $journalPath -Force
    return
  }
  $rollbackSucceeded = $false
  $detail = 'resident updater recovered an incomplete transaction'
  try {
    # Reconcile actual file hashes rather than trusting the last journal
    # phase. A crash can occur after a stop/replace but before the next phase
    # write; the old process must still be relaunched and mixed bytes restored.
    Invoke-Rollback $journal
    $rollbackSucceeded = $true
  } catch {
    $detail = "$detail; rollback failed closed: $($_.Exception.Message)"
  }
  $terminal = [ordered]@{
    schemaVersion = 1
    terminal = $true
    status = 'recoveredIncompleteTransaction'
    targetMachine = $targetMachine
    etag = [string]$journal.etag
    manifestSha256 = [string]$journal.manifestSha256
    generation = [int64]$journal.generation
    invocationId = [string]$journal.invocationId
    completedAtUtc = [DateTime]::UtcNow.ToString('o')
    rollbackSucceeded = $rollbackSucceeded
    detail = $detail
  }
  $manifestIdentity = [pscustomobject]@{
    generation = [int64]$journal.generation
    invocationId = [string]$journal.invocationId
  }
  Complete-Terminal $State ([string]$journal.etag) ([string]$journal.manifestSha256) $manifestIdentity $terminal
  if ($rollbackSucceeded) {
    Remove-Item -LiteralPath $journalPath -Force
  }
}

function Invoke-Update(
  [object]$State,
  [object]$Manifest,
  [string]$Etag,
  [string]$ManifestSha256
) {
  $discordBefore = @()
  $journal = $null
  $oldOsl = $null
  $newOsl = $null
  $rollbackSucceeded = $false
  $status = 'failed'
  $detail = $null
  try {
    Assert-InteractiveSession
    Assert-LauncherTask
    if (-not (Test-Path -LiteralPath $oslExePath -PathType Leaf) -or
        -not (Test-Path -LiteralPath $loaderPath -PathType Leaf)) {
      throw 'exact OSL installation is incomplete'
    }
    if ((Get-Sha256 $oslExePath) -cne [string]$Manifest.expectedCurrent.exeSha256 -or
        (Get-Sha256 $loaderPath) -cne [string]$Manifest.expectedCurrent.loaderSha256) {
      throw 'installed bytes do not match manifest expected-current hashes'
    }
    $oldProcesses = @(Get-ExactOslProcesses)
    if ($oldProcesses.Count -gt 1) { throw 'old OSL process set is ambiguous' }
    if ($oldProcesses.Count -eq 1) { $oldOsl = $oldProcesses[0] }
    $discordBefore = @(Get-DiscordFingerprint)

    $suffix = "$([int64]$Manifest.generation)-$([string]$Manifest.invocationId)"
    $exeStage = Join-Path $installRoot ".OSL Privacy.exe.$suffix.stage"
    $loaderStage = Join-Path $installRoot ".WebView2Loader.dll.$suffix.stage"
    $exeBackup = Join-Path $installRoot "OSL Privacy.exe.pre-$suffix"
    $loaderBackup = Join-Path $installRoot "WebView2Loader.dll.pre-$suffix"
    foreach ($reserved in @($exeStage, $loaderStage, $exeBackup, $loaderBackup)) {
      if (Test-Path -LiteralPath $reserved) { throw 'transaction staging or backup path already exists' }
    }

    Save-Artifact ([string]$Manifest.desired.exe.blobName) $exeStage `
      ([int64]$Manifest.desired.exe.size) ([string]$Manifest.desired.exe.sha256)
    Save-Artifact ([string]$Manifest.desired.loader.blobName) $loaderStage `
      ([int64]$Manifest.desired.loader.size) ([string]$Manifest.desired.loader.sha256)

    $journal = [pscustomobject]@{
      schemaVersion = 1
      targetMachine = $targetMachine
      sessionId = $targetSessionId
      etag = $Etag
      manifestSha256 = $ManifestSha256
      generation = [int64]$Manifest.generation
      invocationId = [string]$Manifest.invocationId
      phase = 'staged'
      oldExeSha256 = [string]$Manifest.expectedCurrent.exeSha256
      oldLoaderSha256 = [string]$Manifest.expectedCurrent.loaderSha256
      desiredExeSha256 = [string]$Manifest.desired.exe.sha256
      desiredLoaderSha256 = [string]$Manifest.desired.loader.sha256
      exeStage = $exeStage
      loaderStage = $loaderStage
      exeBackup = $exeBackup
      loaderBackup = $loaderBackup
      oldOslPid = if ($null -ne $oldOsl) { [int]$oldOsl.ProcessId } else { $null }
      discordFingerprint = Get-FingerprintKey $discordBefore
      updatedAtUtc = [DateTime]::UtcNow.ToString('o')
    }
    Write-AtomicJson $journalPath $journal

    Set-JournalPhase $journal 'stopping'
    if ($null -ne $oldOsl) { Stop-ExactOsl $oldOsl }
    Set-JournalPhase $journal 'stopped'
    if ((Get-FingerprintKey @(Get-DiscordFingerprint)) -cne (Get-FingerprintKey $discordBefore)) {
      throw 'Discord fingerprint changed before swap'
    }

    Set-JournalPhase $journal 'replacingLoader'
    [IO.File]::Replace($loaderStage, $loaderPath, $loaderBackup, $true)
    Set-JournalPhase $journal 'loaderReplaced'
    Set-JournalPhase $journal 'replacingExe'
    [IO.File]::Replace($exeStage, $oslExePath, $exeBackup, $true)
    Set-JournalPhase $journal 'exeReplaced'
    if ((Get-Sha256 $oslExePath) -cne [string]$Manifest.desired.exe.sha256 -or
        (Get-Sha256 $loaderPath) -cne [string]$Manifest.desired.loader.sha256) {
      throw 'installed desired hashes do not verify'
    }

    Set-JournalPhase $journal 'launching'
    $newOsl = Start-ExactOsl ([string]$Manifest.desired.exe.sha256)
    Set-JournalPhase $journal 'launched'
    $discordAfter = @(Get-DiscordFingerprint)
    if ((Get-FingerprintKey $discordAfter) -cne (Get-FingerprintKey $discordBefore)) {
      throw 'Discord fingerprint changed during OSL-only update'
    }
    $status = 'installedPreservedAndLaunched'
    $terminal = [ordered]@{
      schemaVersion = 1
      terminal = $true
      status = $status
      targetMachine = $targetMachine
      etag = $Etag
      manifestSha256 = $ManifestSha256
      generation = [int64]$Manifest.generation
      invocationId = [string]$Manifest.invocationId
      completedAtUtc = [DateTime]::UtcNow.ToString('o')
      oldExeSha256 = [string]$Manifest.expectedCurrent.exeSha256
      oldLoaderSha256 = [string]$Manifest.expectedCurrent.loaderSha256
      installedExeSha256 = Get-Sha256 $oslExePath
      installedLoaderSha256 = Get-Sha256 $loaderPath
      oldOslPid = if ($null -ne $oldOsl) { [int]$oldOsl.ProcessId } else { $null }
      newOslPid = [int]$newOsl.ProcessId
      sessionId = $targetSessionId
      discordFingerprintBefore = $discordBefore
      discordFingerprintAfter = $discordAfter
      discordUnchanged = $true
      profilesAccessed = $false
      foregroundRequested = $false
    }
    Complete-Terminal $State $Etag $ManifestSha256 $Manifest $terminal
    try {
      Set-JournalPhase $journal 'committed'
      foreach ($path in @($exeBackup, $loaderBackup, $exeStage, $loaderStage)) {
        if (Test-Path -LiteralPath $path) {
          Remove-Item -LiteralPath $path -Force
        }
      }
      Remove-Item -LiteralPath $journalPath -Force
    } catch {
      # Durable terminal state makes a remaining launched journal safe to reconcile.
    }
    return
  } catch {
    $detail = $_.Exception.Message
    if ($null -ne $journal) {
      try {
        Invoke-Rollback $journal
        $rollbackSucceeded = $true
      } catch {
        $detail = "$detail; rollback failed closed: $($_.Exception.Message)"
      }
    }
    $discordAfterFailure = @(Get-DiscordFingerprint)
    $discordUnchanged = (Get-FingerprintKey $discordAfterFailure) -ceq (Get-FingerprintKey $discordBefore)
    $terminal = [ordered]@{
      schemaVersion = 1
      terminal = $true
      status = 'failed'
      targetMachine = $targetMachine
      etag = $Etag
      manifestSha256 = $ManifestSha256
      generation = [int64]$Manifest.generation
      invocationId = [string]$Manifest.invocationId
      completedAtUtc = [DateTime]::UtcNow.ToString('o')
      detail = $detail
      rollbackSucceeded = $rollbackSucceeded
      discordFingerprintBefore = $discordBefore
      discordFingerprintAfter = $discordAfterFailure
      discordUnchanged = $discordUnchanged
      profilesAccessed = $false
      foregroundRequested = $false
    }
    Complete-Terminal $State $Etag $ManifestSha256 $Manifest $terminal
    if ($null -ne $journal -and $rollbackSucceeded) {
      Remove-Item -LiteralPath $journalPath -Force
    }
  }
}

[void](New-Item -ItemType Directory -Path $root -Force)
[void](New-Item -ItemType Directory -Path $resultsRoot -Force)
if ((Get-LocalVmName) -cne $targetMachine) { throw 'local Azure VM identity mismatch' }

$state = Get-State
Recover-IncompleteTransaction $state

if (-not [string]::IsNullOrWhiteSpace($LocalBootstrapManifestPath)) {
  if (-not $RunOnce) {
    throw 'local bootstrap manifest is accepted only with RunOnce'
  }
  $canonicalLocalPath = [IO.Path]::GetFullPath($LocalBootstrapManifestPath)
  if (-not [string]::Equals(
      $canonicalLocalPath,
      $bootstrapManifestPath,
      [StringComparison]::OrdinalIgnoreCase
  )) {
    throw 'local bootstrap manifest path is not the exact protected canonical path'
  }
  if (-not (Test-Path -LiteralPath $bootstrapManifestPath -PathType Leaf)) {
    throw 'local bootstrap manifest is absent'
  }
  $bootstrapItem = Get-Item -LiteralPath $bootstrapManifestPath -Force
  if (($bootstrapItem.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0 -or
      $bootstrapItem.Length -le 0 -or
      $bootstrapItem.Length -gt $manifestMaxBytes) {
    throw 'local bootstrap manifest file is invalid'
  }
  $bootstrapBytes = [IO.File]::ReadAllBytes($bootstrapManifestPath)
  $bootstrapSha256 = Get-BytesSha256 $bootstrapBytes
  $bootstrapIdentity = "local-bootstrap-$bootstrapSha256"
  $state = Get-State
  $manifest = $null
  try {
    $manifest = ConvertTo-ValidatedManifest $bootstrapBytes ([int64]$state.lastGeneration)
    Invoke-Update $state $manifest $bootstrapIdentity $bootstrapSha256
  } catch {
    $terminal = [ordered]@{
      schemaVersion = 1
      terminal = $true
      status = 'manifestRejected'
      targetMachine = $targetMachine
      etag = $bootstrapIdentity
      manifestSha256 = $bootstrapSha256
      completedAtUtc = [DateTime]::UtcNow.ToString('o')
      detail = $_.Exception.Message
      profilesAccessed = $false
      foregroundRequested = $false
    }
    Complete-Terminal $state $bootstrapIdentity $bootstrapSha256 $null $terminal
  }
  $terminalState = Get-State
  if ([string]$terminalState.lastTerminalEtag -ceq $bootstrapIdentity) {
    Remove-Item -LiteralPath $bootstrapManifestPath -Force
  } else {
    throw 'local bootstrap manifest did not reach a terminal checkpoint'
  }
  return
}

do {
  $state = Get-State
  Retry-PendingResult $state
  $manifestResponse = $null
  try {
    $manifestResponse = Get-Manifest ([string]$state.lastTerminalEtag)
    if (-not $manifestResponse.NotModified) {
      if ([string]$state.lastTerminalEtag -ceq [string]$manifestResponse.Etag) {
        throw 'manifest ETag repeated outside conditional response'
      }
      $manifest = $null
      try {
        $manifest = ConvertTo-ValidatedManifest $manifestResponse.Bytes ([int64]$state.lastGeneration)
        Invoke-Update $state $manifest $manifestResponse.Etag $manifestResponse.Sha256
      } catch {
        $terminal = [ordered]@{
          schemaVersion = 1
          terminal = $true
          status = 'manifestRejected'
          targetMachine = $targetMachine
          etag = [string]$manifestResponse.Etag
          manifestSha256 = [string]$manifestResponse.Sha256
          completedAtUtc = [DateTime]::UtcNow.ToString('o')
          detail = $_.Exception.Message
          profilesAccessed = $false
          foregroundRequested = $false
        }
        Complete-Terminal $state $manifestResponse.Etag $manifestResponse.Sha256 $null $terminal
      }
    }
  } catch {
    # Transient fetch errors have no ETag and cannot create or repeat a terminal update.
  }
  if (-not $RunOnce) { Start-Sleep -Seconds $PollSeconds }
} while (-not $RunOnce)
