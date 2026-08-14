param(
  [Parameter(Mandatory = $true)]
  [ValidatePattern('^[0-9a-f]{64}$')]
  [string]$ExpectedSha256,

  [string]$InstallerUrl = 'https://github.com/OSLPrivacy/discord-privacy-client/releases/download/hub-v0.1.0/osl-hub-0.1.0-x64-nsis.exe',

  [string]$ReceiptPath = 'C:\ProgramData\OSL-QA\task-1607-install-receipt.json',

  # A short-lived write-only blob SAS is optional.  Run Command does not
  # reliably return stdout on every Azure image, so this transports only the
  # completed, non-secret receipt to the operator for independent verification.
  [string]$ReceiptUploadUri = ''
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$installerName = 'osl-hub-0.1.0-x64-nsis.exe'
$installerSize = 7126171
$installerPath = Join-Path ([IO.Path]::GetDirectoryName($ReceiptPath)) $installerName

trap {
  # Preserve a non-secret terminal failure receipt as well.  The local verifier
  # rejects this shape; it exists solely so unattended VM failures are visible.
  $failure = [ordered]@{
    schemaVersion = 1
    capturedUtc = (Get-Date).ToUniversalTime().ToString('o')
    failure = $_.Exception.Message
  }
  [void](New-Item -ItemType Directory -Path ([IO.Path]::GetDirectoryName($ReceiptPath)) -Force)
  [IO.File]::WriteAllText(
    $ReceiptPath,
    ($failure | ConvertTo-Json -Depth 4),
    [Text.UTF8Encoding]::new($false)
  )
  if (-not [string]::IsNullOrWhiteSpace($ReceiptUploadUri)) {
    try {
      Invoke-WebRequest -Uri $ReceiptUploadUri -Method Put -InFile $ReceiptPath -UseBasicParsing -Headers @{ 'x-ms-blob-type' = 'BlockBlob'; 'x-ms-version' = '2023-11-03' } | Out-Null
    } catch { }
  }
  throw $_
}

function Get-Sha256([string]$Path) {
  return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
}

function Get-OslInstalledApps {
  $roots = @(
    'HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*',
    'HKLM:\Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\*',
    'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*'
  )
  $apps = @(
    foreach ($root in $roots) {
      foreach ($entry in @(Get-ItemProperty -Path $root -ErrorAction SilentlyContinue)) {
        # Most uninstall subkeys are not application records.  Under StrictMode
        # their absent DisplayName property must mean "not OSL", not a failed
        # install check.
        $displayNameProperty = $entry.PSObject.Properties['DisplayName']
        if ($null -eq $displayNameProperty -or [string]$displayNameProperty.Value -cne 'OSL Privacy') { continue }
        $displayVersionProperty = $entry.PSObject.Properties['DisplayVersion']
        $installLocationProperty = $entry.PSObject.Properties['InstallLocation']
        $displayIconProperty = $entry.PSObject.Properties['DisplayIcon']
        [ordered]@{
          displayName = [string]$displayNameProperty.Value
          displayVersion = if ($null -eq $displayVersionProperty) { '' } else { [string]$displayVersionProperty.Value }
          installLocation = if ($null -eq $installLocationProperty) { '' } else { [string]$installLocationProperty.Value }
          displayIcon = if ($null -eq $displayIconProperty) { '' } else { [string]$displayIconProperty.Value }
          registryPath = [string]$entry.PSPath
        }
      }
    }
  )
  return $apps
}

function Resolve-OslExecutable($Apps) {
  foreach ($app in $Apps) {
    # The NSIS uninstall key quotes both DisplayIcon and InstallLocation.  Do
    # not pass quoted InstallLocation to Join-Path: it interprets `"C:` as a
    # distinct PowerShell drive before the DisplayIcon normalisation can run.
    $installLocation = ([string]$app.installLocation).Trim().Trim('"')
    $candidates = @([string]$app.displayIcon)
    if (-not [string]::IsNullOrWhiteSpace($installLocation)) {
      $candidates += (Join-Path $installLocation 'OSL Privacy.exe')
    }
    foreach ($candidate in $candidates) {
      if ([string]::IsNullOrWhiteSpace($candidate)) { continue }
      # NSIS records DisplayIcon as a quoted path followed by `,0`.  Remove
      # the icon index before unquoting, or Test-Path treats the leading quote
      # as part of the drive name (`"C:`).
      $path = ($candidate -replace ',[0-9]+$', '').Trim().Trim('"')
      if (Test-Path -LiteralPath $path -PathType Leaf) { return [IO.Path]::GetFullPath($path) }
    }
  }
  $found = @(Get-ChildItem -LiteralPath 'C:\Users' -Filter 'OSL Privacy.exe' -File -Recurse -ErrorAction SilentlyContinue |
    Where-Object { $_.FullName -match '\\AppData\\Local\\OSL Privacy\\OSL Privacy\.exe$' })
  if ($found.Count -ne 1) { throw "expected exactly one installed OSL Privacy.exe, found $($found.Count)" }
  return $found[0].FullName
}

function Get-WelcomeText([int]$ProcessId) {
  Add-Type -AssemblyName UIAutomationClient
  $deadline = (Get-Date).AddSeconds(45)
  while ((Get-Date) -lt $deadline) {
    $process = Get-Process -Id $ProcessId -ErrorAction SilentlyContinue
    if ($null -eq $process) { throw 'OSL process exited before its first-run screen was observed' }
    $handle = [IntPtr]$process.MainWindowHandle
    if ($handle -ne [IntPtr]::Zero) {
      try {
        $window = [System.Windows.Automation.AutomationElement]::FromHandle($handle)
        $condition = [System.Windows.Automation.PropertyCondition]::new(
          [System.Windows.Automation.AutomationElement]::NameProperty, 'Welcome'
        )
        $welcome = $window.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $condition)
        if ($null -ne $welcome) { return 'Welcome' }
      } catch {
        # The WebView tree can still be loading.  Keep the bounded observation alive.
      }
    }
    Start-Sleep -Milliseconds 500
  }
  throw 'OSL did not expose Welcome through UI Automation within 45 seconds'
}

[void](New-Item -ItemType Directory -Path ([IO.Path]::GetDirectoryName($ReceiptPath)) -Force)
if (Test-Path -LiteralPath $installerPath) { Remove-Item -LiteralPath $installerPath -Force }
Invoke-WebRequest -Uri $InstallerUrl -OutFile $installerPath -UseBasicParsing
if ((Get-Item -LiteralPath $installerPath).Length -ne $installerSize) {
  throw 'downloaded installer size does not match the released asset'
}
$actualSha256 = Get-Sha256 $installerPath
if ($actualSha256 -cne $ExpectedSha256) { throw 'downloaded installer SHA-256 does not match the published checksum' }

$installedApps = @(Get-OslInstalledApps)
if ($installedApps.Count -eq 0) {
  $install = Start-Process -FilePath $installerPath -ArgumentList '/S' -Wait -PassThru
  if ($install.ExitCode -ne 0) { throw "installer exited $($install.ExitCode)" }
  $installedApps = @(Get-OslInstalledApps)
}
if ($installedApps.Count -lt 1) { throw 'OSL Privacy is absent from Installed Apps after installer completion' }
$exePath = Resolve-OslExecutable $installedApps
$launch = Start-Process -FilePath $exePath -WorkingDirectory ([IO.Path]::GetDirectoryName($exePath)) -PassThru
$welcomeText = Get-WelcomeText $launch.Id

$receipt = [ordered]@{
  schemaVersion = 1
  capturedUtc = (Get-Date).ToUniversalTime().ToString('o')
  installerSha256 = $actualSha256
  installer = [ordered]@{ name = $installerName; sizeBytes = $installerSize; url = $InstallerUrl }
  installedApps = @($installedApps)
  welcomeLaunch = [ordered]@{ processId = [int]$launch.Id; processAlive = [bool](Get-Process -Id $launch.Id -ErrorAction SilentlyContinue); visibleText = $welcomeText; executable = $exePath }
}
[IO.File]::WriteAllText(
  $ReceiptPath,
  ($receipt | ConvertTo-Json -Depth 6),
  [Text.UTF8Encoding]::new($false)
)
if (-not [string]::IsNullOrWhiteSpace($ReceiptUploadUri)) {
  Invoke-WebRequest -Uri $ReceiptUploadUri -Method Put -InFile $ReceiptPath -UseBasicParsing -Headers @{ 'x-ms-blob-type' = 'BlockBlob'; 'x-ms-version' = '2023-11-03' } | Out-Null
}
$receipt | ConvertTo-Json -Depth 6 -Compress
