<#
Installs Microsoft Desktop App Installer for task 4952.

This script is intended to run through scripts/qa/osl-vm-desktop-runner.ps1 so
the package registration commands affect the logged-on osladmin account, not
Azure run-command's SYSTEM account.
#>
param(
  [ValidateSet('Good', 'WrongFirstHash')]
  [string]$HashMode = 'Good',

  [string]$InstallRoot = 'C:\OSL\app-installer-4952',

  [switch]$FreshDownload
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$packagesRoot = Join-Path $InstallRoot 'packages'
$extractRoot = Join-Path $InstallRoot 'extract'
$xamlExtractRoot = Join-Path $extractRoot 'xaml'
$xamlAppxRelativePath = 'tools\AppX\x64\Release\Microsoft.UI.Xaml.2.7.appx'

$packages = @(
  [pscustomobject]@{
    Name = 'Microsoft.VCLibs.x64.14.00.Desktop.appx'
    Url = 'https://aka.ms/Microsoft.VCLibs.x64.14.00.Desktop.appx'
    Sha256 = 'b56a9101f706f9d95f815f5b7fa6efbac972e86573d378b96a07cff5540c5961'
    Kind = 'dependency-appx'
  },
  [pscustomobject]@{
    Name = 'microsoft.ui.xaml.2.7.3.nupkg'
    Url = 'https://api.nuget.org/v3-flatcontainer/microsoft.ui.xaml/2.7.3/microsoft.ui.xaml.2.7.3.nupkg'
    Sha256 = '9ef3c54aa8c185603ba87d61673efb527062054adc736e27f2c4a033b5f797a8'
    Kind = 'xaml-nupkg'
    ExtractedRelativePath = $xamlAppxRelativePath
    ExtractedSha256 = '8ce30d92abec6522beb2544e7b716983f5cba50751b580d89a36048bf4d90316'
  },
  [pscustomobject]@{
    Name = 'Microsoft.DesktopAppInstaller_8wekyb3d8bbwe_v1.6.3482.msixbundle'
    Url = 'https://github.com/microsoft/winget-cli/releases/download/v1.6.3482/Microsoft.DesktopAppInstaller_8wekyb3d8bbwe.msixbundle'
    Sha256 = 'c98116463600bf102119938d7a26d2a16a89af8aa7415e8da4701d49d1b4ff1d'
    Kind = 'app-installer-bundle'
  },
  [pscustomobject]@{
    Name = '24146eb205d040e69ef2d92d7034d97f_License1.xml'
    Url = 'https://github.com/microsoft/winget-cli/releases/download/v1.6.3482/24146eb205d040e69ef2d92d7034d97f_License1.xml'
    Sha256 = '61361fcd87aa5744472523ef41b53b1e9b00d579926f4c585bf2e9c98a998abd'
    Kind = 'app-installer-license'
  }
)

if ($HashMode -eq 'WrongFirstHash') {
  $packages[0].Sha256 = '0000000000000000000000000000000000000000000000000000000000000000'
}

function Get-LowerSha256([string]$Path) {
  return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
}

function Assert-ExpectedSha256([string]$Path, [string]$ExpectedSha256, [string]$Label) {
  $actual = Get-LowerSha256 $Path
  if ($actual -cne $ExpectedSha256.ToLowerInvariant()) {
    Write-Output 'VM-4952-HASH-MISMATCH'
    exit 52
  }
  Write-Output "VM-4952-HASH-OK $Label $actual"
}

function Save-Download([string]$Url, [string]$Destination) {
  $temporary = "$Destination.download"
  if (Test-Path -LiteralPath $temporary) {
    Remove-Item -LiteralPath $temporary -Force
  }
  Invoke-WebRequest -Uri $Url -OutFile $temporary -UseBasicParsing
  Move-Item -LiteralPath $temporary -Destination $Destination -Force
}

if ($FreshDownload -and (Test-Path -LiteralPath $packagesRoot)) {
  Remove-Item -LiteralPath $packagesRoot -Recurse -Force
}
if (Test-Path -LiteralPath $extractRoot) {
  Remove-Item -LiteralPath $extractRoot -Recurse -Force
}
[void](New-Item -ItemType Directory -Path $packagesRoot -Force)

$downloaded = @{}
foreach ($package in $packages) {
  $destination = Join-Path $packagesRoot $package.Name
  Save-Download $package.Url $destination
  Assert-ExpectedSha256 $destination $package.Sha256 $package.Name
  $downloaded[$package.Name] = $destination
}

Add-Type -AssemblyName System.IO.Compression.FileSystem
$xamlPackage = $packages | Where-Object { $_.Kind -eq 'xaml-nupkg' } | Select-Object -First 1
$xamlNupkgPath = $downloaded[$xamlPackage.Name]
[IO.Compression.ZipFile]::ExtractToDirectory($xamlNupkgPath, $xamlExtractRoot)
$xamlAppxPath = Join-Path $xamlExtractRoot $xamlPackage.ExtractedRelativePath
if (-not (Test-Path -LiteralPath $xamlAppxPath -PathType Leaf)) {
  throw 'Microsoft.UI.Xaml appx was missing from the verified NuGet package'
}
Assert-ExpectedSha256 $xamlAppxPath $xamlPackage.ExtractedSha256 'Microsoft.UI.Xaml.2.7.appx'

$vclibsPath = $downloaded['Microsoft.VCLibs.x64.14.00.Desktop.appx']
$appInstallerPath = $downloaded['Microsoft.DesktopAppInstaller_8wekyb3d8bbwe_v1.6.3482.msixbundle']
$licensePath = $downloaded['24146eb205d040e69ef2d92d7034d97f_License1.xml']

Add-AppxPackage -Path $vclibsPath -ForceApplicationShutdown
Write-Output 'VM-4952-INSTALLED Microsoft.VCLibs.x64.14.00.Desktop.appx'
Add-AppxPackage -Path $xamlAppxPath -ForceApplicationShutdown
Write-Output 'VM-4952-INSTALLED Microsoft.UI.Xaml.2.7.appx'
Add-AppxPackage -Path $appInstallerPath -ForceApplicationShutdown
Write-Output 'VM-4952-INSTALLED Microsoft.DesktopAppInstaller'
Add-AppxProvisionedPackage -Online -PackagePath $appInstallerPath -LicensePath $licensePath | Out-Null
Write-Output 'VM-4952-PROVISIONED Microsoft.DesktopAppInstaller License1.xml'

$versionOutput = (& winget --version) 2>&1
if ($LASTEXITCODE -ne 0) {
  throw "winget version check failed: $versionOutput"
}
Write-Output "VM-4952-WINGET-VERSION $versionOutput"
