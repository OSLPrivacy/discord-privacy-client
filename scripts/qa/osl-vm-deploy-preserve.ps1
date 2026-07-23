param(
  [Parameter(Mandatory = $true)]
  [ValidatePattern('^[a-z0-9][a-z0-9-]{7,63}$')]
  [string]$InvocationId,

  [Parameter(Mandatory = $true)]
  [uri]$ExeUri,

  [Parameter(Mandatory = $true)]
  [ValidatePattern('^[0-9a-fA-F]{64}$')]
  [string]$ExeSha256,

  [Parameter(Mandatory = $true)]
  [uri]$WebView2LoaderUri,

  [Parameter(Mandatory = $true)]
  [ValidatePattern('^[0-9a-fA-F]{64}$')]
  [string]$WebView2LoaderSha256,

  [Parameter(Mandatory = $true)]
  [ValidateRange(1, 128)]
  [int]$SessionId,

  [string]$OslExePath = 'C:\Users\osltest\Desktop\OSL Privacy\OSL Privacy.exe',

  [ValidateRange(10, 90)]
  [int]$StopTimeoutSeconds = 30,

  [ValidateRange(10, 120)]
  [int]$LaunchTimeoutSeconds = 60
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
Add-Type -AssemblyName System.Net.Http

$allowedArtifactHost = 'osltestartifactsa7d5.blob.core.windows.net'
$oslExePath = [IO.Path]::GetFullPath($OslExePath)
$installRoot = [IO.Path]::GetDirectoryName($oslExePath)
$loaderPath = Join-Path $installRoot 'WebView2Loader.dll'
$taskName = "OSL-QA-Deploy-$InvocationId"
$exeStage = Join-Path $installRoot ".OSL Privacy.exe.$InvocationId.stage"
$loaderStage = Join-Path $installRoot ".WebView2Loader.dll.$InvocationId.stage"
$exeBackup = Join-Path $installRoot "OSL Privacy.exe.pre-$InvocationId"
$loaderBackup = Join-Path $installRoot "WebView2Loader.dll.pre-$InvocationId"
$exeExpected = $ExeSha256.ToLowerInvariant()
$loaderExpected = $WebView2LoaderSha256.ToLowerInvariant()

function Assert-TrustedArtifactUri([uri]$Uri) {
  if ($Uri.Scheme -cne 'https' -or $Uri.Host -cne $allowedArtifactHost -or $Uri.Query) {
    throw 'artifact URI is outside the exact trusted artifact host or contains a query'
  }
}

function Get-Sha256([string]$Path) {
  return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
}

function Save-ManagedIdentityArtifact([uri]$Uri, [string]$Destination, [string]$ExpectedSha256) {
  $tokenUri = 'http://169.254.169.254/metadata/identity/oauth2/token' +
    '?api-version=2018-02-01&resource=https%3A%2F%2Fstorage.azure.com%2F'
  $token = Invoke-RestMethod -Method Get -Uri $tokenUri -Headers @{ Metadata = 'true' } -TimeoutSec 10
  if (-not $token.access_token) { throw 'managed identity storage token unavailable' }

  $download = "$Destination.download"
  $client = [Net.Http.HttpClient]::new()
  try {
    $client.Timeout = [TimeSpan]::FromSeconds(30)
    $client.DefaultRequestHeaders.Authorization =
      [Net.Http.Headers.AuthenticationHeaderValue]::new('Bearer', [string]$token.access_token)
    $client.DefaultRequestHeaders.Add('x-ms-version', '2023-11-03')
    $response = $client.GetAsync($Uri).GetAwaiter().GetResult()
    if (-not $response.IsSuccessStatusCode) { throw 'trusted artifact download failed' }
    $bytes = $response.Content.ReadAsByteArrayAsync().GetAwaiter().GetResult()
    [IO.File]::WriteAllBytes($download, $bytes)
    if ((Get-Sha256 $download) -cne $ExpectedSha256) { throw 'downloaded artifact hash mismatch' }
    [IO.File]::Move($download, $Destination)
  } finally {
    $client.Dispose()
    if (Test-Path -LiteralPath $download) { Remove-Item -LiteralPath $download -Force }
  }
}

function Get-ExactOslProcesses {
  return @(Get-CimInstance Win32_Process -Filter "Name = 'OSL Privacy.exe'" | Where-Object {
    $_.ExecutablePath -and
      [string]::Equals([IO.Path]::GetFullPath([string]$_.ExecutablePath), $oslExePath, [StringComparison]::OrdinalIgnoreCase)
  })
}

function Get-DiscordPidSet {
  return @(
    Get-CimInstance Win32_Process -Filter "Name = 'Discord.exe'" |
      ForEach-Object { [int]$_.ProcessId } |
      Sort-Object -Unique
  )
}

function Test-EqualPidSet([int[]]$Before, [int[]]$After) {
  if ($Before.Count -ne $After.Count) { return $false }
  return -not (Compare-Object -ReferenceObject $Before -DifferenceObject $After)
}

Assert-TrustedArtifactUri $ExeUri
Assert-TrustedArtifactUri $WebView2LoaderUri
if (-not (Test-Path -LiteralPath $installRoot -PathType Container)) { throw 'exact OSL install directory is absent' }
if (-not (Test-Path -LiteralPath $oslExePath -PathType Leaf)) { throw 'exact installed OSL executable is absent' }
if (-not (Test-Path -LiteralPath $loaderPath -PathType Leaf)) { throw 'exact installed WebView2Loader.dll is absent' }
foreach ($reserved in @($exeStage, $loaderStage, $exeBackup, $loaderBackup)) {
  if (Test-Path -LiteralPath $reserved) { throw 'invocation staging or backup path already exists' }
}

$explorers = @(Get-CimInstance Win32_Process -Filter "Name = 'explorer.exe'" | Where-Object {
  [int]$_.SessionId -eq $SessionId
})
if ($explorers.Count -ne 1) { throw 'interactive Explorer session is unavailable or ambiguous' }
$owner = Invoke-CimMethod -InputObject $explorers[0] -MethodName GetOwner
if ($owner.ReturnValue -ne 0 -or $owner.User -cne 'osltest' -or -not $owner.Domain) {
  throw 'interactive session owner is not exact osltest identity'
}
$interactiveUser = "$($owner.Domain)\$($owner.User)"

$discordBefore = @(Get-DiscordPidSet)
$replacedExe = $false
$replacedLoader = $false
$taskRegistered = $false

try {
  # Stage and verify both artifacts before changing the running installation.
  Save-ManagedIdentityArtifact $ExeUri $exeStage $exeExpected
  Save-ManagedIdentityArtifact $WebView2LoaderUri $loaderStage $loaderExpected

  $exactOsl = @(Get-ExactOslProcesses)
  foreach ($process in $exactOsl) { Stop-Process -Id ([int]$process.ProcessId) -Force }

  $stopDeadline = [DateTime]::UtcNow.AddSeconds($StopTimeoutSeconds)
  while (@(Get-ExactOslProcesses).Count -ne 0 -and [DateTime]::UtcNow -lt $stopDeadline) {
    Start-Sleep -Milliseconds 200
  }
  if (@(Get-ExactOslProcesses).Count -ne 0) { throw 'exact OSL process did not stop within the bounded deadline' }

  # File.Replace is atomic per file. If the second replacement fails, the catch
  # path restores both exact previous bytes before attempting a relaunch.
  [IO.File]::Replace($exeStage, $oslExePath, $exeBackup, $true)
  $replacedExe = $true
  [IO.File]::Replace($loaderStage, $loaderPath, $loaderBackup, $true)
  $replacedLoader = $true

  if ((Get-Sha256 $oslExePath) -cne $exeExpected) { throw 'installed executable hash mismatch' }
  if ((Get-Sha256 $loaderPath) -cne $loaderExpected) { throw 'installed WebView2Loader hash mismatch' }

  $action = New-ScheduledTaskAction -Execute $oslExePath -WorkingDirectory $installRoot
  $principal = New-ScheduledTaskPrincipal -UserId $interactiveUser -LogonType Interactive -RunLevel Limited
  $settings = New-ScheduledTaskSettingsSet -ExecutionTimeLimit ([TimeSpan]::FromMinutes(2)) -MultipleInstances IgnoreNew
  Register-ScheduledTask -TaskName $taskName -Action $action -Principal $principal -Settings $settings | Out-Null
  $taskRegistered = $true
  Start-ScheduledTask -TaskName $taskName

  $launchDeadline = [DateTime]::UtcNow.AddSeconds($LaunchTimeoutSeconds)
  do {
    $running = @(Get-ExactOslProcesses | Where-Object { [int]$_.SessionId -eq $SessionId })
    if ($running.Count -eq 1) { break }
    if ($running.Count -gt 1) { throw 'exact OSL process launch is ambiguous' }
    Start-Sleep -Milliseconds 250
  } while ([DateTime]::UtcNow -lt $launchDeadline)
  if ($running.Count -ne 1) { throw 'exact OSL process did not relaunch within the bounded deadline' }
  if ((Get-Sha256 $oslExePath) -cne $exeExpected) { throw 'running executable bytes changed after launch' }

  $discordAfter = @(Get-DiscordPidSet)
  $discordUnchanged = Test-EqualPidSet $discordBefore $discordAfter
  if (-not $discordUnchanged) { throw 'Discord PID set changed during OSL-only deployment' }

  [pscustomobject]@{
    Status = 'installed-preserved-and-launched'
    InvocationId = $InvocationId
    ExeSha256 = $exeExpected
    WebView2LoaderSha256 = $loaderExpected
    OslPid = [int]$running[0].ProcessId
    SessionId = $SessionId
    ProfilesPreserved = $true
    DiscordPidsBefore = $discordBefore
    DiscordPidsAfter = $discordAfter
    DiscordPidSetUnchanged = $true
    ExeBackupPath = $exeBackup
    WebView2LoaderBackupPath = $loaderBackup
  } | ConvertTo-Json -Compress
} catch {
  # Roll back only files replaced by this invocation. No profile or Discord path
  # is read, written, copied, removed, or stopped by this script.
  if ($replacedLoader -and (Test-Path -LiteralPath $loaderBackup)) {
    if (Test-Path -LiteralPath $loaderPath) {
      [IO.File]::Replace($loaderBackup, $loaderPath, $null, $true)
    } else {
      [IO.File]::Move($loaderBackup, $loaderPath)
    }
  }
  if ($replacedExe -and (Test-Path -LiteralPath $exeBackup)) {
    if (Test-Path -LiteralPath $oslExePath) {
      [IO.File]::Replace($exeBackup, $oslExePath, $null, $true)
    } else {
      [IO.File]::Move($exeBackup, $oslExePath)
    }
  }
  throw
} finally {
  if ($taskRegistered) {
    Unregister-ScheduledTask -TaskName $taskName -Confirm:$false -ErrorAction SilentlyContinue
  }
  foreach ($stage in @($exeStage, $loaderStage)) {
    if (Test-Path -LiteralPath $stage) { Remove-Item -LiteralPath $stage -Force }
  }
}
