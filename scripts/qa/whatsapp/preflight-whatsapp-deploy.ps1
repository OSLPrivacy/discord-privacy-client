$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$oslPath = [IO.Path]::GetFullPath('C:\Users\osltest\Desktop\OSL Privacy\OSL Privacy.exe')
$loaderPath = Join-Path ([IO.Path]::GetDirectoryName($oslPath)) 'WebView2Loader.dll'
$exePresent = Test-Path -LiteralPath $oslPath -PathType Leaf
$loaderPresent = Test-Path -LiteralPath $loaderPath -PathType Leaf
$processes = @()
if ($exePresent) {
  $processes = @(Get-CimInstance Win32_Process -Filter "Name = 'OSL Privacy.exe'" | Where-Object {
    $_.ExecutablePath -and [string]::Equals(
      [IO.Path]::GetFullPath([string]$_.ExecutablePath), $oslPath, [StringComparison]::OrdinalIgnoreCase
    )
  })
}
$whatsApp = @(Get-CimInstance Win32_Process -Filter "Name = 'WhatsApp.Root.exe'")
[pscustomobject]@{
  Schema = 'whatsapp-deploy-preflight/v1'
  InstallRootPresent = Test-Path -LiteralPath ([IO.Path]::GetDirectoryName($oslPath)) -PathType Container
  ExePresent = $exePresent
  LoaderPresent = $loaderPresent
  ExeSha256 = if ($exePresent) { (Get-FileHash -LiteralPath $oslPath -Algorithm SHA256).Hash.ToLowerInvariant() } else { $null }
  LoaderSha256 = if ($loaderPresent) { (Get-FileHash -LiteralPath $loaderPath -Algorithm SHA256).Hash.ToLowerInvariant() } else { $null }
  OslProcessCount = $processes.Count
  WhatsAppRootProcessCount = $whatsApp.Count
  ProfileRead = $false
  WhatsAppPrivateStorageRead = $false
  ProcessesTerminated = 0
} | ConvertTo-Json -Compress
