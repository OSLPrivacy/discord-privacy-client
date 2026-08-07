<#
TASK 4955 payload: disables desktop/browser interruptions and audits them from
the logged-on owner session.
#>
[CmdletBinding()]
param(
  [ValidateSet('Stage', 'Apply', 'Audit')]
  [string]$Action = 'Audit',

  [ValidateRange(0, 3600)]
  [int]$IdleSeconds = 0,

  [switch]$SummaryOnly
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes

$script:PowerSettings = @(
  @{ Name = 'display.videoidle'; Sub = '7516b95f-f776-4464-8c53-06167f40cc99'; Setting = '3c0bc021-c8a8-4e07-a973-6b14cbcb2b7e'; Kind = 'display' },
  @{ Name = 'display.consolelock'; Sub = '7516b95f-f776-4464-8c53-06167f40cc99'; Setting = '8ec4b3a5-6868-48c2-be75-4f3044be88a7'; Kind = 'display' },
  @{ Name = 'sleep.standbyidle'; Sub = '238c9fa8-0aad-41ed-83f4-97be242c8f20'; Setting = '29f6c1db-86da-48c5-9fdb-f2b67b1f44da'; Kind = 'sleep' },
  @{ Name = 'sleep.hibernateidle'; Sub = '238c9fa8-0aad-41ed-83f4-97be242c8f20'; Setting = '9d7815a6-7ee4-497e-8888-515a05f02364'; Kind = 'sleep' },
  @{ Name = 'sleep.unattendedidle'; Sub = '238c9fa8-0aad-41ed-83f4-97be242c8f20'; Setting = '7bc4a2f9-d8fc-4469-b07b-33eb785aaca0'; Kind = 'sleep' }
)

function Invoke-StageSelf {
  $payloadRoot = 'C:\OSL\desktop-runner\payloads'
  [void](New-Item -ItemType Directory -Path $payloadRoot -Force)
  if ([string]::IsNullOrWhiteSpace($PSCommandPath) -or -not (Test-Path -LiteralPath $PSCommandPath -PathType Leaf)) {
    throw 'stage source path is unavailable'
  }
  $sha = (Get-FileHash -LiteralPath $PSCommandPath -Algorithm SHA256).Hash.ToLowerInvariant()
  $destination = Join-Path $payloadRoot "quiet-signin-$sha.ps1"
  Copy-Item -LiteralPath $PSCommandPath -Destination $destination -Force
  [ordered]@{
    schema = 'quiet-signin-4955/v1'
    action = 'Stage'
    machine = $env:COMPUTERNAME
    payloadPath = $destination
    payloadSha256 = $sha
    verdict = 'staged'
  } | ConvertTo-Json -Depth 4 -Compress
}

function Ensure-Key([string]$Path) {
  if (-not (Test-Path -LiteralPath $Path)) { [void](New-Item -Path $Path -Force) }
}

function Set-Dword([string]$Path, [string]$Name, [int]$Value) {
  Ensure-Key $Path
  New-ItemProperty -LiteralPath $Path -Name $Name -PropertyType DWord -Value $Value -Force | Out-Null
}

function Set-StringValue([string]$Path, [string]$Name, [string]$Value) {
  Ensure-Key $Path
  New-ItemProperty -LiteralPath $Path -Name $Name -PropertyType String -Value $Value -Force | Out-Null
}

function Set-PowerTimeoutsNever {
  foreach ($setting in $script:PowerSettings) {
    & powercfg.exe /setacvalueindex SCHEME_CURRENT $setting.Sub $setting.Setting 0 | Out-Null
    & powercfg.exe /setdcvalueindex SCHEME_CURRENT $setting.Sub $setting.Setting 0 | Out-Null
  }
  & powercfg.exe /change monitor-timeout-ac 0 | Out-Null
  & powercfg.exe /change monitor-timeout-dc 0 | Out-Null
  & powercfg.exe /change standby-timeout-ac 0 | Out-Null
  & powercfg.exe /change standby-timeout-dc 0 | Out-Null
  & powercfg.exe /hibernate off | Out-Null
  & powercfg.exe /setactive SCHEME_CURRENT | Out-Null
}

function Read-PowerSetting([string]$Sub, [string]$Setting) {
  $text = (& powercfg.exe /query SCHEME_CURRENT $Sub $Setting 2>$null) -join "`n"
  $ac = $null
  $dc = $null
  if ($text -match 'Current AC Power Setting Index:\s*0x([0-9a-fA-F]+)') { $ac = [Convert]::ToInt64($Matches[1], 16) }
  if ($text -match 'Current DC Power Setting Index:\s*0x([0-9a-fA-F]+)') { $dc = [Convert]::ToInt64($Matches[1], 16) }
  if ($null -eq $ac -or $null -eq $dc) {
    $text = (& powercfg.exe /qh SCHEME_CURRENT $Sub $Setting 2>$null) -join "`n"
    if ($text -match 'Current AC Power Setting Index:\s*0x([0-9a-fA-F]+)') { $ac = [Convert]::ToInt64($Matches[1], 16) }
    if ($text -match 'Current DC Power Setting Index:\s*0x([0-9a-fA-F]+)') { $dc = [Convert]::ToInt64($Matches[1], 16) }
  }
  [ordered]@{ ac = $ac; dc = $dc }
}

function Set-QuietRegistry {
  Set-StringValue 'HKCU:\Control Panel\Desktop' 'ScreenSaveActive' '0'
  Set-StringValue 'HKCU:\Control Panel\Desktop' 'ScreenSaverIsSecure' '0'
  Set-StringValue 'HKCU:\Control Panel\Desktop' 'ScreenSaveTimeOut' '0'
  Set-Dword 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Policies\System' 'DisableLockWorkstation' 1
  Set-Dword 'HKLM:\SOFTWARE\Policies\Microsoft\Windows\Personalization' 'NoLockScreen' 1
  Set-Dword 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\System' 'InactivityTimeoutSecs' 0

  Set-Dword 'HKCU:\Software\Microsoft\Windows\CurrentVersion\PushNotifications' 'ToastEnabled' 0
  $notificationPath = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Notifications\Settings'
  Set-Dword $notificationPath 'NOC_GLOBAL_SETTING_TOASTS_ENABLED' 0
  Set-Dword $notificationPath 'NOC_GLOBAL_SETTING_BADGE_ENABLED' 0
  Set-Dword $notificationPath 'NOC_GLOBAL_SETTING_ALLOW_TOASTS_ABOVE_LOCK' 0
  Set-Dword $notificationPath 'NOC_GLOBAL_SETTING_ALLOW_CRITICAL_TOASTS_ABOVE_LOCK' 0
  Set-Dword $notificationPath 'NOC_GLOBAL_SETTING_DND' 1

  $wu = 'HKLM:\SOFTWARE\Policies\Microsoft\Windows\WindowsUpdate'
  $au = Join-Path $wu 'AU'
  Set-Dword $wu 'SetActiveHours' 1
  Set-Dword $wu 'ActiveHoursStart' 0
  Set-Dword $wu 'ActiveHoursEnd' 23
  Set-Dword $au 'NoAutoRebootWithLoggedOnUsers' 1
  Set-Dword $au 'AlwaysAutoRebootAtScheduledTime' 0
  Set-Dword $au 'AUOptions' 3

  Set-ChromiumPolicies 'HKLM:\SOFTWARE\Policies\Google\Chrome'
  Set-ChromiumPolicies 'HKLM:\SOFTWARE\Policies\Microsoft\Edge'
  Set-ChromiumPolicies 'HKCU:\SOFTWARE\Policies\Google\Chrome'
  Set-ChromiumPolicies 'HKCU:\SOFTWARE\Policies\Microsoft\Edge'
  Set-FirefoxPolicies
}

function Set-ChromiumPolicies([string]$Path) {
  Set-Dword $Path 'HideFirstRunExperience' 1
  Set-Dword $Path 'BrowserSignin' 0
  Set-Dword $Path 'SyncDisabled' 1
  Set-Dword $Path 'PromotionalTabsEnabled' 0
  Set-Dword $Path 'DefaultBrowserSettingEnabled' 0
  Set-Dword $Path 'MetricsReportingEnabled' 0
  Set-Dword $Path 'PasswordManagerEnabled' 0
}

function Find-BrowserExe([string[]]$Candidates) {
  foreach ($candidate in $Candidates) {
    if (Test-Path -LiteralPath $candidate -PathType Leaf) { return $candidate }
  }
  return $null
}

function Set-FirefoxPolicies {
  $firefox = Find-BrowserExe @(
    "$env:ProgramFiles\Mozilla Firefox\firefox.exe",
    "${env:ProgramFiles(x86)}\Mozilla Firefox\firefox.exe",
    "$env:LOCALAPPDATA\Mozilla Firefox\firefox.exe"
  )
  Set-Dword 'HKLM:\SOFTWARE\Policies\Mozilla\Firefox' 'DisableFirefoxAccounts' 1
  Set-Dword 'HKLM:\SOFTWARE\Policies\Mozilla\Firefox' 'DontCheckDefaultBrowser' 1
  if (-not $firefox) { return }
  $firefoxRoot = Split-Path -Parent $firefox
  $distribution = Join-Path $firefoxRoot 'distribution'
  [void](New-Item -ItemType Directory -Path $distribution -Force)
  $policies = [ordered]@{
    policies = [ordered]@{
      DisableFirefoxAccounts = $true
      DontCheckDefaultBrowser = $true
      OverrideFirstRunPage = ''
      OverridePostUpdatePage = ''
      Preferences = [ordered]@{
        'browser.aboutwelcome.enabled' = [ordered]@{ Value = $false; Status = 'locked' }
        'browser.shell.checkDefaultBrowser' = [ordered]@{ Value = $false; Status = 'locked' }
        'browser.startup.homepage' = [ordered]@{ Value = 'about:blank'; Status = 'locked' }
        'browser.startup.homepage_override.mstone' = [ordered]@{ Value = 'ignore'; Status = 'locked' }
        'trailhead.firstrun.didSeeAboutWelcome' = [ordered]@{ Value = $true; Status = 'locked' }
        'trailhead.firstrun.branches' = [ordered]@{ Value = 'nofirstrun-empty'; Status = 'locked' }
        'datareporting.policy.dataSubmissionPolicyBypassNotification' = [ordered]@{ Value = $true; Status = 'locked' }
      }
    }
  }
  $policies | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $distribution 'policies.json') -Encoding UTF8
  $defaultsPref = Join-Path $firefoxRoot 'defaults\pref'
  [void](New-Item -ItemType Directory -Path $defaultsPref -Force)
  @'
pref("general.config.filename", "osl-quiet-signin.cfg");
pref("general.config.obscure_value", 0);
'@ | Set-Content -LiteralPath (Join-Path $defaultsPref 'osl-quiet-signin-autoconfig.js') -Encoding ASCII
  @'
// OSL quiet-signin VM policy. First line must be a comment for Firefox autoconfig.
lockPref("browser.aboutwelcome.enabled", false);
lockPref("browser.shell.checkDefaultBrowser", false);
lockPref("browser.startup.homepage", "about:blank");
lockPref("browser.startup.homepage_override.mstone", "ignore");
lockPref("identity.fxaccounts.enabled", false);
lockPref("startup.homepage_welcome_url", "");
lockPref("startup.homepage_welcome_url.additional", "");
lockPref("trailhead.firstrun.branches", "nofirstrun-empty");
lockPref("datareporting.policy.dataSubmissionPolicyBypassNotification", true);
defaultPref("trailhead.firstrun.didSeeAboutWelcome", true);
'@ | Set-Content -LiteralPath (Join-Path $firefoxRoot 'osl-quiet-signin.cfg') -Encoding ASCII
  $defaultProfilePrefs = @'
user_pref("browser.aboutwelcome.enabled", false);
user_pref("browser.shell.checkDefaultBrowser", false);
user_pref("browser.startup.homepage", "about:blank");
user_pref("browser.startup.homepage_override.mstone", "ignore");
user_pref("identity.fxaccounts.enabled", false);
user_pref("startup.homepage_welcome_url", "");
user_pref("startup.homepage_welcome_url.additional", "");
user_pref("trailhead.firstrun.branches", "nofirstrun-empty");
user_pref("datareporting.policy.dataSubmissionPolicyBypassNotification", true);
user_pref("trailhead.firstrun.didSeeAboutWelcome", true);
'@
  foreach ($profileRoot in @(
      (Join-Path $firefoxRoot 'defaults\profile'),
      (Join-Path $firefoxRoot 'browser\defaults\profile')
    )) {
    [void](New-Item -ItemType Directory -Path $profileRoot -Force)
    $defaultProfilePrefs | Set-Content -LiteralPath (Join-Path $profileRoot 'user.js') -Encoding ASCII
  }
}

function Get-RegistryValue($Path, $Name, $Default = $null) {
  try {
    $item = Get-ItemProperty -LiteralPath $Path -ErrorAction Stop
    if ($null -ne $item.PSObject.Properties[$Name]) { return $item.$Name }
  } catch {}
  return $Default
}

function Get-SessionState {
  $sessionId = [Diagnostics.Process]::GetCurrentProcess().SessionId
  $explorers = @(Get-CimInstance Win32_Process -Filter "Name = 'explorer.exe'" | Where-Object { [int]$_.SessionId -eq $sessionId })
  $owners = @()
  foreach ($explorer in $explorers) {
    $owner = Invoke-CimMethod -InputObject $explorer -MethodName GetOwner
    $owners += ('{0}\{1}' -f $owner.Domain, $owner.User)
  }
  $logonUi = @(Get-CimInstance Win32_Process -Filter "Name = 'LogonUI.exe'" | Where-Object { [int]$_.SessionId -eq $sessionId })
  [ordered]@{
    sessionId = $sessionId
    currentUser = [Security.Principal.WindowsIdentity]::GetCurrent().Name
    explorerCount = $explorers.Count
    explorerOwners = @($owners)
    logonUiCount = $logonUi.Count
    unlocked = ($explorers.Count -eq 1 -and $owners.Count -eq 1 -and $owners[0] -match '\\osladmin$' -and $logonUi.Count -eq 0)
    screenshot = Get-DesktopPaint
  }
}

function Get-DesktopPaint {
  $bounds = [Windows.Forms.Screen]::PrimaryScreen.Bounds
  if ($bounds.Width -le 0 -or $bounds.Height -le 0) {
    return [ordered]@{ width = $bounds.Width; height = $bounds.Height; sampledPixels = 0; nonBlackPixels = 0; distinctColors = 0; painted = $false }
  }
  $bitmap = [Drawing.Bitmap]::new($bounds.Width, $bounds.Height)
  $graphics = [Drawing.Graphics]::FromImage($bitmap)
  try {
    $graphics.CopyFromScreen($bounds.Location, [Drawing.Point]::Empty, $bounds.Size)
    $distinct = [Collections.Generic.HashSet[int]]::new()
    $sampled = 0
    $nonBlack = 0
    $stepX = [Math]::Max(1, [int]($bounds.Width / 64))
    $stepY = [Math]::Max(1, [int]($bounds.Height / 36))
    for ($y = 0; $y -lt $bounds.Height; $y += $stepY) {
      for ($x = 0; $x -lt $bounds.Width; $x += $stepX) {
        $color = $bitmap.GetPixel($x, $y)
        [void]$distinct.Add($color.ToArgb())
        $sampled++
        if (($color.R + $color.G + $color.B) -gt 30) { $nonBlack++ }
      }
    }
    [ordered]@{
      width = $bounds.Width
      height = $bounds.Height
      sampledPixels = $sampled
      nonBlackPixels = $nonBlack
      distinctColors = $distinct.Count
      painted = ($sampled -gt 0 -and $nonBlack -gt 0 -and $distinct.Count -gt 1)
    }
  } finally {
    $graphics.Dispose()
    $bitmap.Dispose()
  }
}

function Get-WindowTextsForProcessIds([int[]]$ProcessIds) {
  if ($ProcessIds.Count -eq 0) { return @() }
  $texts = [Collections.Generic.List[string]]::new()
  $wanted = [Collections.Generic.HashSet[int]]::new()
  foreach ($id in $ProcessIds) { [void]$wanted.Add($id) }
  $root = [Windows.Automation.AutomationElement]::RootElement
  $children = $root.FindAll([Windows.Automation.TreeScope]::Children, [Windows.Automation.Condition]::TrueCondition)
  foreach ($child in $children) {
    try {
      if (-not $wanted.Contains([int]$child.Current.ProcessId)) { continue }
      Add-ElementNames $child $texts 0
    } catch {}
  }
  return @($texts | Where-Object { -not [string]::IsNullOrWhiteSpace($_) } | Select-Object -Unique)
}

function Add-ElementNames($Element, [Collections.Generic.List[string]]$Texts, [int]$Depth) {
  if ($Depth -gt 8 -or $Texts.Count -gt 500) { return }
  try {
    $name = [string]$Element.Current.Name
    if (-not [string]::IsNullOrWhiteSpace($name)) { $Texts.Add($name) }
    $children = $Element.FindAll([Windows.Automation.TreeScope]::Children, [Windows.Automation.Condition]::TrueCondition)
    foreach ($child in $children) { Add-ElementNames $child $Texts ($Depth + 1) }
  } catch {}
}

function Count-PromptMatches([string[]]$Texts, [string[]]$Urls, [string[]]$Titles) {
  $joined = (@($Texts) + @($Urls) + @($Titles)) -join "`n"
  [ordered]@{
    firstRunPages = ([regex]::Matches($joined, '(?i)\b(first[- ]run|welcome|getting started|what''s new|about:welcome|chrome://welcome|edge://welcome|firstrun)\b')).Count
    signInPrompts = ([regex]::Matches($joined, '(?i)\b(sign in to (chrome|edge|firefox|browser)|turn on sync|sync across|sync your)\b')).Count
    defaultBrowserPrompts = ([regex]::Matches($joined, '(?i)\b(default browser|set as default|make .* default)\b')).Count
  }
}

function New-FreePort {
  $listener = [Net.Sockets.TcpListener]::new([Net.IPAddress]::Parse('127.0.0.1'), 0)
  $listener.Start()
  try { return [int]$listener.LocalEndpoint.Port } finally { $listener.Stop() }
}

function Invoke-ChromiumFreshProfileAudit([string]$Name, [string]$Exe) {
  Stop-NamedBrowserProcessesInSession ([IO.Path]::GetFileNameWithoutExtension($Exe))
  $profile = Join-Path $env:TEMP ("osl-4955-" + $Name.ToLowerInvariant() + '-' + [guid]::NewGuid())
  [void](New-Item -ItemType Directory -Path $profile -Force)
  $port = New-FreePort
  $args = @(
    "--user-data-dir=$profile",
    "--remote-debugging-port=$port",
    '--force-renderer-accessibility',
    '--disable-features=Translate'
  )
  $process = Start-Process -FilePath $Exe -ArgumentList $args -PassThru
  try {
    Start-Sleep -Seconds 8
    $targets = @()
    try { $targets = @(Invoke-RestMethod -Uri "http://127.0.0.1:$port/json" -TimeoutSec 3) } catch {}
    $pids = @(Get-CimInstance Win32_Process | Where-Object {
      [int]$_.SessionId -eq [Diagnostics.Process]::GetCurrentProcess().SessionId -and
        $_.CommandLine -and $_.CommandLine.Contains($profile)
    } | ForEach-Object { [int]$_.ProcessId })
    $texts = Get-WindowTextsForProcessIds $pids
    $urls = @($targets | ForEach-Object { [string]$_.url })
    $titles = @($targets | ForEach-Object { [string]$_.title }) + @($texts | Select-Object -First 50)
    $counts = Count-PromptMatches $texts $urls $titles
    [ordered]@{
      name = $Name
      executable = $Exe
      launched = $true
      profileRoot = $profile
      targetUrls = @($urls)
      targetTitles = @($targets | ForEach-Object { [string]$_.title })
      textSampleCount = $texts.Count
      counts = $counts
      pass = ($counts.firstRunPages -eq 0 -and $counts.signInPrompts -eq 0 -and $counts.defaultBrowserPrompts -eq 0)
    }
  } finally {
    Stop-BrowserProcessesForProfile $profile
    if ($process -and -not $process.HasExited) { Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue }
  }
}

function Invoke-FirefoxFreshProfileAudit([string]$Exe) {
  Stop-NamedBrowserProcessesInSession 'firefox'
  $profile = Join-Path $env:TEMP ("osl-4955-firefox-" + [guid]::NewGuid())
  [void](New-Item -ItemType Directory -Path $profile -Force)
  $process = Start-Process -FilePath $Exe -ArgumentList @('-no-remote', '-profile', $profile) -PassThru
  try {
    Start-Sleep -Seconds 10
    $pids = @(Get-CimInstance Win32_Process | Where-Object {
      [int]$_.SessionId -eq [Diagnostics.Process]::GetCurrentProcess().SessionId -and
        $_.CommandLine -and $_.CommandLine.Contains($profile)
    } | ForEach-Object { [int]$_.ProcessId })
    if ($pids.Count -eq 0 -and $process) { $pids = @([int]$process.Id) }
    $texts = Get-WindowTextsForProcessIds $pids
    $counts = Count-PromptMatches $texts @() $texts
    [ordered]@{
      name = 'Firefox'
      executable = $Exe
      launched = $true
      profileRoot = $profile
      targetUrls = @()
      targetTitles = @($texts | Select-Object -First 20)
      textSampleCount = $texts.Count
      counts = $counts
      pass = ($counts.firstRunPages -eq 0 -and $counts.signInPrompts -eq 0 -and $counts.defaultBrowserPrompts -eq 0)
    }
  } finally {
    Stop-BrowserProcessesForProfile $profile
    if ($process -and -not $process.HasExited) { Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue }
  }
}

function Stop-BrowserProcessesForProfile([string]$Profile) {
  @(Get-CimInstance Win32_Process | Where-Object { $_.CommandLine -and $_.CommandLine.Contains($Profile) }) |
    ForEach-Object { Stop-Process -Id ([int]$_.ProcessId) -Force -ErrorAction SilentlyContinue }
}

function Stop-NamedBrowserProcessesInSession([string]$ProcessName) {
  @(Get-CimInstance Win32_Process -Filter "Name = '$ProcessName.exe'" | Where-Object {
    [int]$_.SessionId -eq [Diagnostics.Process]::GetCurrentProcess().SessionId
  }) | ForEach-Object {
    Stop-Process -Id ([int]$_.ProcessId) -Force -ErrorAction SilentlyContinue
  }
  Start-Sleep -Milliseconds 500
}

function Invoke-BrowserAudits {
  $browsers = [Collections.Generic.List[object]]::new()
  $chrome = Find-BrowserExe @(
    "$env:ProgramFiles\Google\Chrome\Application\chrome.exe",
    "${env:ProgramFiles(x86)}\Google\Chrome\Application\chrome.exe",
    "$env:LOCALAPPDATA\Google\Chrome\Application\chrome.exe"
  )
  $edge = Find-BrowserExe @(
    "${env:ProgramFiles(x86)}\Microsoft\Edge\Application\msedge.exe",
    "$env:ProgramFiles\Microsoft\Edge\Application\msedge.exe"
  )
  $firefox = Find-BrowserExe @(
    "$env:ProgramFiles\Mozilla Firefox\firefox.exe",
    "${env:ProgramFiles(x86)}\Mozilla Firefox\firefox.exe",
    "$env:LOCALAPPDATA\Mozilla Firefox\firefox.exe"
  )
  if ($chrome) { $browsers.Add((Invoke-ChromiumFreshProfileAudit 'Chrome' $chrome)) } else { $browsers.Add([ordered]@{ name = 'Chrome'; launched = $false; pass = $false }) }
  if ($firefox) { $browsers.Add((Invoke-FirefoxFreshProfileAudit $firefox)) } else { $browsers.Add([ordered]@{ name = 'Firefox'; launched = $false; pass = $false }) }
  if ($edge) { $browsers.Add((Invoke-ChromiumFreshProfileAudit 'Edge' $edge)) } else { $browsers.Add([ordered]@{ name = 'Edge'; launched = $false; pass = $false }) }
  return @($browsers)
}

function Get-QuietSettings {
  $power = [ordered]@{}
  foreach ($setting in $script:PowerSettings) {
    $power[$setting.Name] = Read-PowerSetting $setting.Sub $setting.Setting
  }
  $timeoutsZero = $true
  foreach ($entry in $power.GetEnumerator()) {
    if ($null -eq $entry.Value.ac -or $null -eq $entry.Value.dc -or $entry.Value.ac -ne 0 -or $entry.Value.dc -ne 0) {
      $timeoutsZero = $false
    }
  }
  $previousErrorActionPreference = $ErrorActionPreference
  $ErrorActionPreference = 'Continue'
  try {
    $powerAvailability = ((& cmd.exe /c 'powercfg.exe /a 2>nul') -join "`n")
  } finally {
    $ErrorActionPreference = $previousErrorActionPreference
  }
  $availableSleepStates = ($powerAvailability -split 'The following sleep states are not available', 2)[0]
  $hibernateAvailable = $availableSleepStates -match '(?im)^\s*Hibernate\s*$'
  $screensaverOff = (
    [string](Get-RegistryValue 'HKCU:\Control Panel\Desktop' 'ScreenSaveActive' '') -eq '0' -and
    [string](Get-RegistryValue 'HKCU:\Control Panel\Desktop' 'ScreenSaverIsSecure' '') -eq '0' -and
    [string](Get-RegistryValue 'HKCU:\Control Panel\Desktop' 'ScreenSaveTimeOut' '') -eq '0'
  )
  $updateRestartWithoutAsking = [int](Get-RegistryValue 'HKLM:\SOFTWARE\Policies\Microsoft\Windows\WindowsUpdate\AU' 'AlwaysAutoRebootAtScheduledTime' 0)
  $noAutoRebootWithLoggedOnUsers = [int](Get-RegistryValue 'HKLM:\SOFTWARE\Policies\Microsoft\Windows\WindowsUpdate\AU' 'NoAutoRebootWithLoggedOnUsers' 0)
  [ordered]@{
    power = $power
    everyDisplayAndSleepTimeoutZero = $timeoutsZero
    hibernateAvailable = $hibernateAvailable
    screensaverOff = $screensaverOff
    lockScreenOff = ([int](Get-RegistryValue 'HKLM:\SOFTWARE\Policies\Microsoft\Windows\Personalization' 'NoLockScreen' 0) -eq 1)
    workstationLockDisabled = ([int](Get-RegistryValue 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Policies\System' 'DisableLockWorkstation' 0) -eq 1)
    notificationBannersOff = (
      [int](Get-RegistryValue 'HKCU:\Software\Microsoft\Windows\CurrentVersion\PushNotifications' 'ToastEnabled' 1) -eq 0 -and
      [int](Get-RegistryValue 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Notifications\Settings' 'NOC_GLOBAL_SETTING_TOASTS_ENABLED' 1) -eq 0
    )
    focusAssistDoNotDisturbOn = ([int](Get-RegistryValue 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Notifications\Settings' 'NOC_GLOBAL_SETTING_DND' 0) -eq 1)
    windowsUpdateRestartWithoutAsking = if ($updateRestartWithoutAsking -eq 0 -and $noAutoRebootWithLoggedOnUsers -eq 1) { 'off' } else { 'on' }
    browserPolicies = [ordered]@{
      chromeHideFirstRun = [int](Get-RegistryValue 'HKLM:\SOFTWARE\Policies\Google\Chrome' 'HideFirstRunExperience' 0)
      chromeBrowserSignin = [int](Get-RegistryValue 'HKLM:\SOFTWARE\Policies\Google\Chrome' 'BrowserSignin' 1)
      chromeDefaultBrowserPrompt = [int](Get-RegistryValue 'HKLM:\SOFTWARE\Policies\Google\Chrome' 'DefaultBrowserSettingEnabled' 1)
      edgeHideFirstRun = [int](Get-RegistryValue 'HKLM:\SOFTWARE\Policies\Microsoft\Edge' 'HideFirstRunExperience' 0)
      edgeBrowserSignin = [int](Get-RegistryValue 'HKLM:\SOFTWARE\Policies\Microsoft\Edge' 'BrowserSignin' 1)
      edgeDefaultBrowserPrompt = [int](Get-RegistryValue 'HKLM:\SOFTWARE\Policies\Microsoft\Edge' 'DefaultBrowserSettingEnabled' 1)
      firefoxDisableAccounts = [int](Get-RegistryValue 'HKLM:\SOFTWARE\Policies\Mozilla\Firefox' 'DisableFirefoxAccounts' 0)
      firefoxDontCheckDefaultBrowser = [int](Get-RegistryValue 'HKLM:\SOFTWARE\Policies\Mozilla\Firefox' 'DontCheckDefaultBrowser' 0)
    }
  }
}

function Invoke-Audit {
  $before = Get-SessionState
  if ($IdleSeconds -gt 0) { Start-Sleep -Seconds $IdleSeconds }
  $after = Get-SessionState
  $settings = Get-QuietSettings
  $browsers = Invoke-BrowserAudits
  $browserEvidence = if ($SummaryOnly) {
    @($browsers | ForEach-Object {
      [ordered]@{
        name = $_.name
        launched = [bool]$_.launched
        counts = $_.counts
        pass = [bool]$_.pass
      }
    })
  } else {
    @($browsers)
  }
  $sameSession = (
    $before.sessionId -eq $after.sessionId -and
    $before.currentUser -ceq $after.currentUser -and
    $after.unlocked -and
    [bool]$after.screenshot.painted
  )
  $browserPass = (@($browsers | Where-Object { -not $_.pass }).Count -eq 0)
  $settingsPass = (
    $settings.everyDisplayAndSleepTimeoutZero -and
    -not $settings.hibernateAvailable -and
    $settings.screensaverOff -and
    $settings.lockScreenOff -and
    $settings.workstationLockDisabled -and
    $settings.notificationBannersOff -and
    $settings.focusAssistDoNotDisturbOn -and
    $settings.windowsUpdateRestartWithoutAsking -ceq 'off'
  )
  [ordered]@{
    schema = 'quiet-signin-4955/v1'
    action = 'Audit'
    idleSeconds = $IdleSeconds
    machine = $env:COMPUTERNAME
    beforeSession = $before
    afterSession = $after
    sameUnlockedPaintedSession = $sameSession
    settings = $settings
    browsers = @($browserEvidence)
    verdict = if ($sameSession -and $settingsPass -and $browserPass) { 'pass' } else { 'fail' }
  }
}

if ($Action -ceq 'Stage') {
  Invoke-StageSelf
  exit 0
} elseif ($Action -ceq 'Apply') {
  Set-PowerTimeoutsNever
  Set-QuietRegistry
  $result = [ordered]@{
    schema = 'quiet-signin-4955/v1'
    action = 'Apply'
    machine = $env:COMPUTERNAME
    settings = Get-QuietSettings
    verdict = 'applied'
  }
} else {
  $result = Invoke-Audit
}

$result | ConvertTo-Json -Depth 12 -Compress
if ($result.verdict -ceq 'fail') { exit 1 }
