param(
  [Parameter(Mandatory)][ValidateSet(1,2)][int]$ClientNumber,
  [Parameter(Mandatory)][ValidatePattern('^[a-z0-9-]{8,80}$')][string]$InvocationId,
  [Parameter(Mandatory)][ValidatePattern('^[0-9a-f]{64}$')][string]$OslExeSha256,
  [Parameter(Mandatory)][ValidateRange(1,128)][int]$SessionId
)
$ErrorActionPreference='Stop';Set-StrictMode -Version Latest
$oslPath=[IO.Path]::GetFullPath('C:\Users\osltest\Desktop\OSL Privacy\OSL Privacy.exe')
if((Get-FileHash -LiteralPath $oslPath -Algorithm SHA256).Hash.ToLowerInvariant()-cne$OslExeSha256){throw'exact OSL QA executable is unavailable'}
$explorer=@(Get-CimInstance Win32_Process -Filter "Name = 'explorer.exe'"|Where-Object{[int]$_.SessionId-eq$SessionId})
if($explorer.Count-ne1){throw'interactive session is unavailable or ambiguous'}
$owner=Invoke-CimMethod -InputObject $explorer[0] -MethodName GetOwner
if($owner.ReturnValue-ne0-or$owner.User-cne'osltest'-or-not$owner.Domain){throw'interactive owner is invalid'}
$interactiveUser="$($owner.Domain)\$($owner.User)"
$whatsAppBefore=@(Get-CimInstance Win32_Process|Where-Object{[int]$_.SessionId-eq$SessionId-and$_.ExecutablePath-and[IO.Path]::GetFileName([string]$_.ExecutablePath)-ceq'WhatsApp.Root.exe'}|Sort-Object ProcessId|ForEach-Object{"$($_.ProcessId)|$($_.CreationDate)|$($_.ExecutablePath)"})
if($whatsAppBefore.Count-ne1){throw'exact WhatsApp process is unavailable or ambiguous'}
$osl=@(Get-CimInstance Win32_Process|Where-Object{[int]$_.SessionId-eq$SessionId-and$_.ExecutablePath-and[IO.Path]::GetFullPath([string]$_.ExecutablePath).Equals($oslPath,[StringComparison]::OrdinalIgnoreCase)})
if($osl.Count-lt1-or$osl.Count-gt2){throw'exact OSL process set is unavailable or ambiguous'}
$osl|ForEach-Object{Stop-Process -Id ([int]$_.ProcessId) -Force}
$deadline=[DateTime]::UtcNow.AddSeconds(15)
do{
 $remaining=@(Get-CimInstance Win32_Process|Where-Object{$_.ExecutablePath-and[IO.Path]::GetFullPath([string]$_.ExecutablePath).Equals($oslPath,[StringComparison]::OrdinalIgnoreCase)})
 if($remaining.Count-eq0){break};Start-Sleep -Milliseconds 200
}while([DateTime]::UtcNow-lt$deadline)
if($remaining.Count-ne0){throw'exact OSL process did not stop within deadline'}
$taskName="OSL-WA-Restart-$InvocationId"
$action=New-ScheduledTaskAction -Execute $oslPath -WorkingDirectory ([IO.Path]::GetDirectoryName($oslPath))
$principal=New-ScheduledTaskPrincipal -UserId $interactiveUser -LogonType Interactive -RunLevel Limited
Register-ScheduledTask -TaskName $taskName -Action $action -Principal $principal -Settings (New-ScheduledTaskSettingsSet -ExecutionTimeLimit ([TimeSpan]::FromMinutes(2))) -Force|Out-Null
try{
 Start-ScheduledTask -TaskName $taskName;$deadline=[DateTime]::UtcNow.AddSeconds(30)
 do{
  $running=@(Get-CimInstance Win32_Process|Where-Object{[int]$_.SessionId-eq$SessionId-and$_.ExecutablePath-and[IO.Path]::GetFullPath([string]$_.ExecutablePath).Equals($oslPath,[StringComparison]::OrdinalIgnoreCase)})
  if($running.Count-ge1){break};Start-Sleep -Milliseconds 200
 }while([DateTime]::UtcNow-lt$deadline)
 if($running.Count-lt1-or$running.Count-gt2){throw'exact OSL process did not relaunch'}
 Start-Sleep -Seconds 3
 $whatsAppAfter=@(Get-CimInstance Win32_Process|Where-Object{[int]$_.SessionId-eq$SessionId-and$_.ExecutablePath-and[IO.Path]::GetFileName([string]$_.ExecutablePath)-ceq'WhatsApp.Root.exe'}|Sort-Object ProcessId|ForEach-Object{"$($_.ProcessId)|$($_.CreationDate)|$($_.ExecutablePath)"})
 if((ConvertTo-Json $whatsAppBefore -Compress)-cne(ConvertTo-Json $whatsAppAfter -Compress)){throw'WhatsApp process changed during OSL-only restart'}
 [pscustomobject]@{Schema='whatsapp-osl-restart/v1';Status='restartedPreserved';Terminal=$true;ClientNumber=$ClientNumber;InvocationId=$InvocationId;OslExeSha256=$OslExeSha256;WhatsAppProcessUnchanged=$true;WhatsAppProfileTouched=$false;ProviderStorageRead=$false;LocalRdpForegrounded=$false}|ConvertTo-Json -Compress
}finally{Unregister-ScheduledTask -TaskName $taskName -Confirm:$false -ErrorAction SilentlyContinue}
