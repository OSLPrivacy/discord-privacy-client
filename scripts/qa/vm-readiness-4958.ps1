<#
TASK 4958 - the readiness check that is allowed to say not ready.

Reports, for one named machine:
  * which of the six carrier apps are installed and at what version
  * which are signed in and as which account handle
  * whether the machine is quiet
  * whether OSL itself opens a window

Evidence is the accessibility tree only. This script NEVER saves a picture:
OSL blanks its own window against capture, so a black picture is worse than
none. There is no screen-capture, bitmap, or image-encoder call anywhere in
this file, and the probe asserts at runtime that it created 0 image files.

Run the probe through scripts/qa/osl-vm-desktop-runner.ps1 so it executes in
the logged-on interactive desktop session (task 4951).

Actions:
  Probe   - collect evidence on a Windows desktop session, write it as JSON.
  Verdict - read an evidence JSON document, print the report, set the exit code.
  Full    - Probe then Verdict in one run (the normal on-VM invocation).

Exit codes:
  0  VM-4958-READY
  1  VM-4958-NOT-READY (any named app missing, signed out, or showing no
     window inside WaitSeconds; machine not quiet; OSL window absent)
#>
param(
  [ValidatePattern('^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$')]
  [string]$Machine = $env:COMPUTERNAME,

  [ValidateSet('Probe', 'Verdict', 'Full')]
  [string]$Action = 'Full',

  [ValidateSet('Discord', 'Telegram Desktop', 'Signal Desktop', 'WhatsApp Desktop', 'Chrome', 'Firefox')]
  [string[]]$Apps = @('Discord', 'Telegram Desktop', 'Signal Desktop', 'WhatsApp Desktop', 'Chrome', 'Firefox'),

  [ValidateRange(5, 60)]
  [int]$WaitSeconds = 60,

  [string]$EvidenceJsonPath = '',

  [string]$OutputRoot = ''
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$ImageExtensions = @('.png', '.jpg', '.jpeg', '.bmp', '.gif', '.tif', '.tiff', '.webp', '.ico', '.emf', '.wmf')
$EvidenceSchema = 'osl-vm-readiness-4958/1'
$EvidenceSource = 'accessibility-tree'

function Get-Field($Object, [string]$Name, $Default) {
  if ($null -eq $Object) { return $Default }
  $property = $Object.PSObject.Properties[$Name]
  if (-not $property) { return $Default }
  if ($null -eq $property.Value) { return $Default }
  return $property.Value
}

function ConvertTo-Flag($Value) {
  if ($Value) { return 'true' }
  return 'false'
}

function Format-Seconds($Value) {
  if ($null -eq $Value) { return '0' }
  return ([Math]::Round([double]$Value, 3)).ToString([Globalization.CultureInfo]::InvariantCulture)
}

function Get-ImageFileCount([string]$Root) {
  if (-not $Root -or -not (Test-Path -LiteralPath $Root)) { return 0 }
  return @(Get-ChildItem -LiteralPath $Root -Recurse -File -Force -ErrorAction SilentlyContinue |
    Where-Object { $ImageExtensions -contains $_.Extension.ToLowerInvariant() }).Count
}

# ---------------------------------------------------------------------------
# App table. Install probes, launch probes, and the accessibility-tree names
# that say "signed in as X" or "signed out" for each carrier app.
# ---------------------------------------------------------------------------

function Get-AppSpecs {
  @(
    [pscustomobject]@{
      Name = 'Discord'
      ProcessNames = @('Discord')
      InstallGlob = 'LOCALAPPDATA:Discord\app-*'
      InstallExe = 'Discord.exe'
      LaunchArgs = ''
      Shell = $false
      SignedInNamePatterns = @('^(?<handle>[a-z0-9._]{2,32}),\s', '^(?<handle>[a-z0-9._]{2,32})\s+#\d{4}$', 'User area,\s*(?<handle>[a-z0-9._]{2,32})')
      SignedOutNames = @('Log In', 'Login', 'Register', 'Sign In', 'Open Discord in your browser')
    },
    [pscustomobject]@{
      Name = 'Telegram Desktop'
      ProcessNames = @('Telegram')
      InstallPaths = @('APPDATA:Telegram Desktop\Telegram.exe', 'LOCALAPPDATA:Programs\Telegram Desktop\Telegram.exe')
      LaunchArgs = ''
      Shell = $false
      SignedInNamePatterns = @('^(?<handle>\+\d[\d\s]{6,20})$', '^(?<handle>@[A-Za-z0-9_]{4,32})$')
      SignedOutNames = @('Start Messaging', 'Sign in with QR code', 'Log in by phone Number', 'Your Phone Number')
    },
    [pscustomobject]@{
      Name = 'Signal Desktop'
      ProcessNames = @('Signal')
      InstallPaths = @('LOCALAPPDATA:Programs\signal-desktop\Signal.exe')
      LaunchArgs = ''
      Shell = $false
      SignedInNamePatterns = @('^(?<handle>\+\d[\d\s]{6,20})$', 'Profile,\s*(?<handle>.{2,64})$')
      SignedOutNames = @('Link your phone to Signal Desktop', 'Link this device', 'Use a phone number instead', 'Get started')
    },
    [pscustomobject]@{
      Name = 'WhatsApp Desktop'
      ProcessNames = @('WhatsApp.Root', 'WhatsApp')
      StoreAppUserModelId = 'shell:AppsFolder\5319275A.WhatsAppDesktop_cv1g1gvanyjgm!App'
      StorePackageName = '5319275A.WhatsAppDesktop'
      Shell = $true
      LaunchArgs = ''
      SignedInNamePatterns = @('^(?<handle>\+\d[\d\s]{6,20})$', 'Profile,\s*(?<handle>.{2,64})$')
      SignedOutNames = @('Log into WhatsApp', 'Steps to log in', 'Link with phone number instead', 'Scan the QR code')
    },
    [pscustomobject]@{
      Name = 'Chrome'
      ProcessNames = @('chrome')
      InstallPaths = @('PROGRAMFILES:Google\Chrome\Application\chrome.exe')
      LaunchArgs = '--new-window about:blank --no-first-run --no-default-browser-check'
      Shell = $false
      SignedInNamePatterns = @('Account Information,\s*(?<handle>\S+@\S+)', '^(?<handle>\S+@\S+\.\S+)$', 'Google Account:\s*.*\((?<handle>\S+@\S+)\)')
      SignedOutNames = @('Sign in to Chrome', 'Turn on sync', 'Sign in')
    },
    [pscustomobject]@{
      Name = 'Firefox'
      ProcessNames = @('firefox')
      InstallPaths = @('PROGRAMFILES:Mozilla Firefox\firefox.exe')
      LaunchArgs = '-new-window about:blank'
      Shell = $false
      SignedInNamePatterns = @('Account,\s*(?<handle>\S+@\S+)', '^(?<handle>\S+@\S+\.\S+)$')
      SignedOutNames = @('Sign in', 'Sign In', 'Sign up', 'Sign in to sync')
    }
  )
}

function Resolve-SpecPath([string]$Value) {
  $parts = $Value.Split(':', 2)
  if ($parts.Count -ne 2) { return $Value }
  $base = switch ($parts[0]) {
    'LOCALAPPDATA' { $env:LOCALAPPDATA }
    'APPDATA' { $env:APPDATA }
    'PROGRAMFILES' { $env:ProgramFiles }
    'PROGRAMFILESX86' { ${env:ProgramFiles(x86)} }
    default { $null }
  }
  if (-not $base) { return $null }
  return (Join-Path $base $parts[1])
}

# ---------------------------------------------------------------------------
# Probe (Windows desktop session only).
# ---------------------------------------------------------------------------

function Get-InstalledApp($Spec) {
  $exePath = $null
  if ($Spec.PSObject.Properties['InstallGlob'] -and $Spec.InstallGlob) {
    $glob = Resolve-SpecPath $Spec.InstallGlob
    if ($glob) {
      $dirs = @(Get-ChildItem -Path $glob -Directory -ErrorAction SilentlyContinue | Sort-Object Name -Descending)
      foreach ($dir in $dirs) {
        $candidate = Join-Path $dir.FullName $Spec.InstallExe
        if (Test-Path -LiteralPath $candidate -PathType Leaf) { $exePath = $candidate; break }
      }
    }
  }
  if (-not $exePath -and $Spec.PSObject.Properties['InstallPaths']) {
    foreach ($raw in $Spec.InstallPaths) {
      $candidate = Resolve-SpecPath $raw
      if ($candidate -and (Test-Path -LiteralPath $candidate -PathType Leaf)) { $exePath = $candidate; break }
    }
  }
  if ($exePath) {
    $version = (Get-Item -LiteralPath $exePath).VersionInfo.ProductVersion
    if (-not $version) { $version = (Get-Item -LiteralPath $exePath).VersionInfo.FileVersion }
    return [pscustomobject]@{ Installed = $true; Version = [string]$version; Path = $exePath }
  }
  if ($Spec.PSObject.Properties['StorePackageName'] -and $Spec.StorePackageName) {
    $package = @(Get-AppxPackage -Name $Spec.StorePackageName -ErrorAction SilentlyContinue) | Select-Object -First 1
    if ($package) {
      return [pscustomobject]@{ Installed = $true; Version = [string]$package.Version; Path = $Spec.StoreAppUserModelId }
    }
  }
  return [pscustomobject]@{ Installed = $false; Version = ''; Path = '' }
}

function Get-ProcessIdSet([string[]]$ProcessNames) {
  $processIds = @{}
  foreach ($processName in $ProcessNames) {
    Get-Process -Name $processName -ErrorAction SilentlyContinue | ForEach-Object {
      $processIds[[int]$_.Id] = $true
    }
  }
  return $processIds
}

function Get-TopLevelWindow([hashtable]$ProcessIds) {
  if ($ProcessIds.Count -eq 0) { return $null }
  $windows = [System.Windows.Automation.AutomationElement]::RootElement.FindAll(
    [System.Windows.Automation.TreeScope]::Children,
    [System.Windows.Automation.Condition]::TrueCondition
  )
  foreach ($window in $windows) {
    try {
      $current = $window.Current
      $title = [string]$current.Name
      $windowProcessId = [int]$current.ProcessId
      if ($ProcessIds.ContainsKey($windowProcessId) -and $title.Trim().Length -gt 0) {
        return [pscustomobject]@{
          Element = $window
          Title = $title
          ProcessId = $windowProcessId
          ControlType = [string]$current.ControlType.ProgrammaticName
        }
      }
    } catch {
      continue
    }
  }
  return $null
}

function Get-AccessibleNames($WindowElement, [int]$Limit = 4000) {
  $names = New-Object 'System.Collections.Generic.List[string]'
  if ($null -eq $WindowElement) { return $names }
  try {
    $descendants = $WindowElement.FindAll(
      [System.Windows.Automation.TreeScope]::Descendants,
      [System.Windows.Automation.Condition]::TrueCondition
    )
  } catch {
    return $names
  }
  $count = [Math]::Min($descendants.Count, $Limit)
  for ($index = 0; $index -lt $count; $index++) {
    try {
      $name = [string]$descendants[$index].Current.Name
      if ($name -and $name.Trim().Length -gt 0) { $names.Add($name.Trim()) }
    } catch {
      continue
    }
  }
  return $names
}

function Resolve-SignIn($Spec, $Names) {
  $signedOutMarker = ''
  foreach ($name in $Names) {
    foreach ($marker in $Spec.SignedOutNames) {
      if ($name -ceq $marker) { $signedOutMarker = $marker; break }
    }
    if ($signedOutMarker) { break }
  }
  $handle = ''
  $matchedPattern = ''
  foreach ($name in $Names) {
    foreach ($pattern in $Spec.SignedInNamePatterns) {
      $match = [regex]::Match($name, $pattern)
      if ($match.Success -and $match.Groups['handle'].Success) {
        $handle = $match.Groups['handle'].Value.Trim()
        $matchedPattern = $pattern
        break
      }
    }
    if ($handle) { break }
  }
  $signedIn = [bool]($handle -and -not $signedOutMarker)
  return [pscustomobject]@{
    SignedIn = $signedIn
    Account = $(if ($signedIn) { $handle } else { '' })
    SignedOutMarker = $signedOutMarker
    MatchedPattern = $matchedPattern
    NamesInspected = $Names.Count
  }
}

function Start-App($Spec, $Installed) {
  if ($Spec.Shell) {
    Start-Process -FilePath 'explorer.exe' -ArgumentList $Spec.StoreAppUserModelId | Out-Null
    return
  }
  if (-not $Installed.Path) { throw "no launch path for $($Spec.Name)" }
  if ($Spec.LaunchArgs) {
    Start-Process -FilePath $Installed.Path -ArgumentList $Spec.LaunchArgs | Out-Null
  } else {
    Start-Process -FilePath $Installed.Path | Out-Null
  }
}

function Wait-ForWindow($Spec, [int]$Timeout) {
  $started = [DateTime]::UtcNow
  $deadline = $started.AddSeconds($Timeout)
  $window = $null
  while ([DateTime]::UtcNow -lt $deadline -and -not $window) {
    $window = Get-TopLevelWindow (Get-ProcessIdSet $Spec.ProcessNames)
    if ($window) { break }
    Start-Sleep -Milliseconds 500
  }
  $elapsed = ([DateTime]::UtcNow - $started).TotalSeconds
  return [pscustomobject]@{ Window = $window; Seconds = $elapsed }
}

function Get-QuietState {
  function Get-PowerSetting([string]$Subgroup, [string]$Setting) {
    $lines = & powercfg /query SCHEME_CURRENT $Subgroup $Setting 2>$null
    $ac = -1
    $dc = -1
    foreach ($line in $lines) {
      if ($line -match 'Current AC Power Setting Index:\s*0x([0-9a-fA-F]+)') { $ac = [Convert]::ToInt32($Matches[1], 16) }
      if ($line -match 'Current DC Power Setting Index:\s*0x([0-9a-fA-F]+)') { $dc = [Convert]::ToInt32($Matches[1], 16) }
    }
    return [pscustomobject]@{ Ac = $ac; Dc = $dc }
  }

  function Get-RegistryValue([string]$Path, [string]$Name, $Default) {
    try {
      $item = Get-ItemProperty -LiteralPath $Path -Name $Name -ErrorAction Stop
      return $item.$Name
    } catch {
      return $Default
    }
  }

  $video = Get-PowerSetting 'SUB_VIDEO' 'VIDEOIDLE'
  $consoleLock = Get-PowerSetting 'SUB_VIDEO' 'VIDEOCONLOCK'
  $standby = Get-PowerSetting 'SUB_SLEEP' 'STANDBYIDLE'
  $hibernate = Get-PowerSetting 'SUB_SLEEP' 'HIBERNATEIDLE'

  $screenSaverActive = [string](Get-RegistryValue 'HKCU:\Control Panel\Desktop' 'ScreenSaveActive' '0')
  $notificationsEnabled = [int](Get-RegistryValue 'HKCU:\Software\Microsoft\Windows\CurrentVersion\PushNotifications' 'ToastEnabled' 0)
  $disableLockWorkstation = [int](Get-RegistryValue 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Policies\System' 'DisableLockWorkstation' 0)
  $noAutoRebootWithLoggedOnUsers = [int](Get-RegistryValue 'HKLM:\SOFTWARE\Policies\Microsoft\Windows\WindowsUpdate\AU' 'NoAutoRebootWithLoggedOnUsers' 0)

  # Accessibility-tree proof that nothing has taken the desktop: no logon or
  # lock UI window is a child of the automation root.
  $lockUiTitles = New-Object 'System.Collections.Generic.List[string]'
  try {
    $roots = [System.Windows.Automation.AutomationElement]::RootElement.FindAll(
      [System.Windows.Automation.TreeScope]::Children,
      [System.Windows.Automation.Condition]::TrueCondition
    )
    for ($index = 0; $index -lt $roots.Count; $index++) {
      try {
        $className = [string]$roots[$index].Current.ClassName
        if ($className -match 'LockScreen|Credential|LogonUI') { $lockUiTitles.Add($className) }
      } catch { continue }
    }
  } catch {
    $lockUiTitles.Add('automation-root-unreadable')
  }
  $logonUiProcesses = @(Get-Process -Name 'LogonUI' -ErrorAction SilentlyContinue).Count

  $timeoutsZero = ($video.Ac -eq 0 -and $video.Dc -eq 0 -and $consoleLock.Ac -eq 0 -and $consoleLock.Dc -eq 0 -and
    $standby.Ac -eq 0 -and $standby.Dc -eq 0 -and $hibernate.Ac -eq 0 -and $hibernate.Dc -eq 0)

  $quiet = ($timeoutsZero -and $screenSaverActive -eq '0' -and $notificationsEnabled -eq 0 -and
    $disableLockWorkstation -eq 1 -and $noAutoRebootWithLoggedOnUsers -eq 1 -and
    $lockUiTitles.Count -eq 0 -and $logonUiProcesses -eq 0)

  return [pscustomobject]@{
    quiet = $quiet
    displayAndSleepTimeoutsZero = $timeoutsZero
    videoIdle = @{ ac = $video.Ac; dc = $video.Dc }
    consoleLock = @{ ac = $consoleLock.Ac; dc = $consoleLock.Dc }
    standbyIdle = @{ ac = $standby.Ac; dc = $standby.Dc }
    hibernateIdle = @{ ac = $hibernate.Ac; dc = $hibernate.Dc }
    screenSaverOff = ($screenSaverActive -eq '0')
    notificationBannersOff = ($notificationsEnabled -eq 0)
    workstationLockDisabled = ($disableLockWorkstation -eq 1)
    updateRestartWithoutAskingOff = ($noAutoRebootWithLoggedOnUsers -eq 1)
    lockUiWindowsInAccessibilityTree = $lockUiTitles.Count
    logonUiProcesses = $logonUiProcesses
  }
}

function Get-OslWindowState([int]$Timeout) {
  $candidates = @(
    'C:\Users\osladmin\Desktop\OSL Privacy\OSL Privacy.exe',
    'C:\Users\osltest\Desktop\OSL Privacy\OSL Privacy.exe',
    'C:\Program Files\OSL Privacy\OSL Privacy.exe',
    (Join-Path $env:LOCALAPPDATA 'Programs\OSL Privacy\OSL Privacy.exe')
  )
  $exePath = $candidates | Where-Object { $_ -and (Test-Path -LiteralPath $_ -PathType Leaf) } | Select-Object -First 1
  $spec = [pscustomobject]@{ Name = 'OSL Privacy'; ProcessNames = @('OSL Privacy'); Shell = $false; LaunchArgs = '' }
  if (-not $exePath) {
    return [pscustomobject]@{
      exePath = ''
      installed = $false
      windowFound = $false
      windowTitle = ''
      windowControlType = ''
      windowSeconds = 0
    }
  }
  if ((Get-ProcessIdSet $spec.ProcessNames).Count -eq 0) {
    Start-Process -FilePath $exePath | Out-Null
  }
  $waited = Wait-ForWindow $spec $Timeout
  return [pscustomobject]@{
    exePath = $exePath
    installed = $true
    windowFound = [bool]$waited.Window
    windowTitle = $(if ($waited.Window) { $waited.Window.Title } else { '' })
    windowControlType = $(if ($waited.Window) { $waited.Window.ControlType } else { '' })
    windowSeconds = $waited.Seconds
  }
}

function Invoke-Probe([string]$MachineName, [string[]]$AppNames, [int]$Timeout, [string]$Root) {
  Add-Type -AssemblyName UIAutomationClient
  Add-Type -AssemblyName UIAutomationTypes

  $imageFilesBefore = Get-ImageFileCount $Root
  $specs = Get-AppSpecs
  $appReports = New-Object 'System.Collections.Generic.List[object]'

  foreach ($appName in $AppNames) {
    $spec = $specs | Where-Object { $_.Name -ceq $appName } | Select-Object -First 1
    if (-not $spec) { throw "unknown app $appName" }
    $installed = Get-InstalledApp $spec
    $windowFound = $false
    $windowTitle = ''
    $windowControlType = ''
    $windowSeconds = 0
    $signIn = [pscustomobject]@{ SignedIn = $false; Account = ''; SignedOutMarker = ''; MatchedPattern = ''; NamesInspected = 0 }

    if ($installed.Installed) {
      if ((Get-ProcessIdSet $spec.ProcessNames).Count -eq 0) {
        Start-App $spec $installed
      }
      $waited = Wait-ForWindow $spec $Timeout
      $windowSeconds = $waited.Seconds
      if ($waited.Window) {
        $windowFound = $true
        $windowTitle = $waited.Window.Title
        $windowControlType = $waited.Window.ControlType
        $names = Get-AccessibleNames $waited.Window.Element
        $signIn = Resolve-SignIn $spec $names
      }
    }

    $appReports.Add([ordered]@{
      name = $spec.Name
      installed = $installed.Installed
      version = $installed.Version
      windowFound = $windowFound
      windowTitle = $windowTitle
      windowControlType = $windowControlType
      windowSeconds = [Math]::Round($windowSeconds, 3)
      signedIn = $signIn.SignedIn
      account = $signIn.Account
      signedOutMarker = $signIn.SignedOutMarker
      accountNamePattern = $signIn.MatchedPattern
      accessibilityNamesInspected = $signIn.NamesInspected
    })
  }

  $quiet = Get-QuietState
  $osl = Get-OslWindowState $Timeout
  $imageFilesAfter = Get-ImageFileCount $Root
  $imagesWritten = $imageFilesAfter - $imageFilesBefore
  if ($imagesWritten -ne 0) { throw "readiness probe wrote $imagesWritten image files; it must write 0" }

  return [ordered]@{
    schema = $EvidenceSchema
    machine = $MachineName
    collectedUtc = [DateTime]::UtcNow.ToString('o')
    evidenceSource = $EvidenceSource
    source = 'probe'
    waitSeconds = $Timeout
    imageFilesWritten = $imagesWritten
    apps = $appReports.ToArray()
    quiet = $quiet
    osl = $osl
  }
}

# ---------------------------------------------------------------------------
# Verdict (runs anywhere, on collected or replayed evidence).
# ---------------------------------------------------------------------------

function Invoke-Verdict($Evidence, [string]$MachineOverride) {
  $schema = [string](Get-Field $Evidence 'schema' '')
  if ($schema -cne $EvidenceSchema) { throw "readiness evidence schema mismatch: $schema" }
  $evidenceSource = [string](Get-Field $Evidence 'evidenceSource' '')
  if ($evidenceSource -cne $EvidenceSource) { throw "readiness evidence must come from the accessibility tree" }

  $machineName = [string](Get-Field $Evidence 'machine' '')
  if ($MachineOverride -and $machineName -cne $MachineOverride) {
    throw "readiness evidence is for $machineName, not $MachineOverride"
  }
  $waitSeconds = [int](Get-Field $Evidence 'waitSeconds' 60)
  $imagesWritten = [int](Get-Field $Evidence 'imageFilesWritten' -1)
  $apps = @(Get-Field $Evidence 'apps' @())
  $quiet = Get-Field $Evidence 'quiet' $null
  $osl = Get-Field $Evidence 'osl' $null

  # The collection source is printed so replayed evidence can never be read as
  # a live probe of the machine.
  $collectionSource = [string](Get-Field $Evidence 'source' 'unknown')
  Write-Output ("VM-4958-MACHINE {0} apps={1} waitSeconds={2} evidence={3} source={4}" -f
    $machineName, $apps.Count, $waitSeconds, $EvidenceSource, $collectionSource)

  $notInstalled = New-Object 'System.Collections.Generic.List[string]'
  $notSignedIn = New-Object 'System.Collections.Generic.List[string]'
  $noWindow = New-Object 'System.Collections.Generic.List[string]'
  $accounts = New-Object 'System.Collections.Generic.List[object]'

  foreach ($app in $apps) {
    $name = [string](Get-Field $app 'name' '')
    $installed = [bool](Get-Field $app 'installed' $false)
    $version = [string](Get-Field $app 'version' '')
    $windowFound = [bool](Get-Field $app 'windowFound' $false)
    $windowSeconds = Get-Field $app 'windowSeconds' 0
    $signedIn = [bool](Get-Field $app 'signedIn' $false)
    $account = [string](Get-Field $app 'account' '')
    if (-not $version) { $version = '-' }
    if (-not $account) { $account = '-' }

    Write-Output ("VM-4958-APP {0} {1} installed={2} version={3} window={4} windowSeconds={5} signedIn={6} account={7}" -f
      $machineName, $name, (ConvertTo-Flag $installed), $version, (ConvertTo-Flag $windowFound),
      (Format-Seconds $windowSeconds), (ConvertTo-Flag $signedIn), $account)

    if (-not $installed) { $notInstalled.Add($name) }
    if (-not $windowFound) { $noWindow.Add($name) }
    if (-not $signedIn) { $notSignedIn.Add($name) } else { $accounts.Add([pscustomobject]@{ App = $name; Account = $account }) }
  }

  foreach ($name in $notInstalled) { Write-Output ("VM-4958-NOT-INSTALLED {0} {1}" -f $machineName, $name) }
  foreach ($name in $noWindow) { Write-Output ("VM-4958-NO-WINDOW {0} {1} seconds={2}" -f $machineName, $name, $waitSeconds) }
  foreach ($name in $notSignedIn) { Write-Output ("VM-4958-NOT-SIGNED-IN {0} {1}" -f $machineName, $name) }
  foreach ($entry in $accounts) { Write-Output ("VM-4958-ACCOUNT {0} {1} {2}" -f $machineName, $entry.App, $entry.Account) }

  $installedCount = $apps.Count - $notInstalled.Count
  $signedInCount = $apps.Count - $notSignedIn.Count
  Write-Output ("VM-4958-INSTALLED {0} {1} of {2}" -f $machineName, $installedCount, $apps.Count)
  Write-Output ("VM-4958-SIGNED-IN {0} {1} of {2}" -f $machineName, $signedInCount, $apps.Count)

  $machineQuiet = [bool](Get-Field $quiet 'quiet' $false)
  Write-Output ("VM-4958-QUIET {0} quiet={1} timeoutsZero={2} screenSaverOff={3} notificationBannersOff={4} lockUiWindows={5}" -f
    $machineName, (ConvertTo-Flag $machineQuiet),
    (ConvertTo-Flag (Get-Field $quiet 'displayAndSleepTimeoutsZero' $false)),
    (ConvertTo-Flag (Get-Field $quiet 'screenSaverOff' $false)),
    (ConvertTo-Flag (Get-Field $quiet 'notificationBannersOff' $false)),
    [int](Get-Field $quiet 'lockUiWindowsInAccessibilityTree' -1))

  $oslWindow = [bool](Get-Field $osl 'windowFound' $false)
  Write-Output ("VM-4958-OSL-WINDOW {0} window={1} title=""{2}"" seconds={3}" -f
    $machineName, (ConvertTo-Flag $oslWindow), [string](Get-Field $osl 'windowTitle' ''), (Format-Seconds (Get-Field $osl 'windowSeconds' 0)))

  Write-Output ("VM-4958-IMAGES-WRITTEN {0} {1}" -f $machineName, $imagesWritten)
  if ($imagesWritten -ne 0) { throw 'readiness evidence claims image files were written; this check saves 0 pictures' }

  $reasons = New-Object 'System.Collections.Generic.List[string]'
  if ($notInstalled.Count -gt 0) { $reasons.Add("notInstalled:$($notInstalled -join '+')") }
  if ($notSignedIn.Count -gt 0) { $reasons.Add("notSignedIn:$($notSignedIn -join '+')") }
  if ($noWindow.Count -gt 0) { $reasons.Add("noWindow:$($noWindow -join '+')") }
  if (-not $machineQuiet) { $reasons.Add('notQuiet') }
  if (-not $oslWindow) { $reasons.Add('oslNoWindow') }

  $summary = "installed={0} of {1} signedIn={2} of {3} quiet={4} oslWindow={5}" -f
    $installedCount, $apps.Count, $signedInCount, $apps.Count, (ConvertTo-Flag $machineQuiet), (ConvertTo-Flag $oslWindow)

  if ($reasons.Count -gt 0) {
    Write-Output ("VM-4958-NOT-READY {0} {1} reasons={2}" -f $machineName, $summary, ($reasons -join ','))
    return 1
  }
  Write-Output ("VM-4958-READY {0} {1}" -f $machineName, $summary)
  return 0
}

# ---------------------------------------------------------------------------

if (-not $OutputRoot) { $OutputRoot = Join-Path ([IO.Path]::GetTempPath()) 'osl-4958' }
[void](New-Item -ItemType Directory -Path $OutputRoot -Force -ErrorAction SilentlyContinue)

if ($Action -ceq 'Verdict') {
  if (-not $EvidenceJsonPath) { throw 'Verdict needs -EvidenceJsonPath' }
  if (-not (Test-Path -LiteralPath $EvidenceJsonPath -PathType Leaf)) { throw "readiness evidence not found: $EvidenceJsonPath" }
  $evidence = Get-Content -LiteralPath $EvidenceJsonPath -Raw | ConvertFrom-Json
  exit (Invoke-Verdict $evidence $Machine)
}

$collected = Invoke-Probe $Machine $Apps $WaitSeconds $OutputRoot
$json = $collected | ConvertTo-Json -Depth 8
if (-not $EvidenceJsonPath) { $EvidenceJsonPath = Join-Path $OutputRoot ("readiness-{0}.json" -f $Machine) }
[IO.File]::WriteAllText($EvidenceJsonPath, $json, [Text.UTF8Encoding]::new($false))
Write-Output ("VM-4958-EVIDENCE {0} {1}" -f $Machine, $EvidenceJsonPath)

if ($Action -ceq 'Probe') { exit 0 }
exit (Invoke-Verdict ($json | ConvertFrom-Json) $Machine)
