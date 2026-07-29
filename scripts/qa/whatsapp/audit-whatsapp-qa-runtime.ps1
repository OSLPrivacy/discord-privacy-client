# OSL-QA-Capability: audit-whatsapp-qa-runtime/v1
param(
  [Parameter(Mandatory)][ValidateRange(1,2)][int]$ClientNumber,
  [Parameter(Mandatory)][ValidatePattern('^[a-z0-9][a-z0-9-]{7,52}$')][string]$InvocationId,
  [Parameter(Mandatory)][ValidatePattern('^[0-9a-fA-F]{64}$')][string]$OslExeSha256
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$oslPath = [IO.Path]::GetFullPath('C:\Users\osltest\Desktop\OSL Privacy\OSL Privacy.exe')
$statusPath = [IO.Path]::GetFullPath('C:\Users\osltest\Desktop\OSL Privacy\whatsapp-qa-runtime-status.v1.json')
$expected = $OslExeSha256.ToLowerInvariant()
if (-not (Test-Path -LiteralPath $oslPath -PathType Leaf) -or
    (Get-FileHash -LiteralPath $oslPath -Algorithm SHA256).Hash.ToLowerInvariant() -cne $expected) {
  throw 'exact OSL QA executable is missing or hash-mismatched'
}
$processes = @(Get-CimInstance Win32_Process -Filter "Name = 'OSL Privacy.exe'" | Where-Object {
  $_.ExecutablePath -and
  [string]::Equals([IO.Path]::GetFullPath([string]$_.ExecutablePath),$oslPath,[StringComparison]::OrdinalIgnoreCase) -and
  ([string]$_.CommandLine -notmatch '(?:^|\s)--osl-whatsapp-window-guardian-v1(?:\s|$)')
})
if ($processes.Count -ne 1) { throw 'exact OSL QA primary process is unavailable or ambiguous' }

$deadline = [DateTime]::UtcNow.AddSeconds(30)
while (-not (Test-Path -LiteralPath $statusPath -PathType Leaf) -and [DateTime]::UtcNow -lt $deadline) { Start-Sleep -Milliseconds 100 }
if (-not (Test-Path -LiteralPath $statusPath -PathType Leaf)) { throw 'bounded QA runtime receipt is unavailable' }
$processStarted = ([DateTime]$processes[0].CreationDate).ToUniversalTime()
$statusFile = Get-Item -LiteralPath $statusPath
if ($statusFile.LastWriteTimeUtc -lt $processStarted.AddSeconds(-2)) { throw 'QA runtime receipt predates the exact running process' }
$raw = Get-Content -LiteralPath $statusPath -Raw
if ([Text.Encoding]::UTF8.GetByteCount($raw) -gt 4096) { throw 'QA runtime receipt exceeds its fixed bound' }
$receipt = $raw | ConvertFrom-Json
$keys = @($receipt.PSObject.Properties.Name | Sort-Object)
$expectedKeys = @('browserFallbackUsed','failureCode','hostReceipt','nativeWindowClaimed','phase','protectedControlsEnabled','providerContentRead','providerPrivateStorageRead','schema') | Sort-Object
if (Compare-Object $keys $expectedKeys) { throw 'QA runtime receipt shape changed' }
if ($receipt.schema -cne 'whatsapp-qa-runtime-status/v1' -or
    $receipt.phase -notin @('coreReady','nativeWindowClaimed','protectedControlsReady','visualBindingFailed','protectedProbeDispatched','protectedProbeDispatchFailed','protectedProbePreparationFailed','failedClosed') -or
    $receipt.protectedControlsEnabled -notin @($true,$false) -or $receipt.browserFallbackUsed -ne $false -or
    $receipt.providerContentRead -ne $false -or $receipt.providerPrivateStorageRead -ne $false) {
  throw 'QA runtime receipt semantics failed closed'
}
if (($receipt.phase -cin @('nativeWindowClaimed','protectedControlsReady','visualBindingFailed','protectedProbeDispatched','protectedProbeDispatchFailed','protectedProbePreparationFailed')) -ne [bool]$receipt.nativeWindowClaimed) { throw 'QA runtime claim state is inconsistent' }
if (($receipt.phase -ceq 'protectedControlsReady') -ne [bool]$receipt.protectedControlsEnabled) { throw 'QA runtime protection state is inconsistent' }

$audit = [ordered]@{
  Schema='whatsapp-qa-runtime-audit/v1'
  Status='audited'
  ExeSha256=$expected
  Phase=[string]$receipt.phase
  NativeWindowClaimed=[bool]$receipt.nativeWindowClaimed
  ProtectedControlsEnabled=[bool]$receipt.protectedControlsEnabled
  FailureCode=if($receipt.failureCode){[string]$receipt.failureCode}else{$null}
  BrowserFallbackUsed=$false
  ProviderContentRead=$false
  WhatsAppPrivateStorageRead=$false
  ProfileRead=$false
  InputSent=($receipt.phase -cin @('protectedProbeDispatched','protectedProbeDispatchFailed'))
  WindowForegrounded=$false
}
$auditJson = $audit | ConvertTo-Json -Compress
$token = Invoke-RestMethod -Method Get -Uri 'http://169.254.169.254/metadata/identity/oauth2/token?api-version=2019-08-01&resource=https%3A%2F%2Fstorage.azure.com%2F' -Headers @{Metadata='true'} -TimeoutSec 10
if (-not $token.access_token) { throw 'managed identity is unavailable for the audit receipt' }
$receiptUri = "https://osltestartifactsa7d5.blob.core.windows.net/osl-whatsapp-qa-client$ClientNumber/audits/$InvocationId.json"
[void](Invoke-WebRequest -UseBasicParsing -Method Put -Uri $receiptUri -Headers @{
  Authorization="Bearer $($token.access_token)"
  'x-ms-version'='2023-11-03'
  'x-ms-blob-type'='BlockBlob'
  'Content-Type'='application/json'
} -Body ([Text.Encoding]::UTF8.GetBytes($auditJson)) -TimeoutSec 20)
$token = $null
$auditJson
