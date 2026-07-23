$ErrorActionPreference='Stop';Set-StrictMode -Version Latest
function Safe([string]$Name){
  @(Get-CimInstance Win32_Process -Filter "Name = '$Name'"|ForEach-Object{
    $owner=Invoke-CimMethod -InputObject $_ -MethodName GetOwner
    [pscustomobject]@{SessionId=[int]$_.SessionId;OwnerIsOsltest=($owner.ReturnValue -eq 0 -and $owner.User -ceq 'osltest')}
  })
}
$explorer=@(Safe 'explorer.exe');$osl=@(Safe 'OSL Privacy.exe');$whatsapp=@(Safe 'WhatsApp.Root.exe')
[pscustomobject]@{Schema='whatsapp-live-session-audit/v1';Explorer=$explorer;Osl=$osl;WhatsApp=$whatsapp;ProfileRead=$false;ProviderStorageRead=$false;ContentRead=$false;WindowForegrounded=$false;ProcessesTerminated=$false}|ConvertTo-Json -Compress -Depth 4
