param(
  [Parameter(Mandatory = $true)]
  [ValidatePattern('^[0-9a-fA-F]{64}$')]
  [string]$UpdaterSha256,

  [Parameter(Mandatory = $true)]
  [ValidateRange(1024, 1048576)]
  [int64]$UpdaterSize,

  [Parameter(Mandatory = $true)]
  [ValidatePattern('^[0-9a-fA-F]{64}$')]
  [string]$BootstrapSha256,

  [Parameter(Mandatory = $true)]
  [ValidateRange(1024, 1048576)]
  [int64]$BootstrapSize
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
Add-Type -AssemblyName System.Net.Http

$storageAccount = 'osltestartifactsa7d5'
$updaterBlobName = 'tools/fast-updater/osl-vm-fast-qa-updater.ps1'
$bootstrapBlobName = 'tools/fast-updater/osl-vm-fast-qa-updater-bootstrap.ps1'
$installRoot = 'C:\ProgramData\OSL-QA\fast-updater'
$configPath = Join-Path $installRoot 'config.json'
$bootstrapManifestPath = Join-Path $installRoot 'bootstrap-manifest.json'
$oslExePath = 'C:\Users\osltest\Desktop\OSL Privacy\OSL Privacy.exe'
$loaderPath = 'C:\Users\osltest\Desktop\OSL Privacy\WebView2Loader.dll'
$downloadRoot = 'C:\ProgramData\OSL-QA\fast-updater-install'
$downloadedUpdater = Join-Path $downloadRoot 'osl-vm-fast-qa-updater.ps1'
$downloadedBootstrap = Join-Path $downloadRoot 'osl-vm-fast-qa-updater-bootstrap.ps1'
$qaExeBlobName = 'builds/qa/2065d3329f9f6999/OSL-Privacy.exe'
$qaExeSha256 = '2065d3329f9f6999c3e51e149634c6dab660a03a5cda86cfa146465ead28410f'
$qaExeSize = 43865117
$qaLoaderBlobName = 'builds/qa/8427b1fc58ec7078/WebView2Loader.dll'
$qaLoaderSha256 = '8427b1fc58ec707813e5c0a51eb5d69397bb333250a7b891be4d3b123f1e0f1c'
$qaLoaderSize = 160320

function Get-LocalVmName {
  return ([string](Invoke-RestMethod -Method Get `
    -Uri 'http://169.254.169.254/metadata/instance/compute/name?api-version=2021-02-01&format=text' `
    -Headers @{ Metadata = 'true' } -TimeoutSec 10)).Trim()
}

function Get-ClosedClientMapping([string]$MachineName) {
  switch -CaseSensitive ($MachineName) {
    'OSL-Azure-Client-1' {
      return [pscustomobject]@{
        targetMachine = 'OSL-Azure-Client-1'
        storageContainer = 'osl-discord-qa-client1'
        pollerTaskName = 'OSL-QA-FastUpdater-Poller-C1'
      }
    }
    'OSL-Azure-Client-2' {
      return [pscustomobject]@{
        targetMachine = 'OSL-Azure-Client-2'
        storageContainer = 'osl-discord-qa-client2'
        pollerTaskName = 'OSL-QA-FastUpdater-Poller-C2'
      }
    }
    default { throw 'Azure VM name is outside the exact closed client mapping' }
  }
}

function Get-Sha256([string]$Path) {
  return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
}

function Get-StorageToken {
  $tokenUri = 'http://169.254.169.254/metadata/identity/oauth2/token' +
    '?api-version=2018-02-01&resource=https%3A%2F%2Fstorage.azure.com%2F'
  $token = Invoke-RestMethod -Method Get -Uri $tokenUri -Headers @{ Metadata = 'true' } -TimeoutSec 10
  if ([string]::IsNullOrWhiteSpace([string]$token.access_token)) {
    throw 'managed identity storage token unavailable'
  }
  return [string]$token.access_token
}

function Save-ExactScript(
  [string]$Container,
  [string]$BlobName,
  [string]$Destination,
  [int64]$ExpectedSize,
  [string]$ExpectedSha256
) {
  $uri = "https://$storageAccount.blob.core.windows.net/$Container/$BlobName"
  $temporary = "$Destination.download"
  $client = [Net.Http.HttpClient]::new()
  try {
    $client.Timeout = [TimeSpan]::FromSeconds(30)
    $client.DefaultRequestHeaders.Authorization =
      [Net.Http.Headers.AuthenticationHeaderValue]::new('Bearer', (Get-StorageToken))
    $client.DefaultRequestHeaders.Add('x-ms-version', '2023-11-03')
    $response = $client.GetAsync($uri).GetAwaiter().GetResult()
    if (-not $response.IsSuccessStatusCode) {
      throw "universal script download failed with HTTP $([int]$response.StatusCode)"
    }
    if ($null -eq $response.Content.Headers.ContentLength -or
        [int64]$response.Content.Headers.ContentLength -ne $ExpectedSize) {
      throw 'universal script response size mismatch'
    }
    $bytes = $response.Content.ReadAsByteArrayAsync().GetAwaiter().GetResult()
    if ($bytes.LongLength -ne $ExpectedSize) { throw 'universal script size mismatch' }
    [IO.File]::WriteAllBytes($temporary, $bytes)
    if ((Get-Sha256 $temporary) -cne $ExpectedSha256) {
      throw 'universal script hash mismatch'
    }
    [IO.File]::Move($temporary, $Destination)
  } finally {
    $client.Dispose()
    if (Test-Path -LiteralPath $temporary) {
      Remove-Item -LiteralPath $temporary -Force
    }
  }
}

function Assert-PowerShellParses([string]$Path) {
  $tokens = $null
  $errors = $null
  [void][Management.Automation.Language.Parser]::ParseFile(
    [IO.Path]::GetFullPath($Path),
    [ref]$tokens,
    [ref]$errors
  )
  if (@($errors).Count -ne 0) { throw 'downloaded universal script does not parse' }
}

$mapping = Get-ClosedClientMapping (Get-LocalVmName)
[void](New-Item -ItemType Directory -Path $downloadRoot -Force)
foreach ($reserved in @($downloadedUpdater, $downloadedBootstrap)) {
  if (Test-Path -LiteralPath $reserved) { throw 'manual installer download path already exists' }
}

Save-ExactScript $mapping.storageContainer $updaterBlobName $downloadedUpdater `
  $UpdaterSize $UpdaterSha256.ToLowerInvariant()
Save-ExactScript $mapping.storageContainer $bootstrapBlobName $downloadedBootstrap `
  $BootstrapSize $BootstrapSha256.ToLowerInvariant()
Assert-PowerShellParses $downloadedUpdater
Assert-PowerShellParses $downloadedBootstrap

$bootstrapRaw = (& $downloadedBootstrap `
  -UpdaterSourcePath $downloadedUpdater `
  -UpdaterSha256 $UpdaterSha256 `
  -DeferPollerStart | Out-String).Trim()
$bootstrapResult = $bootstrapRaw | ConvertFrom-Json -ErrorAction Stop
if ([string]$bootstrapResult.Status -cne 'resident-fast-qa-updater-installed' -or
    [string]$bootstrapResult.TargetMachine -cne [string]$mapping.targetMachine -or
    [string]$bootstrapResult.PollerTaskName -cne [string]$mapping.pollerTaskName -or
    [bool]$bootstrapResult.PollerStarted) {
  throw 'universal bootstrap proof is invalid'
}

$residentUpdater = Join-Path $installRoot 'osl-vm-fast-qa-updater.ps1'
$statePath = Join-Path $installRoot 'state.json'
if (-not (Test-Path -LiteralPath $configPath -PathType Leaf) -or
    -not (Test-Path -LiteralPath $oslExePath -PathType Leaf) -or
    -not (Test-Path -LiteralPath $loaderPath -PathType Leaf)) {
  throw 'manual bootstrap exact config or installed OSL files are absent'
}
if (Test-Path -LiteralPath $bootstrapManifestPath) {
  throw 'manual bootstrap manifest path already exists'
}
$config = Get-Content -LiteralPath $configPath -Raw | ConvertFrom-Json -ErrorAction Stop
$bootstrapManifest = [ordered]@{
  schemaVersion = 1
  targetMachine = [string]$config.targetMachine
  sessionId = [int]$config.sessionId
  generation = 1
  invocationId = "bootstrap-2065d332-c$([int]$config.clientId)"
  expectedCurrent = [ordered]@{
    exeSha256 = Get-Sha256 $oslExePath
    loaderSha256 = Get-Sha256 $loaderPath
  }
  desired = [ordered]@{
    exe = [ordered]@{
      blobName = $qaExeBlobName
      sha256 = $qaExeSha256
      size = $qaExeSize
    }
    loader = [ordered]@{
      blobName = $qaLoaderBlobName
      sha256 = $qaLoaderSha256
      size = $qaLoaderSize
    }
  }
}
$bootstrapManifestTemporary = "$bootstrapManifestPath.tmp"
$bootstrapManifestJson = $bootstrapManifest | ConvertTo-Json -Depth 6 -Compress
[IO.File]::WriteAllText(
  $bootstrapManifestTemporary,
  $bootstrapManifestJson,
  [Text.UTF8Encoding]::new($false)
)
[IO.File]::Move($bootstrapManifestTemporary, $bootstrapManifestPath)

& $residentUpdater -RunOnce -LocalBootstrapManifestPath $bootstrapManifestPath
if (Test-Path -LiteralPath $bootstrapManifestPath) {
  throw 'terminal updater did not consume the local bootstrap manifest'
}
if (-not (Test-Path -LiteralPath $statePath -PathType Leaf)) {
  throw 'manual update did not produce resident state'
}
$state = Get-Content -LiteralPath $statePath -Raw | ConvertFrom-Json -ErrorAction Stop
if ([int64]$state.lastGeneration -le 0 -or
    [string]::IsNullOrWhiteSpace([string]$state.lastResultPath) -or
    -not (Test-Path -LiteralPath ([string]$state.lastResultPath) -PathType Leaf)) {
  throw 'manual update did not produce an exact terminal result'
}
$terminal = Get-Content -LiteralPath ([string]$state.lastResultPath) -Raw |
  ConvertFrom-Json -ErrorAction Stop
if (-not [bool]$terminal.terminal -or
    [string]$terminal.status -cne 'installedPreservedAndLaunched' -or
    [string]$terminal.targetMachine -cne [string]$mapping.targetMachine -or
    -not [bool]$terminal.discordUnchanged -or
    [string]$terminal.installedExeSha256 -cne $qaExeSha256 -or
    [string]$terminal.installedLoaderSha256 -cne $qaLoaderSha256) {
  throw 'manual update terminal proof failed closed'
}

Start-ScheduledTask -TaskName ([string]$mapping.pollerTaskName)

[pscustomobject]@{
  Status = 'installed-and-proven'
  TargetMachine = [string]$mapping.targetMachine
  StorageContainer = [string]$mapping.storageContainer
  UpdaterSha256 = Get-Sha256 $residentUpdater
  BootstrapSha256 = Get-Sha256 $downloadedBootstrap
  Generation = [int64]$terminal.generation
  InstalledExeSha256 = [string]$terminal.installedExeSha256
  InstalledLoaderSha256 = [string]$terminal.installedLoaderSha256
  OslPid = [int]$terminal.newOslPid
  DiscordUnchanged = [bool]$terminal.discordUnchanged
  PollerTaskName = [string]$mapping.pollerTaskName
  PollerStarted = $true
} | ConvertTo-Json -Compress
