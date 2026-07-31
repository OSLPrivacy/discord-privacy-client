$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$matches = @()
foreach ($explorer in @(Get-CimInstance Win32_Process -Filter "Name = 'explorer.exe'")) {
  $owner = Invoke-CimMethod -InputObject $explorer -MethodName GetOwner
  if ($owner.ReturnValue -eq 0 -and $owner.User -ceq 'osltest' -and $owner.Domain) {
    $matches += $explorer
  }
}

$sessionIds = @()
if ($matches.Count -eq 1) {
  $sessionIds = @([int]$matches[0].SessionId)
} elseif ($matches.Count -eq 0) {
  $oslPath = [IO.Path]::GetFullPath('C:\Users\osltest\Desktop\OSL Privacy\OSL Privacy.exe')
  $oslSessions = @(Get-CimInstance Win32_Process -Filter "Name = 'OSL Privacy.exe'" | Where-Object {
    if (-not $_.ExecutablePath -or -not [string]::Equals(
      [IO.Path]::GetFullPath([string]$_.ExecutablePath), $oslPath, [StringComparison]::OrdinalIgnoreCase
    )) { return $false }
    $owner = Invoke-CimMethod -InputObject $_ -MethodName GetOwner
    return $owner.ReturnValue -eq 0 -and $owner.User -ceq 'osltest'
  } | ForEach-Object { [int]$_.SessionId } | Sort-Object -Unique)
  $whatsAppSessions = @(Get-CimInstance Win32_Process -Filter "Name = 'WhatsApp.Root.exe'" | Where-Object {
    $owner = Invoke-CimMethod -InputObject $_ -MethodName GetOwner
    return $owner.ReturnValue -eq 0 -and $owner.User -ceq 'osltest'
  } | ForEach-Object { [int]$_.SessionId } | Sort-Object -Unique)
  $sessionIds = @($oslSessions | Where-Object { $whatsAppSessions -contains $_ })
}
if ($sessionIds.Count -ne 1 -or $sessionIds[0] -lt 1 -or $sessionIds[0] -gt 128) {
  throw 'exact osltest interactive session is unavailable or ambiguous'
}
[pscustomobject]@{
  Status = 'ready'
  SessionId = [int]$sessionIds[0]
  ProfileRead = $false
  ProviderStorageRead = $false
  ProcessesTerminated = $false
} | ConvertTo-Json -Compress
