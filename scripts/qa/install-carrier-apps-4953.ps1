<#
Installs the task 4953 carrier desktop apps from pinned installer bytes.

Run this through scripts/qa/osl-vm-desktop-runner.ps1. The runner is the gate
4951 mechanism that executes in the logged-on osladmin desktop session.
#>
param(
  [ValidateSet('Discord', 'Telegram Desktop', 'Signal Desktop', 'WhatsApp Desktop', 'Chrome', 'Firefox')]
  [string[]]$Apps = @('Discord', 'Telegram Desktop', 'Signal Desktop', 'WhatsApp Desktop', 'Chrome', 'Firefox'),

  [string]$InstallRoot = 'C:\OSL\carrier-installers-4953',

  [string]$PinnedListPath = '',

  [switch]$FreshDownload
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$installerRoot = Join-Path $InstallRoot 'installers'
[void](New-Item -ItemType Directory -Path $installerRoot -Force)

if (-not $PinnedListPath) {
  $PinnedListPath = Join-Path $PSScriptRoot 'carrier-app-installers-4953.json'
}

function Get-LowerSha256([string]$Path) {
  return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
}

function Read-PinnedCarrierSpecs([string]$Path) {
  if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
    throw "pinned installer list not found: $Path"
  }
  $decoded = Get-Content -LiteralPath $Path -Raw | ConvertFrom-Json -ErrorAction Stop
  $items = @($decoded)
  if ($items.Count -eq 1 -and $items[0] -is [Array]) {
    $items = @($items[0])
  }
  if ($items.Count -ne 6) {
    throw "pinned installer list must contain exactly 6 apps"
  }
  $seen = @{}
  foreach ($item in $items) {
    foreach ($field in @('Name', 'Version', 'Url', 'Sha256', 'FileName', 'Kind', 'Arguments')) {
      if (-not $item.PSObject.Properties[$field] -or [string]::IsNullOrWhiteSpace([string]$item.$field)) {
        throw "pinned installer list entry is missing $field"
      }
    }
    if ($item.Sha256 -cnotmatch '^[0-9a-f]{64}$') {
      throw "pinned installer SHA-256 for $($item.Name) is not lowercase hex"
    }
    if ($item.Kind -cnotin @('exe', 'msi', 'store-stub')) {
      throw "pinned installer kind for $($item.Name) is unsupported"
    }
    if ($seen.ContainsKey([string]$item.Name)) {
      throw "duplicate pinned installer app $($item.Name)"
    }
    $seen[[string]$item.Name] = $true
  }
  return $items
}

$carrierSpecs = Read-PinnedCarrierSpecs $PinnedListPath
$script:InstallCount = 0

function Assert-ExpectedSha256([string]$Path, [string]$ExpectedSha256, [string]$AppName) {
  $actual = Get-LowerSha256 $Path
  if ($actual -cne $ExpectedSha256.ToLowerInvariant()) {
    Write-Output "VM-4953-HASH-MISMATCH $AppName"
    Write-Output "VM-4953-HASH-MISMATCH-DETAIL expected=$($ExpectedSha256.ToLowerInvariant()) actual=$actual"
    Write-Output "VM-4953-INSTALL-COUNT $script:InstallCount"
    exit 53
  }
  Write-Output "VM-4953-HASH-OK $AppName $actual"
}

function Save-Download([string]$Url, [string]$Destination) {
  $temporary = "$Destination.download"
  if (Test-Path -LiteralPath $temporary) {
    Remove-Item -LiteralPath $temporary -Force
  }
  Invoke-WebRequest -Uri $Url -OutFile $temporary -UseBasicParsing
  Move-Item -LiteralPath $temporary -Destination $Destination -Force
}

function Get-HighestVersionPath([string]$GlobPath, [string]$ExeName) {
  $dirs = @(Get-ChildItem -Path $GlobPath -Directory -ErrorAction SilentlyContinue | Sort-Object Name -Descending)
  foreach ($dir in $dirs) {
    $candidate = Join-Path $dir.FullName $ExeName
    if (Test-Path -LiteralPath $candidate -PathType Leaf) {
      return $candidate
    }
  }
  return $null
}

function Get-FileProductVersion([string]$Path) {
  if ([string]::IsNullOrWhiteSpace($Path) -or -not (Test-Path -LiteralPath $Path -PathType Leaf)) {
    return $null
  }
  $version = (Get-Item -LiteralPath $Path).VersionInfo.ProductVersion
  if (-not $version) {
    $version = (Get-Item -LiteralPath $Path).VersionInfo.FileVersion
  }
  if (-not $version) {
    $version = 'unknown'
  }
  return [string]$version
}

function Resolve-CarrierApp([string]$Name) {
  switch ($Name) {
    'Discord' {
      $path = Get-HighestVersionPath (Join-Path $env:LOCALAPPDATA 'Discord\app-*') 'Discord.exe'
      $version = if ($path -and $path -match '\\app-([^\\]+)\\Discord[.]exe$') { $Matches[1] } else { Get-FileProductVersion $path }
      return [pscustomobject]@{ Name = $Name; Version = $version; Path = $path; Source = 'LocalAppDataDiscord' }
    }
    'Telegram Desktop' {
      $candidates = @(
        (Join-Path $env:APPDATA 'Telegram Desktop\Telegram.exe'),
        (Join-Path $env:LOCALAPPDATA 'Programs\Telegram Desktop\Telegram.exe')
      )
      $path = $candidates | Where-Object { Test-Path -LiteralPath $_ -PathType Leaf } | Select-Object -First 1
      return [pscustomobject]@{ Name = $Name; Version = (Get-FileProductVersion $path); Path = $path; Source = 'TelegramExe' }
    }
    'Signal Desktop' {
      $path = Join-Path $env:LOCALAPPDATA 'Programs\signal-desktop\Signal.exe'
      return [pscustomobject]@{ Name = $Name; Version = (Get-FileProductVersion $path); Path = $path; Source = 'LocalAppDataProgramsSignalDesktop' }
    }
    'WhatsApp Desktop' {
      $pkg = Get-AppxPackage -Name '5319275A.WhatsAppDesktop' -ErrorAction SilentlyContinue
      $path = if ($pkg) { Join-Path $pkg.InstallLocation 'WhatsApp.Root.exe' } else { $null }
      $version = if ($pkg) { [string]$pkg.Version } else { $null }
      return [pscustomobject]@{ Name = $Name; Version = $version; Path = $path; Source = 'GetAppxPackage5319275A.WhatsAppDesktop' }
    }
    'Chrome' {
      $path = 'C:\Program Files\Google\Chrome\Application\chrome.exe'
      return [pscustomobject]@{ Name = $Name; Version = (Get-FileProductVersion $path); Path = $path; Source = 'ProgramFilesGoogleChrome' }
    }
    'Firefox' {
      $path = 'C:\Program Files\Mozilla Firefox\firefox.exe'
      return [pscustomobject]@{ Name = $Name; Version = (Get-FileProductVersion $path); Path = $path; Source = 'ProgramFilesMozillaFirefox' }
    }
    default {
      throw "unknown carrier app $Name"
    }
  }
}

function Test-VersionSatisfied($Spec, $Installed) {
  if (-not $Installed -or -not $Installed.Version) {
    return $false
  }
  if ($Spec.Name -eq 'WhatsApp Desktop') {
    return $true
  }
  return ([string]$Installed.Version).StartsWith([string]$Spec.Version, [StringComparison]::OrdinalIgnoreCase)
}

function Stop-CarrierProcesses([string]$Name) {
  $processNames = switch ($Name) {
    'Discord' { @('Discord', 'Update') }
    'Telegram Desktop' { @('Telegram') }
    'Signal Desktop' { @('Signal') }
    'WhatsApp Desktop' { @('WhatsApp', 'WhatsApp.Root') }
    'Chrome' { @('chrome') }
    'Firefox' { @('firefox') }
  }
  foreach ($processName in $processNames) {
    Get-Process -Name $processName -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
  }
}

function Invoke-Installer($Spec, [string]$InstallerPath) {
  if ($Spec.Kind -eq 'msi') {
    $arguments = '/i "{0}" {1}' -f $InstallerPath, $Spec.Arguments
    $process = Start-Process -FilePath 'msiexec.exe' -ArgumentList $arguments -Wait -PassThru
  } elseif ($Spec.Kind -eq 'store-stub') {
    $process = Start-Process -FilePath $InstallerPath -ArgumentList $Spec.Arguments -Wait -PassThru
  } else {
    $process = Start-Process -FilePath $InstallerPath -ArgumentList $Spec.Arguments -Wait -PassThru
  }
  $exitCode = if ($null -eq $process.ExitCode) { 0 } else { [int]$process.ExitCode }
  Write-Output "VM-4953-INSTALLER-EXIT $($Spec.Name) exit=$exitCode"
  if ($exitCode -ne 0) {
    if ($Spec.Kind -eq 'store-stub') {
      return
    }
    throw "installer for $($Spec.Name) exited $exitCode"
  }
}

function Wait-CarrierVersion($Spec, [int]$WaitSeconds) {
  $deadline = [DateTime]::UtcNow.AddSeconds($WaitSeconds)
  do {
    $installed = Resolve-CarrierApp $Spec.Name
    if (Test-VersionSatisfied $Spec $installed) {
      return $installed
    }
    Start-Sleep -Seconds 2
  } while ([DateTime]::UtcNow -lt $deadline)
  return Resolve-CarrierApp $Spec.Name
}

$selected = foreach ($app in $Apps) {
  $spec = $carrierSpecs | Where-Object { $_.Name -ceq $app } | Select-Object -First 1
  if (-not $spec) {
    throw "unknown requested carrier app $app"
  }
  $spec
}

foreach ($spec in $selected) {
  $installerPath = Join-Path $installerRoot $spec.FileName
  if ($FreshDownload -and (Test-Path -LiteralPath $installerPath)) {
    Remove-Item -LiteralPath $installerPath -Force
  }
  if (-not (Test-Path -LiteralPath $installerPath -PathType Leaf)) {
    Save-Download $spec.Url $installerPath
  }
  Assert-ExpectedSha256 $installerPath $spec.Sha256 $spec.Name

  $before = Resolve-CarrierApp $spec.Name
  if (Test-VersionSatisfied $spec $before) {
    Write-Output "VM-4953-ALREADY-INSTALLED $($spec.Name) version=$($before.Version) path=$($before.Path)"
    continue
  }

  Stop-CarrierProcesses $spec.Name
  Invoke-Installer $spec $installerPath
  $script:InstallCount += 1
  $after = Wait-CarrierVersion $spec 240
  if ($spec.Kind -eq 'store-stub' -and -not (Test-VersionSatisfied $spec $after)) {
    Write-Output 'VM-4953-WHATSAPP-STUB-NOT-INSTALLED fallback=winget-msstore'
    $wingetOutput = (& winget install --id 9NKSQGP7F2NH --source msstore --accept-source-agreements --accept-package-agreements --silent --disable-interactivity) 2>&1
    $wingetExit = [int]$LASTEXITCODE
    $wingetOutput | ForEach-Object { Write-Output "VM-4953-WINGET-MSSTORE $_" }
    if ($wingetExit -ne 0) {
      throw "winget msstore WhatsApp install exited $wingetExit"
    }
    $after = Wait-CarrierVersion $spec 240
  }
  if (-not (Test-VersionSatisfied $spec $after)) {
    throw "installed version for $($spec.Name) was absent or wrong after installer"
  }
  Write-Output "VM-4953-INSTALL-FINISHED $($spec.Name) version=$($after.Version) path=$($after.Path)"
}

Write-Output "VM-4953-INSTALL-COUNT $script:InstallCount"

$inventory = foreach ($spec in $carrierSpecs) {
  $installed = Resolve-CarrierApp $spec.Name
  if (-not $installed.Version) {
    throw "inventory missing version for $($spec.Name)"
  }
  Write-Output "VM-4953-INVENTORY $($spec.Name) version=$($installed.Version) path=$($installed.Path)"
  [pscustomobject]@{ App = $spec.Name; Version = $installed.Version; Path = $installed.Path; Source = $installed.Source }
}

$installedCount = @($inventory).Count
Write-Output "VM-4953-INVENTORY-COUNT $installedCount"
if ($installedCount -ne 6) {
  throw "expected 6 carrier apps, got $installedCount"
}
