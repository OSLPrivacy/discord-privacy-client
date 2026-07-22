$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$matches = @()
foreach ($explorer in @(Get-CimInstance Win32_Process -Filter "Name = 'explorer.exe'")) {
  $owner = Invoke-CimMethod -InputObject $explorer -MethodName GetOwner
  if ($owner.ReturnValue -eq 0 -and $owner.User -ceq 'osltest' -and $owner.Domain) {
    $matches += $explorer
  }
}
if ($matches.Count -ne 1) {
  throw 'exact osltest interactive session is unavailable or ambiguous'
}
[pscustomobject]@{
  Status = 'ready'
  SessionId = [int]$matches[0].SessionId
  ProfileRead = $false
  ProviderStorageRead = $false
} | ConvertTo-Json -Compress
