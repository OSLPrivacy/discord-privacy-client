param(
  [Parameter(Mandatory)][ValidateSet(1, 2)][int]$ClientNumber
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

function Safe([string]$Name) {
  @(Get-CimInstance Win32_Process -Filter "Name = '$Name'" | ForEach-Object {
    $owner = Invoke-CimMethod -InputObject $_ -MethodName GetOwner
    [pscustomobject]@{
      SessionId = [int]$_.SessionId
      OwnerIsOsltest = ($owner.ReturnValue -eq 0 -and $owner.User -ceq 'osltest')
    }
  })
}

function WhatsAppTwoClientQualification([int]$ClientNumber, [array]$Explorer, [array]$Osl, [array]$WhatsApp) {
  $allowedPair = @('OSL-WhatsApp-Client-1', 'OSL-WhatsApp-Client-2')
  $expectedVm = $allowedPair[$ClientNumber - 1]
  $explorerSessions = @($Explorer | Where-Object { $_.OwnerIsOsltest } | ForEach-Object { [int]$_.SessionId } | Sort-Object -Unique)
  $oslSessions = @($Osl | Where-Object { $_.OwnerIsOsltest } | ForEach-Object { [int]$_.SessionId } | Sort-Object -Unique)
  $whatsAppSessions = @($WhatsApp | Where-Object { $_.OwnerIsOsltest } | ForEach-Object { [int]$_.SessionId } | Sort-Object -Unique)
  $sharedSessions = @($explorerSessions | Where-Object { $oslSessions -contains $_ -and $whatsAppSessions -contains $_ })
  $qualified = (
    $allowedPair -contains $expectedVm -and
    $explorerSessions.Count -eq 1 -and
    $oslSessions.Count -ge 1 -and
    $whatsAppSessions.Count -ge 1 -and
    $sharedSessions.Count -eq 1
  )
  $status = if ($qualified) { 'qualified' } else { 'failedClosed' }
  $sessionId = if ($sharedSessions.Count -eq 1) { [int]$sharedSessions[0] } else { $null }
  [pscustomobject]@{
    Schema = 'whatsapp-two-client-qualification/v1'
    Status = $status
    ClientNumber = $ClientNumber
    ExpectedVmName = $expectedVm
    AllowedPair = $allowedPair
    SessionId = $sessionId
    ExplorerOsltestSessionCount = $explorerSessions.Count
    OslOsltestSessionCount = $oslSessions.Count
    WhatsAppOsltestSessionCount = $whatsAppSessions.Count
    ProfileRead = $false
    ProviderStorageRead = $false
    ContentRead = $false
    WindowForegrounded = $false
    ProcessesTerminated = $false
  }
}

$explorer = @(Safe 'explorer.exe')
$osl = @(Safe 'OSL Privacy.exe')
$whatsapp = @(Safe 'WhatsApp.Root.exe')
$qualification = WhatsAppTwoClientQualification $ClientNumber $explorer $osl $whatsapp
[pscustomobject]@{
  Schema = 'whatsapp-live-session-audit/v1'
  ClientNumber = $ClientNumber
  Qualification = $qualification
  Explorer = $explorer
  Osl = $osl
  WhatsApp = $whatsapp
  ProfileRead = $false
  ProviderStorageRead = $false
  ContentRead = $false
  WindowForegrounded = $false
  ProcessesTerminated = $false
} | ConvertTo-Json -Compress -Depth 5
