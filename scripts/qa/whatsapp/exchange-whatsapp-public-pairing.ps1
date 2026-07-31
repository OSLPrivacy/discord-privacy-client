param(
  [Parameter(Mandatory)][ValidateSet(1,2)][int]$ClientNumber,
  [Parameter(Mandatory)][ValidateSet('publish','consume')][string]$Phase,
  [Parameter(Mandatory)][ValidatePattern('^[a-z0-9][a-z0-9-]{7,63}$')][string]$InvocationId,
  [Parameter(Mandatory)][ValidateRange(1,128)][int]$SessionId,
  [ValidatePattern('^$|^[a-f0-9]{64}$')][string]$ExpectedPeerOfferSha256=''
)
$ErrorActionPreference='Stop';Set-StrictMode -Version Latest
Add-Type -AssemblyName System.Net.Http
$accountDir='C:\Users\osltest\AppData\Roaming\org.oslprivacy.hub\osl-core'
$offer=Join-Path $accountDir 'whatsapp-qa-offer.v1.json'
$peerOffer=Join-Path $accountDir 'whatsapp-qa-peer-offer.v1.json'
$status=Join-Path $accountDir 'whatsapp-qa-pairing-status.v1.json'
$oslPath='C:\Users\osltest\Desktop\OSL Privacy\OSL Privacy.exe'
$taskName="OSL-QA-WhatsApp-Pair-$InvocationId-c$ClientNumber"
$container="osl-whatsapp-qa-client$ClientNumber"
$peerNumber=if($ClientNumber -eq 1){2}else{1}
$ownUri="https://osltestartifactsa7d5.blob.core.windows.net/$container/pairing/$InvocationId/offer.json"
$peerUri="https://osltestartifactsa7d5.blob.core.windows.net/osl-whatsapp-qa-client$peerNumber/pairing/$InvocationId/offer.json"

function Get-Sha256Bytes([byte[]]$Bytes){$sha=[Security.Cryptography.SHA256]::Create();try{([BitConverter]::ToString($sha.ComputeHash($Bytes))).Replace('-','').ToLowerInvariant()}finally{$sha.Dispose()}}
function Assert-PublicOffer([byte[]]$Bytes){
  if($Bytes.Length -lt 16 -or $Bytes.Length -gt 16384){throw 'public offer has invalid bounds'}
  $value=[Text.Encoding]::UTF8.GetString($Bytes)|ConvertFrom-Json
  $names=@($value.PSObject.Properties.Name|Sort-Object)
  if(($names -join ',') -cne 'friend_code,osl_user_id,safety_number,version' -or [int]$value.version -ne 1 -or
    -not ([string]$value.friend_code).StartsWith('OSLFR1.') -or ([string]$value.friend_code).Length -gt 8192 -or
    [string]::IsNullOrWhiteSpace([string]$value.osl_user_id) -or ([string]$value.osl_user_id).Length -gt 160 -or
    [string]::IsNullOrWhiteSpace([string]$value.safety_number) -or ([string]$value.safety_number).Length -gt 160){throw 'public offer schema is invalid'}
}
function Get-MiToken(){
  $result=Invoke-RestMethod -Headers @{Metadata='true'} -Method Get -TimeoutSec 10 -Uri 'http://169.254.169.254/metadata/identity/oauth2/token?api-version=2019-08-01&resource=https%3A%2F%2Fstorage.azure.com%2F'
  if(-not $result.access_token){throw 'managed identity unavailable'}
  [string]$result.access_token
}
function New-StorageClient([string]$Token){
  $client=[Net.Http.HttpClient]::new();$client.Timeout=[TimeSpan]::FromSeconds(30)
  $client.DefaultRequestHeaders.Authorization=[Net.Http.Headers.AuthenticationHeaderValue]::new('Bearer',$Token)
  $client.DefaultRequestHeaders.Add('x-ms-version','2023-11-03');$client
}
function Get-WhatsAppSnapshot(){
  @(Get-CimInstance Win32_Process|Where-Object{$_.ExecutablePath -and ([string]$_.ExecutablePath).StartsWith('C:\Program Files\WindowsApps\5319275A.WhatsAppDesktop_',[StringComparison]::OrdinalIgnoreCase)}|Sort-Object ProcessId|ForEach-Object{"$($_.ProcessId):$($_.CreationDate):$($_.ExecutablePath)"})
}
function Get-ExactOsl(){@(Get-CimInstance Win32_Process -Filter "Name = 'OSL Privacy.exe'"|Where-Object{$_.ExecutablePath -and [string]::Equals([IO.Path]::GetFullPath([string]$_.ExecutablePath),$oslPath,[StringComparison]::OrdinalIgnoreCase)})}

if(-not(Test-Path -LiteralPath $offer -PathType Leaf) -or -not(Test-Path -LiteralPath $oslPath -PathType Leaf)){throw 'exact OSL public pairing state is unavailable'}
$whatsAppBefore=@(Get-WhatsAppSnapshot);if($whatsAppBefore.Count -eq 0){throw 'official WhatsApp process evidence is unavailable'}
$token=$null;$client=$null
try{
  $token=Get-MiToken;$client=New-StorageClient $token
  if($Phase -ceq 'publish'){
    $bytes=[IO.File]::ReadAllBytes($offer);Assert-PublicOffer $bytes;$hash=Get-Sha256Bytes $bytes
    $content=[Net.Http.ByteArrayContent]::new($bytes);$content.Headers.ContentType=[Net.Http.Headers.MediaTypeHeaderValue]::new('application/json');$content.Headers.Add('x-ms-blob-type','BlockBlob')
    try{$response=$client.PutAsync($ownUri,$content).GetAwaiter().GetResult();if(-not $response.IsSuccessStatusCode){throw 'managed-identity public offer upload failed'}}finally{$content.Dispose();[Array]::Clear($bytes,0,$bytes.Length)}
    [pscustomobject]@{Schema='whatsapp-public-pairing/v1';Phase='published';ClientNumber=$ClientNumber;OfferSha256=$hash;Terminal=$true;ManagedIdentityOnly=$true;PrivateIdentityRead=$false;WhatsAppPrivateStorageRead=$false;WhatsAppProcessSetUnchanged=((Get-WhatsAppSnapshot)-join '|') -ceq ($whatsAppBefore-join '|');WindowForegrounded=$false}|ConvertTo-Json -Compress
    exit 0
  }
  if(-not $ExpectedPeerOfferSha256){throw 'expected peer offer hash is required'}
  $response=$client.GetAsync($peerUri).GetAwaiter().GetResult();if(-not $response.IsSuccessStatusCode){throw 'managed-identity peer offer download failed'}
  $bytes=$response.Content.ReadAsByteArrayAsync().GetAwaiter().GetResult();Assert-PublicOffer $bytes
  if((Get-Sha256Bytes $bytes) -cne $ExpectedPeerOfferSha256){throw 'peer public offer hash mismatch'}
  $temporary="$peerOffer.$InvocationId.tmp";[IO.File]::WriteAllBytes($temporary,$bytes);[Array]::Clear($bytes,0,$bytes.Length);Move-Item -LiteralPath $temporary -Destination $peerOffer -Force
  $running=@(Get-ExactOsl);if($running.Count -ne 1){throw 'exact OSL process is unavailable or ambiguous'}
  $owner=Invoke-CimMethod -InputObject $running[0] -MethodName GetOwner
  if($owner.ReturnValue -ne 0 -or $owner.User -cne 'osltest' -or -not $owner.Domain){throw 'exact OSL owner is unavailable'}
  $interactiveUser="$($owner.Domain)\$($owner.User)"
  $terminated=Invoke-CimMethod -InputObject $running[0] -MethodName Terminate
  if($terminated.ReturnValue -ne 0){throw 'exact OSL restart could not begin'}
  $action=New-ScheduledTaskAction -Execute $oslPath -WorkingDirectory ([IO.Path]::GetDirectoryName($oslPath))
  $principal=New-ScheduledTaskPrincipal -UserId $interactiveUser -LogonType Interactive -RunLevel Limited
  $settings=New-ScheduledTaskSettingsSet -ExecutionTimeLimit ([TimeSpan]::FromMinutes(2)) -MultipleInstances IgnoreNew
  Register-ScheduledTask -TaskName $taskName -Action $action -Principal $principal -Settings $settings -Force|Out-Null
  try{Start-ScheduledTask -TaskName $taskName;$deadline=[DateTime]::UtcNow.AddSeconds(60);do{Start-Sleep -Milliseconds 250;$next=@(Get-ExactOsl)}while($next.Count -ne 1 -and [DateTime]::UtcNow -lt $deadline);if($next.Count -ne 1){throw 'exact OSL did not relaunch'}}finally{Unregister-ScheduledTask -TaskName $taskName -Confirm:$false -ErrorAction SilentlyContinue}
  $deadline=[DateTime]::UtcNow.AddSeconds(60);$verified=$false
  do{if(Test-Path -LiteralPath $status -PathType Leaf){$pair=Get-Content -LiteralPath $status -Raw|ConvertFrom-Json;$verified=([bool]$pair.verified -and [string]$pair.peer_offer_sha256 -ceq $ExpectedPeerOfferSha256)};if(-not $verified){Start-Sleep -Milliseconds 250}}while(-not $verified -and [DateTime]::UtcNow -lt $deadline)
  if(-not $verified){throw 'OSL did not verify the exact peer offer'}
  [pscustomobject]@{Schema='whatsapp-public-pairing/v1';Phase='consumedAndVerified';ClientNumber=$ClientNumber;PeerOfferSha256=$ExpectedPeerOfferSha256;Terminal=$true;ManagedIdentityOnly=$true;OslPublicPairingStateUpdated=$true;OslProcessRestarted=$true;PrivateIdentityRead=$false;WhatsAppPrivateStorageRead=$false;WhatsAppProcessSetUnchanged=((Get-WhatsAppSnapshot)-join '|') -ceq ($whatsAppBefore-join '|');WindowForegrounded=$false}|ConvertTo-Json -Compress
}finally{if($client){$client.Dispose()};$token=$null}
