# OSL-QA-Capability: signal-deploy-preserve/v1
param(
  [Parameter(Mandatory)][ValidatePattern('^[a-z0-9][a-z0-9-]{7,63}$')][string]$InvocationId,
  [Parameter(Mandatory)][uri]$ExeUri,
  [Parameter(Mandatory)][ValidatePattern('^[0-9a-fA-F]{64}$')][string]$ExeSha256,
  [Parameter(Mandatory)][uri]$WebView2LoaderUri,
  [Parameter(Mandatory)][ValidatePattern('^[0-9a-fA-F]{64}$')][string]$WebView2LoaderSha256,
  [Parameter(Mandatory)][ValidateRange(1,128)][int]$SessionId,
  [Parameter(Mandatory)][ValidateNotNullOrEmpty()][string]$WindowsUser,
  [Parameter(Mandatory)][ValidateNotNullOrEmpty()][string]$ProfileRoot,
  [Parameter(Mandatory)][ValidateNotNullOrEmpty()][string]$OslExePath,
  [Parameter(Mandatory)][ValidateNotNullOrEmpty()][string]$SignalExePath,
  [Parameter(Mandatory)][ValidatePattern('^[0-9a-fA-F]{64}$')][string]$SignalExeSha256,
  [Parameter(Mandatory)][ValidateNotNullOrEmpty()][string]$SignalPublisherSubject,
  [ValidateRange(10,90)][int]$StopTimeoutSeconds=30,
  [ValidateRange(10,120)][int]$LaunchTimeoutSeconds=60
)
$ErrorActionPreference='Stop'
Set-StrictMode -Version Latest
Add-Type -AssemblyName System.Net.Http
$path=[IO.Path]::GetFullPath($OslExePath)
$profile=[IO.Path]::GetFullPath($ProfileRoot)
$install=[IO.Path]::GetDirectoryName($path)
$loader=Join-Path $install 'WebView2Loader.dll'
$exeStage=Join-Path $install ".OSL Privacy.exe.$InvocationId.stage"
$loaderStage=Join-Path $install ".WebView2Loader.dll.$InvocationId.stage"
$exeBackup=Join-Path $install "OSL Privacy.exe.pre-$InvocationId"
$loaderBackup=Join-Path $install "WebView2Loader.dll.pre-$InvocationId"
$signalPath=[IO.Path]::GetFullPath($SignalExePath)
$signalExpected=$SignalExeSha256.ToLowerInvariant()
$taskName="OSL-QA-Signal-Deploy-$InvocationId"
$exeExpected=$ExeSha256.ToLowerInvariant(); $loaderExpected=$WebView2LoaderSha256.ToLowerInvariant()
function Get-Hash([string]$p){(Get-FileHash -LiteralPath $p -Algorithm SHA256).Hash.ToLowerInvariant()}
function Save-Artifact([uri]$uri,[string]$destination,[string]$expected){
  if($uri.Scheme -cne 'https' -or $uri.Query){throw 'artifact URI must be query-free HTTPS'}
  $tokenUri='http://169.254.169.254/metadata/identity/oauth2/token?api-version=2018-02-01&resource=https%3A%2F%2Fstorage.azure.com%2F'
  $token=Invoke-RestMethod -Method Get -Uri $tokenUri -Headers @{Metadata='true'} -TimeoutSec 10
  if(-not $token.access_token){throw 'managed identity storage token unavailable'}
  $download="$destination.download"; $client=[Net.Http.HttpClient]::new()
  try{
    $client.Timeout=[TimeSpan]::FromSeconds(30)
    $client.DefaultRequestHeaders.Authorization=[Net.Http.Headers.AuthenticationHeaderValue]::new('Bearer',[string]$token.access_token)
    $client.DefaultRequestHeaders.Add('x-ms-version','2023-11-03')
    $response=$client.GetAsync($uri).GetAwaiter().GetResult()
    if(-not $response.IsSuccessStatusCode){throw 'artifact download failed'}
    [IO.File]::WriteAllBytes($download,$response.Content.ReadAsByteArrayAsync().GetAwaiter().GetResult())
    if((Get-Hash $download) -cne $expected){throw 'downloaded artifact hash mismatch'}
    [IO.File]::Move($download,$destination)
  }finally{$client.Dispose();if(Test-Path -LiteralPath $download){Remove-Item -LiteralPath $download -Force}}
}
function Get-ExactOslProcesses{@(Get-CimInstance Win32_Process -Filter "Name = 'OSL Privacy.exe'"|Where-Object{[int]$_.SessionId -eq $SessionId -and $_.ExecutablePath -and [string]::Equals([IO.Path]::GetFullPath([string]$_.ExecutablePath),$path,[StringComparison]::OrdinalIgnoreCase)})}
function Test-IsSignalGuardian($process){[string]$process.CommandLine -match '(?:^|\s)--osl-signal-window-guardian-v1(?:\s|$)'}
function Get-ExactPrimaryProcesses{@(Get-ExactOslProcesses|Where-Object{-not(Test-IsSignalGuardian $_)})}
function Get-ExactSignalGuardians{@(Get-ExactOslProcesses|Where-Object{Test-IsSignalGuardian $_})}
function Get-SignalPids{@(Get-CimInstance Win32_Process -Filter "Name = 'Signal.exe'"|Where-Object{$_.ExecutablePath -and [string]::Equals([IO.Path]::GetFullPath([string]$_.ExecutablePath),$signalPath,[StringComparison]::OrdinalIgnoreCase)}|ForEach-Object{[int]$_.ProcessId}|Sort-Object -Unique)}
if(-not(Test-Path -LiteralPath $profile -PathType Container)){throw 'configured profile root is missing'}
$exePresent=Test-Path -LiteralPath $path -PathType Leaf;$loaderPresent=Test-Path -LiteralPath $loader -PathType Leaf
if($exePresent -xor $loaderPresent){throw 'configured installation is incomplete'}
$initialInstall=-not $exePresent
if(-not(Test-Path -LiteralPath $signalPath -PathType Leaf)){throw 'configured Signal executable is missing'}
if((Get-Hash $signalPath) -cne $signalExpected){throw 'configured Signal executable hash mismatch'}
$signalSignature=Get-AuthenticodeSignature -LiteralPath $signalPath
if($signalSignature.Status -cne 'Valid' -or $signalSignature.SignerCertificate.Subject -cne $SignalPublisherSubject){throw 'configured Signal publisher verification failed'}
$explorers=@(Get-CimInstance Win32_Process -Filter "Name = 'explorer.exe'"|Where-Object{[int]$_.SessionId -eq $SessionId})
if($explorers.Count -ne 1){throw 'interactive Explorer session is unavailable or ambiguous'}
$owner=Invoke-CimMethod -InputObject $explorers[0] -MethodName GetOwner
if($owner.ReturnValue -ne 0 -or $owner.User -cne $WindowsUser -or -not $owner.Domain){throw 'interactive session owner mismatch'}
$interactiveUser="$($owner.Domain)\$($owner.User)"
foreach($reserved in @($exeStage,$loaderStage,$exeBackup,$loaderBackup)){if(Test-Path -LiteralPath $reserved){throw 'invocation staging path already exists'}}
$signalBefore=@(Get-SignalPids);if($signalBefore.Count -lt 1){throw 'exact Signal process set is empty'};$replacedExe=$false;$replacedLoader=$false;$installedExe=$false;$installedLoader=$false;$taskRegistered=$false
try{
  if($initialInstall -and -not(Test-Path -LiteralPath $install -PathType Container)){
    $installParent=[IO.Path]::GetDirectoryName($install)
    if(-not(Test-Path -LiteralPath $installParent -PathType Container)){throw 'configured installation parent is missing'}
    [void](New-Item -ItemType Directory -Path $install)
  }
  Save-Artifact $ExeUri $exeStage $exeExpected;Save-Artifact $WebView2LoaderUri $loaderStage $loaderExpected
  $primaryBefore=@(Get-ExactPrimaryProcesses);if(($initialInstall -and $primaryBefore.Count -ne 0) -or (-not $initialInstall -and $primaryBefore.Count -ne 1)){throw 'exact OSL primary process state is unavailable or ambiguous'}
  $guardianBefore=@(Get-ExactSignalGuardians);if($guardianBefore.Count -gt 1){throw 'exact Signal guardian process is ambiguous'}
  if(-not $initialInstall){
    $primaryBefore|ForEach-Object{Stop-Process -Id ([int]$_.ProcessId) -Force}
    $deadline=[DateTime]::UtcNow.AddSeconds($StopTimeoutSeconds)
    while((@(Get-ExactPrimaryProcesses).Count -or @(Get-ExactSignalGuardians).Count) -and [DateTime]::UtcNow -lt $deadline){Start-Sleep -Milliseconds 200}
    if(@(Get-ExactPrimaryProcesses).Count){throw 'exact OSL primary process did not stop'}
    if(@(Get-ExactSignalGuardians).Count){throw 'Signal guardian did not restore and exit before replacement'}
    [IO.File]::Replace($exeStage,$path,$exeBackup,$true);$replacedExe=$true
    [IO.File]::Replace($loaderStage,$loader,$loaderBackup,$true);$replacedLoader=$true
  }else{
    if($guardianBefore.Count){throw 'Signal guardian exists without an installed OSL primary'}
    [IO.File]::Move($exeStage,$path);$installedExe=$true
    [IO.File]::Move($loaderStage,$loader);$installedLoader=$true
  }
  if((Get-Hash $path) -cne $exeExpected -or (Get-Hash $loader) -cne $loaderExpected){throw 'installed artifact hash mismatch'}
  $action=New-ScheduledTaskAction -Execute $path -WorkingDirectory $install
  $principal=New-ScheduledTaskPrincipal -UserId $interactiveUser -LogonType Interactive -RunLevel Limited
  $settings=New-ScheduledTaskSettingsSet -ExecutionTimeLimit ([TimeSpan]::FromMinutes(2)) -MultipleInstances IgnoreNew
  Register-ScheduledTask -TaskName $taskName -Action $action -Principal $principal -Settings $settings|Out-Null;$taskRegistered=$true;Start-ScheduledTask -TaskName $taskName
  $deadline=[DateTime]::UtcNow.AddSeconds($LaunchTimeoutSeconds)
  do{$running=@(Get-ExactPrimaryProcesses);if($running.Count -eq 1){break};if($running.Count -gt 1){throw 'exact launch is ambiguous'};Start-Sleep -Milliseconds 250}while([DateTime]::UtcNow -lt $deadline)
  if($running.Count -ne 1){throw 'exact OSL process did not relaunch'}
  $signalAfter=@(Get-SignalPids)
  if($signalBefore.Count -ne $signalAfter.Count -or (Compare-Object $signalBefore $signalAfter)){throw 'Signal PID set changed'}
  [pscustomobject]@{Status='installedPreservedAndLaunched';InvocationId=$InvocationId;ExeSha256=$exeExpected;WebView2LoaderSha256=$loaderExpected;SessionId=$SessionId;ProfilesPreserved=$true;SignalPidSetUnchanged=$true}|ConvertTo-Json -Compress
}catch{
  if($replacedLoader -and (Test-Path -LiteralPath $loaderBackup)){if(Test-Path $loader){[IO.File]::Replace($loaderBackup,$loader,$null,$true)}else{[IO.File]::Move($loaderBackup,$loader)}}
  if($replacedExe -and (Test-Path -LiteralPath $exeBackup)){if(Test-Path $path){[IO.File]::Replace($exeBackup,$path,$null,$true)}else{[IO.File]::Move($exeBackup,$path)}}
  if($installedExe -or $installedLoader){@(Get-ExactPrimaryProcesses)|ForEach-Object{Stop-Process -Id ([int]$_.ProcessId) -Force -ErrorAction SilentlyContinue};$deadline=[DateTime]::UtcNow.AddSeconds(10);while(@(Get-ExactPrimaryProcesses).Count -and [DateTime]::UtcNow -lt $deadline){Start-Sleep -Milliseconds 100}}
  if($installedLoader -and (Test-Path -LiteralPath $loader)){Remove-Item -LiteralPath $loader -Force}
  if($installedExe -and (Test-Path -LiteralPath $path)){Remove-Item -LiteralPath $path -Force}
  throw
}finally{
  if($taskRegistered){Unregister-ScheduledTask -TaskName $taskName -Confirm:$false -ErrorAction SilentlyContinue}
  foreach($stage in @($exeStage,$loaderStage)){if(Test-Path -LiteralPath $stage){Remove-Item -LiteralPath $stage -Force}}
}
