param()
$ErrorActionPreference='Stop';Set-StrictMode -Version Latest
$candidates=@(
  'C:\Users\osltest\AppData\Roaming\org.oslprivacy.hub\osl-core\whatsapp-qa-offer.v1.json',
  'C:\Users\osltest\AppData\Roaming\OSL Privacy\osl-core\whatsapp-qa-offer.v1.json',
  'C:\Users\osltest\AppData\Roaming\com.osl.privacy\osl-core\whatsapp-qa-offer.v1.json'
)
$matches=@()
for($index=0;$index-lt $candidates.Count;$index++){
  $path=$candidates[$index]
  if(Test-Path -LiteralPath $path -PathType Leaf){
    $length=(Get-Item -LiteralPath $path).Length
    if($length -lt 16 -or $length -gt 16384){throw 'public offer has invalid bounds'}
    $matches+=,[pscustomobject]@{
      Candidate=$index
      Bytes=$length
      Sha256=(Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant()
    }
  }
}
[pscustomobject]@{
  Schema='whatsapp-public-offer-audit/v1'
  Matches=$matches.Count
  Offers=$matches
  ProfileRead=$false
  WhatsAppStorageRead=$false
  WindowForegrounded=$false
}|ConvertTo-Json -Compress -Depth 4
