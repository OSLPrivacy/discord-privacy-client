param(
  [Parameter(Mandatory = $true)]
  [string]$UpdaterSourcePath,

  [Parameter(Mandatory = $true)]
  [ValidatePattern('^[0-9a-fA-F]{64}$')]
  [string]$UpdaterSha256,

  [switch]$DeferPollerStart
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$storageAccount = 'osltestartifactsa7d5'
$targetSessionId = 2
$interactiveUserName = 'osltest'
$root = 'C:\ProgramData\OSL-QA\fast-updater'
$configPath = Join-Path $root 'config.json'
$updaterPath = Join-Path $root 'osl-vm-fast-qa-updater.ps1'
$oslExePath = 'C:\Users\osltest\Desktop\OSL Privacy\OSL Privacy.exe'
$installRoot = [IO.Path]::GetDirectoryName($oslExePath)
$loaderPath = Join-Path $installRoot 'WebView2Loader.dll'

function Get-LocalVmName {
  return ([string](Invoke-RestMethod -Method Get `
    -Uri 'http://169.254.169.254/metadata/instance/compute/name?api-version=2021-02-01&format=text' `
    -Headers @{ Metadata = 'true' } -TimeoutSec 10)).Trim()
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

$localVmName = Get-LocalVmName
$clientConfig = Get-ClosedClientMapping $localVmName
if ([string]$clientConfig.storageAccount -cne $storageAccount -or
    [int]$clientConfig.sessionId -ne $targetSessionId -or
    [string]$clientConfig.interactiveUserName -cne $interactiveUserName -or
    [string]$clientConfig.oslExePath -cne $oslExePath) {
  throw 'closed client mapping changed fixed updater invariants'
}
$launcherTaskName = [string]$clientConfig.launcherTaskName
$pollerTaskName = [string]$clientConfig.pollerTaskName

if (-not (Test-Path -LiteralPath $UpdaterSourcePath -PathType Leaf)) {
  throw 'updater source is absent'
}
$expected = $UpdaterSha256.ToLowerInvariant()
if ((Get-FileHash -LiteralPath $UpdaterSourcePath -Algorithm SHA256).Hash.ToLowerInvariant() -cne $expected) {
  throw 'updater source hash mismatch'
}
$tokens = $null
$parseErrors = $null
[void][Management.Automation.Language.Parser]::ParseFile(
  [IO.Path]::GetFullPath($UpdaterSourcePath),
  [ref]$tokens,
  [ref]$parseErrors
)
if (@($parseErrors).Count -ne 0) {
  throw 'updater source does not parse as Windows PowerShell'
}
if (-not (Test-Path -LiteralPath $oslExePath -PathType Leaf) -or
    -not (Test-Path -LiteralPath $loaderPath -PathType Leaf)) {
  throw 'exact installed OSL files are absent'
}
if (Get-ScheduledTask -TaskName $launcherTaskName -ErrorAction SilentlyContinue) {
  throw 'fixed launcher task already exists'
}
if (Get-ScheduledTask -TaskName $pollerTaskName -ErrorAction SilentlyContinue) {
  throw 'fixed poller task already exists'
}

$explorers = @(Get-CimInstance Win32_Process -Filter "Name = 'explorer.exe'" | Where-Object {
  [int]$_.SessionId -eq $targetSessionId
})
if ($explorers.Count -ne 1) { throw 'interactive Explorer session is unavailable or ambiguous' }
$owner = Invoke-CimMethod -InputObject $explorers[0] -MethodName GetOwner
if ($owner.ReturnValue -ne 0 -or $owner.User -cne $interactiveUserName -or -not $owner.Domain) {
  throw 'interactive session owner is not exact osltest identity'
}
$interactiveUser = "$($owner.Domain)\$($owner.User)"

[void](New-Item -ItemType Directory -Path $root -Force)
$acl = [Security.AccessControl.DirectorySecurity]::new()
$acl.SetAccessRuleProtection($true, $false)
foreach ($sidValue in @('S-1-5-18', 'S-1-5-32-544')) {
  $sid = [Security.Principal.SecurityIdentifier]::new($sidValue)
  $rule = [Security.AccessControl.FileSystemAccessRule]::new(
    $sid,
    [Security.AccessControl.FileSystemRights]::FullControl,
    [Security.AccessControl.InheritanceFlags]'ContainerInherit, ObjectInherit',
    [Security.AccessControl.PropagationFlags]::None,
    [Security.AccessControl.AccessControlType]::Allow
  )
  [void]$acl.AddAccessRule($rule)
}
Set-Acl -LiteralPath $root -AclObject $acl

$configTemporary = "$configPath.tmp"
$configJson = $clientConfig | ConvertTo-Json -Depth 4 -Compress
[IO.File]::WriteAllText($configTemporary, $configJson, [Text.UTF8Encoding]::new($false))
[IO.File]::Move($configTemporary, $configPath)

$temporary = "$updaterPath.stage"
[IO.File]::Copy([IO.Path]::GetFullPath($UpdaterSourcePath), $temporary, $false)
if ((Get-FileHash -LiteralPath $temporary -Algorithm SHA256).Hash.ToLowerInvariant() -cne $expected) {
  throw 'staged updater hash mismatch'
}
[IO.File]::Move($temporary, $updaterPath)
if ((Get-FileHash -LiteralPath $updaterPath -Algorithm SHA256).Hash.ToLowerInvariant() -cne $expected) {
  throw 'resident updater hash mismatch'
}

$launcherAction = New-ScheduledTaskAction -Execute $oslExePath -WorkingDirectory $installRoot
$launcherPrincipal = New-ScheduledTaskPrincipal `
  -UserId $interactiveUser -LogonType Interactive -RunLevel Limited
$launcherSettings = New-ScheduledTaskSettingsSet `
  -ExecutionTimeLimit ([TimeSpan]::Zero) -MultipleInstances IgnoreNew

$pollerAction = New-ScheduledTaskAction -Execute 'powershell.exe' -Argument (
  '-NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "{0}"' -f $updaterPath
)
$pollerPrincipal = New-ScheduledTaskPrincipal `
  -UserId 'SYSTEM' -LogonType ServiceAccount -RunLevel Highest
$pollerSettings = New-ScheduledTaskSettingsSet `
  -ExecutionTimeLimit ([TimeSpan]::Zero) `
  -MultipleInstances IgnoreNew `
  -StartWhenAvailable
$pollerTrigger = New-ScheduledTaskTrigger -AtStartup

$launcherRegistered = $false
try {
  Register-ScheduledTask -TaskName $launcherTaskName `
    -Action $launcherAction -Principal $launcherPrincipal -Settings $launcherSettings | Out-Null
  $launcherRegistered = $true
  Register-ScheduledTask -TaskName $pollerTaskName `
    -Action $pollerAction -Principal $pollerPrincipal -Settings $pollerSettings `
    -Trigger $pollerTrigger | Out-Null
} catch {
  if ($launcherRegistered) {
    Unregister-ScheduledTask -TaskName $launcherTaskName -Confirm:$false -ErrorAction SilentlyContinue
  }
  throw
}

if (-not $DeferPollerStart) {
  Start-ScheduledTask -TaskName $pollerTaskName
}

[pscustomobject]@{
  Status = 'resident-fast-qa-updater-installed'
  ClientId = [int]$clientConfig.clientId
  TargetMachine = [string]$clientConfig.targetMachine
  StorageContainer = [string]$clientConfig.storageContainer
  ManifestBlobName = [string]$clientConfig.manifestBlobName
  ResultPrefix = [string]$clientConfig.resultPrefix
  UpdaterPath = $updaterPath
  UpdaterSha256 = $expected
  PollerTaskName = $pollerTaskName
  PollerPrincipal = 'SYSTEM'
  PollerStarted = -not $DeferPollerStart
  LauncherTaskName = $launcherTaskName
  LauncherPrincipal = $interactiveUser
  LauncherLogonType = 'Interactive'
  LauncherRunLevel = 'Limited'
  SessionId = $targetSessionId
} | ConvertTo-Json -Compress
