# OSL-QA-Capability: whatsapp-deploy-preserve/v1
param(
  [Parameter(Mandatory)][ValidatePattern('^[a-z0-9][a-z0-9-]{7,63}$')][string]$InvocationId,
  [Parameter(Mandatory)][uri]$ExeUri,
  [Parameter(Mandatory)][ValidatePattern('^[0-9a-fA-F]{64}$')][string]$ExeSha256,
  [Parameter(Mandatory)][uri]$WebView2LoaderUri,
  [Parameter(Mandatory)][ValidatePattern('^[0-9a-fA-F]{64}$')][string]$WebView2LoaderSha256,
  [Parameter(Mandatory)][ValidateRange(1,128)][int]$SessionId,
  [ValidateRange(10,90)][int]$StopTimeoutSeconds=30,
  [ValidateRange(10,120)][int]$LaunchTimeoutSeconds=60
)

$ErrorActionPreference='Stop'
Set-StrictMode -Version Latest
Add-Type -AssemblyName System.Net.Http
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class OslWhatsAppDeployWindows {
  public delegate bool EnumWindowsProc(IntPtr hwnd, IntPtr lparam);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumWindowsProc callback, IntPtr lparam);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint processId);
  [DllImport("user32.dll")] [return: MarshalAs(UnmanagedType.Bool)] public static extern bool IsWindowVisible(IntPtr hwnd);
  [DllImport("user32.dll")] [return: MarshalAs(UnmanagedType.Bool)] public static extern bool IsIconic(IntPtr hwnd);
}
'@

$allowedArtifactHost='osltestartifactsa7d5.blob.core.windows.net'
$oslPath=[IO.Path]::GetFullPath('C:\Users\osltest\Desktop\OSL Privacy\OSL Privacy.exe')
$installRoot=[IO.Path]::GetDirectoryName($oslPath)
$loaderPath=Join-Path $installRoot 'WebView2Loader.dll'
$exeStage=Join-Path $installRoot ".OSL Privacy.exe.$InvocationId.stage"
$loaderStage=Join-Path $installRoot ".WebView2Loader.dll.$InvocationId.stage"
$exeBackup=Join-Path $installRoot ".OSL Privacy.exe.$InvocationId.rollback"
$loaderBackup=Join-Path $installRoot ".WebView2Loader.dll.$InvocationId.rollback"
$taskName="OSL-QA-WhatsApp-Deploy-$InvocationId"
$exeExpected=$ExeSha256.ToLowerInvariant()
$loaderExpected=$WebView2LoaderSha256.ToLowerInvariant()
$packageFamily='5319275A.WhatsAppDesktop_cv1g1gvanyjgm'
$packageName='5319275A.WhatsAppDesktop'

function Get-Sha256([string]$Path) {
  (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
}

function Assert-ArtifactLayout([uri]$Exe,[uri]$Loader) {
  foreach($uri in @($Exe,$Loader)) {
    if($uri.Scheme -cne 'https' -or $uri.Host -cne $allowedArtifactHost -or $uri.Query -or $uri.Fragment) {
      throw 'artifact URI is outside the exact managed-identity blob host or is not query-free HTTPS'
    }
  }
  if(-not $Exe.AbsolutePath.EndsWith('/OSL%20Privacy.exe',[StringComparison]::OrdinalIgnoreCase)) {
    throw 'executable URI does not use the immutable OSL artifact layout'
  }
  if(-not $Loader.AbsolutePath.EndsWith('/WebView2Loader.dll',[StringComparison]::OrdinalIgnoreCase)) {
    throw 'loader URI does not use the immutable OSL artifact layout'
  }
  $exeDirectory=$Exe.AbsolutePath.Substring(0,$Exe.AbsolutePath.LastIndexOf('/'))
  $loaderDirectory=$Loader.AbsolutePath.Substring(0,$Loader.AbsolutePath.LastIndexOf('/'))
  if($exeDirectory -cne $loaderDirectory) { throw 'artifacts are not in the same immutable build directory' }
}

function Save-ManagedIdentityArtifact([uri]$Uri,[string]$Destination,[string]$Expected) {
  $tokenUri='http://169.254.169.254/metadata/identity/oauth2/token?api-version=2018-02-01&resource=https%3A%2F%2Fstorage.azure.com%2F'
  $token=Invoke-RestMethod -Method Get -Uri $tokenUri -Headers @{Metadata='true'} -TimeoutSec 10
  if(-not $token.access_token) { throw 'managed identity storage token unavailable' }
  $download="$Destination.download"
  $client=[Net.Http.HttpClient]::new()
  try {
    $client.Timeout=[TimeSpan]::FromSeconds(30)
    $client.DefaultRequestHeaders.Authorization=[Net.Http.Headers.AuthenticationHeaderValue]::new('Bearer',[string]$token.access_token)
    $client.DefaultRequestHeaders.Add('x-ms-version','2023-11-03')
    $response=$client.GetAsync($Uri).GetAwaiter().GetResult()
    if(-not $response.IsSuccessStatusCode) { throw 'managed-identity artifact download failed' }
    [IO.File]::WriteAllBytes($download,$response.Content.ReadAsByteArrayAsync().GetAwaiter().GetResult())
    if((Get-Sha256 $download) -cne $Expected) { throw 'downloaded artifact hash mismatch' }
    [IO.File]::Move($download,$Destination)
  } finally {
    $client.Dispose()
    if(Test-Path -LiteralPath $download) { Remove-Item -LiteralPath $download -Force }
  }
}

function Get-ExactOslProcesses {
  @(Get-CimInstance Win32_Process -Filter "Name = 'OSL Privacy.exe'" | Where-Object {
    [int]$_.SessionId -eq $SessionId -and $_.ExecutablePath -and
      [string]::Equals([IO.Path]::GetFullPath([string]$_.ExecutablePath),$oslPath,[StringComparison]::OrdinalIgnoreCase)
  })
}

function Test-IsWhatsAppGuardian($Process) {
  [string]$Process.CommandLine -match '(?:^|\s)--osl-whatsapp-window-guardian-v1(?:\s|$)'
}

function Get-ExactOslPrimary {
  @(Get-ExactOslProcesses | Where-Object { -not (Test-IsWhatsAppGuardian $_) })
}

function Get-OfficialWhatsAppPackage([string]$InteractiveUser) {
  $all=@(Get-AppxPackage -User $InteractiveUser -Name $packageName -ErrorAction Stop)
  $matching=@($all | Where-Object {
    $_.PackageFamilyName -ceq $packageFamily -and
    $_.PackageFullName -cmatch '^5319275A[.]WhatsAppDesktop_[0-9]+[.][0-9]+[.][0-9]+[.][0-9]+_(x64|x86|arm64|neutral)__cv1g1gvanyjgm$'
  })
  if($all.Count -ne 1 -or $matching.Count -ne 1) { throw 'official WhatsApp package is absent or ambiguous' }
  $location=[IO.Path]::GetFullPath([string]$matching[0].InstallLocation).TrimEnd('\')
  $windowsApps=[IO.Path]::GetFullPath((Join-Path $env:ProgramFiles 'WindowsApps')).TrimEnd('\')
  if(-not $location.StartsWith("$windowsApps\",[StringComparison]::OrdinalIgnoreCase)) {
    throw 'WhatsApp package is outside protected WindowsApps'
  }
  $matching[0]
}

function Get-WhatsAppProcessSnapshot([string]$PackageLocation) {
  $prefix=[IO.Path]::GetFullPath($PackageLocation).TrimEnd('\')+'\'
  @(
    Get-CimInstance Win32_Process | Where-Object {
      [int]$_.SessionId -eq $SessionId -and $_.ExecutablePath -and
        [IO.Path]::GetFullPath([string]$_.ExecutablePath).StartsWith($prefix,[StringComparison]::OrdinalIgnoreCase)
    } | Sort-Object ProcessId | ForEach-Object {
      [pscustomobject]@{
        ProcessId=[int]$_.ProcessId
        CreationDate=[string]$_.CreationDate
        ExecutablePath=[IO.Path]::GetFullPath([string]$_.ExecutablePath)
      }
    }
  )
}

function Get-WhatsAppWindowSnapshot([object[]]$Processes) {
  $allowed=@{};foreach($process in $Processes){$allowed[[int]$process.ProcessId]=$true}
  $windows=[Collections.Generic.List[object]]::new()
  $callback=[OslWhatsAppDeployWindows+EnumWindowsProc]{
    param([IntPtr]$hwnd,[IntPtr]$unused)
    [uint32]$pid=0
    [void][OslWhatsAppDeployWindows]::GetWindowThreadProcessId($hwnd,[ref]$pid)
    if($allowed.ContainsKey([int]$pid)) {
      $windows.Add([pscustomobject]@{
        Handle=$hwnd.ToInt64()
        ProcessId=[int]$pid
        Visible=[OslWhatsAppDeployWindows]::IsWindowVisible($hwnd)
        Minimized=[OslWhatsAppDeployWindows]::IsIconic($hwnd)
      })
    }
    $true
  }
  [void][OslWhatsAppDeployWindows]::EnumWindows($callback,[IntPtr]::Zero)
  @($windows | Sort-Object Handle)
}

function Test-ExactSnapshot([object[]]$Before,[object[]]$After) {
  if($Before.Count -ne $After.Count) { return $false }
  (ConvertTo-Json @($Before) -Compress -Depth 4) -ceq (ConvertTo-Json @($After) -Compress -Depth 4)
}

function Start-ExactOsl([string]$InteractiveUser) {
  if(-not (Get-ScheduledTask -TaskName $taskName -ErrorAction SilentlyContinue)) {
    $action=New-ScheduledTaskAction -Execute $oslPath -WorkingDirectory $installRoot
    $principal=New-ScheduledTaskPrincipal -UserId $InteractiveUser -LogonType Interactive -RunLevel Limited
    $settings=New-ScheduledTaskSettingsSet -ExecutionTimeLimit ([TimeSpan]::FromMinutes(2)) -MultipleInstances IgnoreNew
    Register-ScheduledTask -TaskName $taskName -Action $action -Principal $principal -Settings $settings | Out-Null
  }
  Start-ScheduledTask -TaskName $taskName
  $deadline=[DateTime]::UtcNow.AddSeconds($LaunchTimeoutSeconds)
  do {
    $running=@(Get-ExactOslPrimary)
    if($running.Count -eq 1) { return $running[0] }
    if($running.Count -gt 1) { throw 'exact OSL relaunch is ambiguous' }
    Start-Sleep -Milliseconds 250
  } while([DateTime]::UtcNow -lt $deadline)
  throw 'exact OSL process did not relaunch within the bounded deadline'
}

Assert-ArtifactLayout $ExeUri $WebView2LoaderUri
if(-not (Test-Path -LiteralPath $oslPath -PathType Leaf) -or -not (Test-Path -LiteralPath $loaderPath -PathType Leaf)) {
  throw 'exact OSL installation is absent or incomplete'
}
$explorers=@(Get-CimInstance Win32_Process -Filter "Name = 'explorer.exe'" | Where-Object { [int]$_.SessionId -eq $SessionId })
if($explorers.Count -ne 1) { throw 'interactive Explorer session is unavailable or ambiguous' }
$owner=Invoke-CimMethod -InputObject $explorers[0] -MethodName GetOwner
if($owner.ReturnValue -ne 0 -or $owner.User -cne 'osltest' -or -not $owner.Domain) { throw 'interactive session owner is not exact osltest identity' }
$interactiveUser="$($owner.Domain)\$($owner.User)"
$package=Get-OfficialWhatsAppPackage $interactiveUser
$whatsAppBefore=@(Get-WhatsAppProcessSnapshot ([string]$package.InstallLocation))
$windowsBefore=@(Get-WhatsAppWindowSnapshot $whatsAppBefore)
$primaryBefore=@(Get-ExactOslPrimary)
if($primaryBefore.Count -ne 1) { throw 'exact OSL primary process is unavailable or ambiguous' }

if((Get-Sha256 $oslPath) -ceq $exeExpected -and (Get-Sha256 $loaderPath) -ceq $loaderExpected) {
  $whatsAppAfter=@(Get-WhatsAppProcessSnapshot ([string]$package.InstallLocation))
  $windowsAfter=@(Get-WhatsAppWindowSnapshot $whatsAppAfter)
  if(-not (Test-ExactSnapshot $whatsAppBefore $whatsAppAfter) -or -not (Test-ExactSnapshot $windowsBefore $windowsAfter)) {
    throw 'WhatsApp process or window state changed during idempotent verification'
  }
  [pscustomobject]@{
    Schema='whatsapp-deploy-preserve/v1';Status='alreadyInstalledPreserved';Terminal=$true
    InvocationId=$InvocationId;ExeSha256=$exeExpected;WebView2LoaderSha256=$loaderExpected
    SessionId=$SessionId;OslProcessCount=1;WhatsAppProcessCount=$whatsAppAfter.Count;WhatsAppWindowCount=$windowsAfter.Count
    ExactOfficialWhatsAppPackageVerified=$true;WhatsAppProcessSetUnchanged=$true;WhatsAppWindowStateUnchanged=$true
    OslProfileTouched=$false;WhatsAppPrivateStorageRead=$false;WhatsAppProfileTouched=$false
    WhatsAppProcessTerminated=$false;WhatsAppWindowForegrounded=$false;BrowserFallbackUsed=$false
  } | ConvertTo-Json -Compress
  exit 0
}

foreach($reserved in @($exeStage,$loaderStage,$exeBackup,$loaderBackup,"$exeStage.download","$loaderStage.download")) {
  if(Test-Path -LiteralPath $reserved) { throw 'invocation staging or rollback path already exists' }
}
$replacedExe=$false;$replacedLoader=$false;$taskRegistered=$false;$oldExeHash=Get-Sha256 $oslPath;$oldLoaderHash=Get-Sha256 $loaderPath
try {
  Save-ManagedIdentityArtifact $ExeUri $exeStage $exeExpected
  Save-ManagedIdentityArtifact $WebView2LoaderUri $loaderStage $loaderExpected
  Stop-Process -Id ([int]$primaryBefore[0].ProcessId) -Force
  $stopDeadline=[DateTime]::UtcNow.AddSeconds($StopTimeoutSeconds)
  while(@(Get-ExactOslProcesses).Count -ne 0 -and [DateTime]::UtcNow -lt $stopDeadline) { Start-Sleep -Milliseconds 200 }
  if(@(Get-ExactOslProcesses).Count -ne 0) { throw 'exact OSL process or guardian did not stop within the bounded deadline' }

  [IO.File]::Replace($exeStage,$oslPath,$exeBackup,$true);$replacedExe=$true
  [IO.File]::Replace($loaderStage,$loaderPath,$loaderBackup,$true);$replacedLoader=$true
  if((Get-Sha256 $oslPath) -cne $exeExpected -or (Get-Sha256 $loaderPath) -cne $loaderExpected) { throw 'installed artifact hash mismatch' }
  $running=Start-ExactOsl $interactiveUser;$taskRegistered=$true

  $whatsAppAfter=@(Get-WhatsAppProcessSnapshot ([string]$package.InstallLocation))
  $windowsAfter=@(Get-WhatsAppWindowSnapshot $whatsAppAfter)
  if(-not (Test-ExactSnapshot $whatsAppBefore $whatsAppAfter)) { throw 'WhatsApp process set changed during OSL-only deployment' }
  if(-not (Test-ExactSnapshot $windowsBefore $windowsAfter)) { throw 'WhatsApp top-level window state changed during OSL-only deployment' }
  if((Get-Sha256 $oslPath) -cne $exeExpected) { throw 'running OSL executable bytes changed after launch' }

  Remove-Item -LiteralPath $exeBackup,$loaderBackup -Force
  $replacedExe=$false;$replacedLoader=$false
  [pscustomobject]@{
    Schema='whatsapp-deploy-preserve/v1';Status='installedPreservedAndLaunched';Terminal=$true
    InvocationId=$InvocationId;ExeSha256=$exeExpected;WebView2LoaderSha256=$loaderExpected
    SessionId=$SessionId;OslProcessCount=1;WhatsAppProcessCount=$whatsAppAfter.Count;WhatsAppWindowCount=$windowsAfter.Count
    ExactOfficialWhatsAppPackageVerified=$true;WhatsAppProcessSetUnchanged=$true;WhatsAppWindowStateUnchanged=$true
    OslProfileTouched=$false;WhatsAppPrivateStorageRead=$false;WhatsAppProfileTouched=$false
    WhatsAppProcessTerminated=$false;WhatsAppWindowForegrounded=$false;BrowserFallbackUsed=$false
  } | ConvertTo-Json -Compress
} catch {
  $failure=$_.Exception.Message
  # A post-launch validation failure must not leave the rejected executable
  # mapped while its on-disk bytes are rolled back. Stop only the exact OSL
  # primary; never stop, close, or signal a WhatsApp process/window.
  if($replacedExe) {
    $rollbackPrimary=@(Get-ExactOslPrimary)
    if($rollbackPrimary.Count -gt 1) { throw 'deployment failed and rollback cannot identify one exact OSL primary' }
    if($rollbackPrimary.Count -eq 1) {
      Stop-Process -Id ([int]$rollbackPrimary[0].ProcessId) -Force
      $rollbackStopDeadline=[DateTime]::UtcNow.AddSeconds($StopTimeoutSeconds)
      while(@(Get-ExactOslProcesses).Count -ne 0 -and [DateTime]::UtcNow -lt $rollbackStopDeadline) { Start-Sleep -Milliseconds 200 }
      if(@(Get-ExactOslProcesses).Count -ne 0) { throw 'deployment failed and exact OSL rollback process stop timed out' }
    }
  }
  if($replacedLoader -and (Test-Path -LiteralPath $loaderBackup)) {
    if(Test-Path -LiteralPath $loaderPath) {[IO.File]::Replace($loaderBackup,$loaderPath,$null,$true)} else {[IO.File]::Move($loaderBackup,$loaderPath)}
  }
  if($replacedExe -and (Test-Path -LiteralPath $exeBackup)) {
    if(Test-Path -LiteralPath $oslPath) {[IO.File]::Replace($exeBackup,$oslPath,$null,$true)} else {[IO.File]::Move($exeBackup,$oslPath)}
  }
  if((Get-Sha256 $oslPath) -cne $oldExeHash -or (Get-Sha256 $loaderPath) -cne $oldLoaderHash) { throw 'deployment failed and exact rollback verification failed' }
  if(@(Get-ExactOslPrimary).Count -eq 0) { [void](Start-ExactOsl $interactiveUser);$taskRegistered=$true }
  throw "deployment failed; previous exact OSL build restored and relaunched: $failure"
} finally {
  if(Get-ScheduledTask -TaskName $taskName -ErrorAction SilentlyContinue) { Unregister-ScheduledTask -TaskName $taskName -Confirm:$false -ErrorAction SilentlyContinue }
  foreach($temporary in @($exeStage,$loaderStage,"$exeStage.download","$loaderStage.download")) {
    if(Test-Path -LiteralPath $temporary) { Remove-Item -LiteralPath $temporary -Force }
  }
}
