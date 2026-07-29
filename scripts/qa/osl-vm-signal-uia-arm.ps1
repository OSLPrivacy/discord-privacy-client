# OSL-QA-Capability: signal-uia-arm/v1
param(
  [Parameter(Mandatory)][ValidatePattern('^[a-z0-9][a-z0-9-]{7,63}$')][string]$InvocationId,
  [Parameter(Mandatory)][uri]$HarnessUri,
  [Parameter(Mandatory)][ValidatePattern('^[0-9a-fA-F]{64}$')][string]$HarnessSha256,
  [Parameter(Mandatory)][ValidateSet('Inventory','ClaimExactWindow','CaptureSafeChrome')][string]$Action,
  [Parameter(Mandatory)][ValidateRange(1,128)][int]$SessionId,
  [Parameter(Mandatory)][ValidateNotNullOrEmpty()][string]$WindowsUser,
  [Parameter(Mandatory)][ValidateNotNullOrEmpty()][string]$ProfileRoot,
  [Parameter(Mandatory)][ValidateNotNullOrEmpty()][string]$OslExePath,
  [Parameter(Mandatory)][ValidateNotNullOrEmpty()][string]$SignalExePath,
  [Parameter(Mandatory)][ValidatePattern('^[0-9a-fA-F]{64}$')][string]$SignalExeSha256,
  [Parameter(Mandatory)][ValidateNotNullOrEmpty()][string]$SignalPublisherSubject,
  [Parameter(Mandatory)][ValidatePattern('^[0-9a-fA-F]{64}$')][string]$OslExeSha256,
  [Parameter(Mandatory)][ValidatePattern('^[A-Za-z0-9._-]{1,48}$')][string]$CaseId,
  [uri]$UploadUri,
  [ValidateSet('On','Off')][string]$Visibility='On',[ValidateRange(10,180)][int]$TimeoutSeconds=60
)
$ErrorActionPreference='Stop';Set-StrictMode -Version Latest;Add-Type -AssemblyName System.Net.Http
$path=[IO.Path]::GetFullPath($OslExePath);$profile=[IO.Path]::GetFullPath($ProfileRoot);$signalPath=[IO.Path]::GetFullPath($SignalExePath)
if(-not(Test-Path -LiteralPath $profile -PathType Container)){throw 'configured profile root is missing'}
if(-not(Test-Path -LiteralPath $signalPath -PathType Leaf)){throw 'configured Signal executable is missing'}
if((Get-FileHash $signalPath -Algorithm SHA256).Hash.ToLowerInvariant() -cne $SignalExeSha256.ToLowerInvariant()){throw 'configured Signal executable hash mismatch'}
$signalSignature=Get-AuthenticodeSignature -LiteralPath $signalPath;if($signalSignature.Status -cne 'Valid' -or $signalSignature.SignerCertificate.Subject -cne $SignalPublisherSubject){throw 'configured Signal publisher verification failed'}
if(-not(Test-Path -LiteralPath $path -PathType Leaf) -or (Get-FileHash $path -Algorithm SHA256).Hash.ToLowerInvariant() -cne $OslExeSha256.ToLowerInvariant()){throw 'exact OSL executable is missing or hash-mismatched'}
$explorers=@(Get-CimInstance Win32_Process -Filter "Name = 'explorer.exe'"|Where-Object{[int]$_.SessionId -eq $SessionId});if($explorers.Count -ne 1){throw 'interactive Explorer session is unavailable or ambiguous'}
$owner=Invoke-CimMethod -InputObject $explorers[0] -MethodName GetOwner;if($owner.ReturnValue -ne 0 -or $owner.User -cne $WindowsUser -or -not $owner.Domain){throw 'interactive session owner mismatch'};$interactiveUser="$($owner.Domain)\$($owner.User)"
$processes=@(Get-CimInstance Win32_Process -Filter "Name = 'OSL Privacy.exe'"|Where-Object{[int]$_.SessionId -eq $SessionId -and $_.ExecutablePath -and [string]::Equals([IO.Path]::GetFullPath([string]$_.ExecutablePath),$path,[StringComparison]::OrdinalIgnoreCase) -and ([string]$_.CommandLine -notmatch '(?:^|\s)--osl-signal-window-guardian-v1(?:\s|$)')});if($processes.Count -ne 1){throw 'exact running OSL primary process is unavailable or ambiguous'}
$signalProcesses=@(Get-CimInstance Win32_Process -Filter "Name = 'Signal.exe'"|Where-Object{[int]$_.SessionId -eq $SessionId -and $_.ExecutablePath -and [string]::Equals([IO.Path]::GetFullPath([string]$_.ExecutablePath),$signalPath,[StringComparison]::OrdinalIgnoreCase)});if($signalProcesses.Count -lt 1){throw 'exact Signal process set is empty'}
if($HarnessUri.Scheme -cne 'https' -or $HarnessUri.Query){throw 'harness URI must be query-free HTTPS'}
if($Action -ceq 'CaptureSafeChrome'){
  if(-not $UploadUri -or $UploadUri.Scheme -cne 'https' -or $UploadUri.Query -or $UploadUri.Fragment){throw 'safe screenshot upload URI must be query-free HTTPS'}
}elseif($UploadUri){throw 'upload URI is allowed only for safe screenshot capture'}
$root=Join-Path 'C:\ProgramData\OSL-QA\signal-uia' $InvocationId;if(Test-Path $root){throw 'invocation ID already exists'};[void](New-Item -ItemType Directory -Path $root)
$harness=Join-Path $root 'harness.ps1';$download="$harness.download";$requestPath=Join-Path $root 'request.clixml';$wrapperPath=Join-Path $root 'runner.ps1';$resultPath=Join-Path $root 'result.json';$taskName="OSL-QA-Signal-$InvocationId"
$token=Invoke-RestMethod -Method Get -Uri 'http://169.254.169.254/metadata/identity/oauth2/token?api-version=2018-02-01&resource=https%3A%2F%2Fstorage.azure.com%2F' -Headers @{Metadata='true'} -TimeoutSec 10
$client=[Net.Http.HttpClient]::new();try{$client.DefaultRequestHeaders.Authorization=[Net.Http.Headers.AuthenticationHeaderValue]::new('Bearer',[string]$token.access_token);$client.DefaultRequestHeaders.Add('x-ms-version','2023-11-03');$response=$client.GetAsync($HarnessUri).GetAwaiter().GetResult();if(-not $response.IsSuccessStatusCode){throw 'harness download failed'};[IO.File]::WriteAllBytes($download,$response.Content.ReadAsByteArrayAsync().GetAwaiter().GetResult());if((Get-FileHash $download -Algorithm SHA256).Hash.ToLowerInvariant() -cne $HarnessSha256.ToLowerInvariant()){throw 'harness hash mismatch'};[IO.File]::Move($download,$harness)}finally{$client.Dispose();Remove-Item $download -Force -ErrorAction SilentlyContinue}
$request=[pscustomobject]@{InvocationId=$InvocationId;Action=$Action;OslExePath=$path;OslExeSha256=$OslExeSha256.ToLowerInvariant();SessionId=$SessionId;SignalExePath=$signalPath;CaseId=$CaseId;UploadUri=$UploadUri;Visibility=$Visibility;TimeoutSeconds=$TimeoutSeconds;HarnessPath=$harness;ResultPath=$resultPath};$request|Export-Clixml -LiteralPath $requestPath
$wrapper=@'
param([string]$RequestPath)
$ErrorActionPreference='Stop';$r=Import-Clixml -LiteralPath $RequestPath;$tmp="$($r.ResultPath).tmp"
try{$raw=& $r.HarnessPath -Action $r.Action -OslExePath $r.OslExePath -OslExeSha256 $r.OslExeSha256 -SessionId $r.SessionId -SignalExePath $r.SignalExePath -CaseId $r.CaseId -UploadUri $r.UploadUri -Visibility $r.Visibility -TimeoutSeconds $r.TimeoutSeconds;$h=$raw|ConvertFrom-Json;$out=@{Terminal=$true;InvocationId=$r.InvocationId;Status=if($h.Ok){'completed'}else{'harnessFailed'};HarnessResult=$h}}catch{$out=@{Terminal=$true;InvocationId=$r.InvocationId;Status='harnessFailed';HarnessResult=@{Ok=$false;Detail=if($_.Exception.Message.Length -le 120){$_.Exception.Message}else{$_.Exception.Message.Substring(0,120)}}}}
[IO.File]::WriteAllText($tmp,($out|ConvertTo-Json -Depth 12 -Compress),[Text.UTF8Encoding]::new($false));[IO.File]::Move($tmp,$r.ResultPath)
'@
[IO.File]::WriteAllText($wrapperPath,$wrapper,[Text.UTF8Encoding]::new($false))
$taskAction=New-ScheduledTaskAction -Execute 'powershell.exe' -Argument ('-NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "{0}" -RequestPath "{1}"'-f$wrapperPath,$requestPath)
$principal=New-ScheduledTaskPrincipal -UserId $interactiveUser -LogonType Interactive -RunLevel Limited
Register-ScheduledTask -TaskName $taskName -Action $taskAction -Principal $principal -Settings (New-ScheduledTaskSettingsSet -ExecutionTimeLimit ([TimeSpan]::FromMinutes(4)))|Out-Null;Start-ScheduledTask $taskName
[pscustomobject]@{Status='armed';InvocationId=$InvocationId;Action=$Action;CaseId=$CaseId;SessionId=$SessionId}|ConvertTo-Json -Compress
