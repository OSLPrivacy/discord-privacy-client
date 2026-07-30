# OSL-QA-Capability: signal-exact-build-audit/v1
param(
  [Parameter(Mandatory)][ValidateRange(1, 128)][int]$SessionId,
  [Parameter(Mandatory)][ValidateNotNullOrEmpty()][string]$WindowsUser,
  [Parameter(Mandatory)][ValidateNotNullOrEmpty()][string]$ProfileRoot,
  [Parameter(Mandatory)][ValidateNotNullOrEmpty()][string]$OslExePath,
  [Parameter(Mandatory)][ValidatePattern('^[0-9a-fA-F]{64}$')][string]$OslExeSha256,
  [Parameter(Mandatory)][ValidatePattern('^[0-9a-fA-F]{64}$')][string]$WebView2LoaderSha256,
  [Parameter(Mandatory)][ValidateNotNullOrEmpty()][string]$SignalExePath,
  [Parameter(Mandatory)][ValidatePattern('^[0-9a-fA-F]{64}$')][string]$SignalExeSha256,
  [Parameter(Mandatory)][ValidateNotNullOrEmpty()][string]$SignalPublisherSubject
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$path = [IO.Path]::GetFullPath($OslExePath)
$profile = [IO.Path]::GetFullPath($ProfileRoot)
$signalPath = [IO.Path]::GetFullPath($SignalExePath)
$loader = Join-Path ([IO.Path]::GetDirectoryName($path)) 'WebView2Loader.dll'
if (-not (Test-Path -LiteralPath $profile -PathType Container)) { throw 'configured profile root is missing' }
if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { throw 'configured OSL executable is missing' }
if (-not (Test-Path -LiteralPath $loader -PathType Leaf)) { throw 'configured WebView2 loader is missing' }
if (-not (Test-Path -LiteralPath $signalPath -PathType Leaf)) { throw 'configured Signal executable is missing' }
if ((Get-FileHash -LiteralPath $signalPath -Algorithm SHA256).Hash.ToLowerInvariant() -cne $SignalExeSha256.ToLowerInvariant()) { throw 'Signal executable hash mismatch' }
$signalSignature = Get-AuthenticodeSignature -LiteralPath $signalPath
if ($signalSignature.Status -cne 'Valid' -or $signalSignature.SignerCertificate.Subject -cne $SignalPublisherSubject) { throw 'Signal publisher verification failed' }
if ((Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant() -cne $OslExeSha256.ToLowerInvariant()) { throw 'OSL executable hash mismatch' }
if ((Get-FileHash -LiteralPath $loader -Algorithm SHA256).Hash.ToLowerInvariant() -cne $WebView2LoaderSha256.ToLowerInvariant()) { throw 'WebView2 loader hash mismatch' }
$explorers = @(Get-CimInstance Win32_Process -Filter "Name = 'explorer.exe'" | Where-Object { [int]$_.SessionId -eq $SessionId })
if ($explorers.Count -ne 1) { throw 'interactive Explorer session is unavailable or ambiguous' }
$owner = Invoke-CimMethod -InputObject $explorers[0] -MethodName GetOwner
if ($owner.ReturnValue -ne 0 -or $owner.User -cne $WindowsUser -or -not $owner.Domain) { throw 'interactive session owner mismatch' }
$processes = @(Get-CimInstance Win32_Process -Filter "Name = 'OSL Privacy.exe'" | Where-Object {
  [int]$_.SessionId -eq $SessionId -and $_.ExecutablePath -and
  [string]::Equals([IO.Path]::GetFullPath([string]$_.ExecutablePath), $path, [StringComparison]::OrdinalIgnoreCase) -and
  ([string]$_.CommandLine -notmatch '(?:^|\s)--osl-signal-window-guardian-v1(?:\s|$)')
})
$signalPids = @(Get-CimInstance Win32_Process -Filter "Name = 'Signal.exe'" | Where-Object {
  [int]$_.SessionId -eq $SessionId -and $_.ExecutablePath -and
  [string]::Equals([IO.Path]::GetFullPath([string]$_.ExecutablePath), $signalPath, [StringComparison]::OrdinalIgnoreCase)
} | ForEach-Object { [int]$_.ProcessId } | Sort-Object -Unique)
if ($processes.Count -ne 1) { throw 'exact OSL process is unavailable or ambiguous' }
if ($signalPids.Count -lt 1) { throw 'exact Signal process set is empty' }
[pscustomobject]@{
  Status = 'audited'
  Sha256 = $OslExeSha256.ToLowerInvariant()
  WebView2LoaderSha256 = $WebView2LoaderSha256.ToLowerInvariant()
  ExactProcessCount = $processes.Count
  ExactSignalProcessCount = $signalPids.Count
  SignalSha256 = $SignalExeSha256.ToLowerInvariant()
  SignalPublisherVerified = $true
  SessionOwnerVerified = $true
  ProfileRootVerified = $true
} | ConvertTo-Json -Compress
