<#
TASK 4958 readiness check payload. Runs inside the interactive desktop session
through scripts/qa/osl-vm-desktop-runner.ps1 (the 4951 runner).

For the machine it runs on it reports, for each of the six carrier apps:
installed + version, signed in + account handle, plus whether the machine is
quiet and whether OSL itself opens a window. All UI evidence comes from the
UI Automation accessibility tree. It never captures or saves any picture:
OSL blanks its own window against capture, so a black image would be worse
than none. It exits 1 whenever any named app is missing, signed out, or shows
no window inside 60 seconds.
#>
param(
    [string]$ExpectedMachine = '',
    [ValidateRange(5, 300)][int]$WindowWaitSeconds = 60
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$ScriptStartUtc = [DateTime]::UtcNow
$UserRoot = 'C:\Users\osladmin'
$StateRoot = 'C:\OSL\qa-4958'
New-Item -ItemType Directory -Force -Path $StateRoot | Out-Null

Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes

$Machine = [string]$env:COMPUTERNAME
if ($ExpectedMachine -and ($Machine.ToLowerInvariant() -ne $ExpectedMachine.ToLowerInvariant())) {
    Write-Output ("VM-4958-WRONG-MACHINE expected={0} actual={1}" -f $ExpectedMachine, $Machine)
    exit 1
}

function Get-FirstExistingPath([string[]]$Candidates) {
    foreach ($candidate in $Candidates) {
        $item = Get-ChildItem -Path $candidate -ErrorAction SilentlyContinue | Select-Object -First 1
        if ($item) { return [string]$item.FullName }
    }
    return $null
}

function Get-ExeVersion([string]$Path) {
    try {
        $info = (Get-Item -LiteralPath $Path).VersionInfo
        foreach ($field in @('ProductVersion', 'FileVersion')) {
            $value = [string]$info.$field
            if (-not [string]::IsNullOrWhiteSpace($value)) { return $value.Trim() }
        }
    } catch { }
    return 'unknown'
}

function Stop-AppProcesses([string[]]$Names) {
    foreach ($name in $Names) {
        Get-Process -Name $name -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
    }
}

function Wait-UiaWindow([string[]]$ProcessNames, [int]$DeadlineSeconds) {
    # Accessibility-tree window proof: a UI Automation root element reachable
    # from the process main window handle, carrying a non-empty Name.
    $deadline = [DateTime]::UtcNow.AddSeconds($DeadlineSeconds)
    while ([DateTime]::UtcNow -lt $deadline) {
        foreach ($procName in $ProcessNames) {
            foreach ($proc in Get-Process -Name $procName -ErrorAction SilentlyContinue) {
                try {
                    $proc.Refresh()
                    if ($proc.MainWindowHandle -eq 0) { continue }
                    $root = [System.Windows.Automation.AutomationElement]::FromHandle($proc.MainWindowHandle)
                    if (-not $root) { continue }
                    $title = [string]$root.Current.Name
                    if (-not [string]::IsNullOrWhiteSpace($title)) {
                        return [pscustomobject]@{ Found = $true; Title = $title.Trim() }
                    }
                } catch { }
            }
        }
        Start-Sleep -Milliseconds 750
    }
    return [pscustomobject]@{ Found = $false; Title = $null }
}

function Get-UiaTextForProcessNames([string[]]$ProcessNames, [int]$DeadlineSeconds) {
    $deadline = [DateTime]::UtcNow.AddSeconds($DeadlineSeconds)
    $texts = New-Object System.Collections.Generic.List[string]
    while ([DateTime]::UtcNow -lt $deadline) {
        $texts.Clear()
        foreach ($procName in $ProcessNames) {
            foreach ($proc in Get-Process -Name $procName -ErrorAction SilentlyContinue) {
                try {
                    if ($proc.MainWindowHandle -eq 0) { continue }
                    $root = [System.Windows.Automation.AutomationElement]::FromHandle($proc.MainWindowHandle)
                    if (-not $root) { continue }
                    $walker = [System.Windows.Automation.TreeWalker]::ControlViewWalker
                    $queue = New-Object 'System.Collections.Generic.Queue[System.Windows.Automation.AutomationElement]'
                    $queue.Enqueue($root)
                    $seen = 0
                    while ($queue.Count -gt 0 -and $seen -lt 600) {
                        $seen += 1
                        $node = $queue.Dequeue()
                        foreach ($property in @(
                            [System.Windows.Automation.AutomationElement]::NameProperty,
                            [System.Windows.Automation.AutomationElement]::HelpTextProperty
                        )) {
                            $value = [string]$node.GetCurrentPropertyValue($property, $true)
                            if (-not [string]::IsNullOrWhiteSpace($value)) { $texts.Add($value.Trim()) }
                        }
                        $child = $walker.GetFirstChild($node)
                        while ($child) {
                            $queue.Enqueue($child)
                            $child = $walker.GetNextSibling($child)
                        }
                    }
                } catch { }
            }
        }
        if ($texts.Count -gt 0) { break }
        Start-Sleep -Seconds 1
    }
    return (($texts | Sort-Object -Unique) -join "`n")
}

function Get-RegexAccount([string]$Text, [string[]]$Patterns) {
    foreach ($pattern in $Patterns) {
        $m = [regex]::Match($Text, $pattern, [Text.RegularExpressions.RegexOptions]::IgnoreCase)
        if ($m.Success -and $m.Groups.Count -gt 1) { return $m.Groups[1].Value.Trim() }
        if ($m.Success) { return $m.Value.Trim() }
    }
    return $null
}

function Test-SignInPrompt([pscustomobject]$App, [string]$UiText) {
    foreach ($prompt in $App.SignInPrompts) {
        if ($UiText -match [regex]::Escape($prompt)) { return $prompt }
    }
    return $null
}

$Apps = @(
    [pscustomobject]@{
        Name = 'Discord'
        ExeCandidates = @("$UserRoot\AppData\Local\Discord\app-*\Discord.exe", "$UserRoot\AppData\Local\Discord\Update.exe")
        UpdateLaunchArguments = @('--processStart', 'Discord.exe')
        ProcessNames = @('Discord')
        SignInPrompts = @('Log In', 'Email or Phone Number', 'Forgot your password', 'Download Discord')
        AccountPatterns = @('(?m)^@?([A-Za-z0-9_.-]{2,32}#[0-9]{4})$', '(?m)^@([A-Za-z0-9_.-]{2,32})$')
    },
    [pscustomobject]@{
        Name = 'Telegram'
        ExeCandidates = @("$UserRoot\AppData\Roaming\Telegram Desktop\Telegram.exe", "$UserRoot\AppData\Local\Programs\Telegram Desktop\Telegram.exe", 'C:\Program Files\Telegram Desktop\Telegram.exe')
        UpdateLaunchArguments = @()
        ProcessNames = @('Telegram')
        SignInPrompts = @('Start Messaging', 'Log in by phone Number', 'Please enter your phone number', 'QR code')
        AccountPatterns = @('(?m)^\+?[0-9][0-9 ()-]{6,}$', '(?m)^@([A-Za-z0-9_]{5,32})$')
    },
    [pscustomobject]@{
        Name = 'Signal'
        ExeCandidates = @("$UserRoot\AppData\Local\Programs\signal-desktop\Signal.exe")
        UpdateLaunchArguments = @()
        ProcessNames = @('Signal')
        SignInPrompts = @('Link your Signal account', 'Scan the QR code', 'Set up Signal', 'Link Device')
        AccountPatterns = @('(?m)^\+?[0-9][0-9 ()-]{6,}$')
    },
    [pscustomobject]@{
        Name = 'WhatsApp'
        ExeCandidates = @("$UserRoot\AppData\Local\WhatsApp\WhatsApp.exe", 'C:\Program Files\WindowsApps\*WhatsApp*\WhatsApp.exe')
        UpdateLaunchArguments = @()
        ProcessNames = @('WhatsApp')
        SignInPrompts = @('Link with phone number', 'Link a device', 'Use WhatsApp on your phone', 'QR code')
        AccountPatterns = @('(?m)^\+?[0-9][0-9 ()-]{6,}$')
    },
    [pscustomobject]@{
        Name = 'Chrome'
        ExeCandidates = @('C:\Program Files\Google\Chrome\Application\chrome.exe', 'C:\Program Files (x86)\Google\Chrome\Application\chrome.exe')
        UpdateLaunchArguments = @()
        LaunchArguments = @('--new-window', 'chrome://settings/people')
        ProcessNames = @('chrome')
        SignInPrompts = @('Sign in to Chrome', 'Turn on sync', 'Add account', 'You and Google')
        AccountPatterns = @('[A-Z0-9._%+-]+@[A-Z0-9.-]+\.[A-Z]{2,}')
    },
    [pscustomobject]@{
        Name = 'Firefox'
        ExeCandidates = @('C:\Program Files\Mozilla Firefox\firefox.exe', 'C:\Program Files (x86)\Mozilla Firefox\firefox.exe')
        UpdateLaunchArguments = @()
        LaunchArguments = @('-new-window', 'about:preferences#sync')
        ProcessNames = @('firefox')
        SignInPrompts = @('Sign in to sync', 'Enter your email', 'Firefox account', 'Mozilla account', 'Sign in')
        AccountPatterns = @('[A-Z0-9._%+-]+@[A-Z0-9.-]+\.[A-Z]{2,}')
    }
)

$OslExeCandidates = @(
    "$UserRoot\Desktop\OSL Privacy\OSL Privacy.exe",
    'C:\Users\osltest\Desktop\OSL Privacy\OSL Privacy.exe',
    "$UserRoot\AppData\Local\Programs\OSL*\OSL*.exe",
    'C:\Program Files\OSL*\OSL*.exe',
    'C:\OSL\qa\OSL-Privacy.exe'
)
$OslProcessNames = @('OSL Privacy', 'OSL-Privacy', 'osl-hub')

function Get-PowerSettingIndexes([string]$SubAlias, [string]$SettingAlias) {
    $ac = $null
    $dc = $null
    $raw = & powercfg.exe /query SCHEME_CURRENT $SubAlias $SettingAlias 2>$null
    foreach ($line in @($raw)) {
        if ($line -match 'Current AC Power Setting Index:\s*0x([0-9a-fA-F]+)') { $ac = [Convert]::ToInt32($Matches[1], 16) }
        if ($line -match 'Current DC Power Setting Index:\s*0x([0-9a-fA-F]+)') { $dc = [Convert]::ToInt32($Matches[1], 16) }
    }
    return [pscustomobject]@{ Ac = $ac; Dc = $dc }
}

function Get-RegistryValueOrNull([string]$Path, [string]$Name) {
    try {
        $item = Get-ItemProperty -Path $Path -Name $Name -ErrorAction Stop
        return $item.$Name
    } catch { return $null }
}

function Get-QuietState {
    # Reads back the settings the 4955 quiet task applied. No pictures taken.
    $video = Get-PowerSettingIndexes 'SUB_VIDEO' 'VIDEOIDLE'
    $standby = Get-PowerSettingIndexes 'SUB_SLEEP' 'STANDBYIDLE'
    $screenSaverActive = [string](Get-RegistryValueOrNull 'HKCU:\Control Panel\Desktop' 'ScreenSaveActive')
    $lockDisabled = Get-RegistryValueOrNull 'HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\System' 'DisableLockWorkstation'
    if ($null -eq $lockDisabled) {
        $lockDisabled = Get-RegistryValueOrNull 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\System' 'DisableLockWorkstation'
    }
    $toastEnabled = Get-RegistryValueOrNull 'HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\PushNotifications' 'ToastEnabled'
    $displayTimeoutsZero = ($video.Ac -eq 0 -and $video.Dc -eq 0)
    $sleepTimeoutsZero = ($standby.Ac -eq 0 -and $standby.Dc -eq 0)
    $screensaverOff = ($screenSaverActive -eq '0' -or [string]::IsNullOrEmpty($screenSaverActive))
    $workstationLockDisabled = ($lockDisabled -eq 1)
    $notificationBannersOff = ($toastEnabled -eq 0)
    $quiet = $displayTimeoutsZero -and $sleepTimeoutsZero -and $screensaverOff -and $workstationLockDisabled -and $notificationBannersOff
    return [pscustomobject]@{
        Quiet = [bool]$quiet
        DisplayTimeoutsZero = [bool]$displayTimeoutsZero
        SleepTimeoutsZero = [bool]$sleepTimeoutsZero
        ScreensaverOff = [bool]$screensaverOff
        WorkstationLockDisabled = [bool]$workstationLockDisabled
        NotificationBannersOff = [bool]$notificationBannersOff
    }
}

$results = @()

foreach ($app in $Apps) {
    Stop-AppProcesses $app.ProcessNames
}
Start-Sleep -Seconds 2

foreach ($app in $Apps) {
    $exe = Get-FirstExistingPath $app.ExeCandidates
    $installed = [bool]$exe
    $version = if ($installed) { Get-ExeVersion $exe } else { '-' }
    $windowFound = $false
    $windowTitle = $null
    $prompt = $null
    $account = $null
    if ($installed) {
        try {
            $launchArgs = @()
            if ($exe -like '*\Update.exe') {
                $launchArgs = $app.UpdateLaunchArguments
            } elseif ($app.PSObject.Properties.Name -contains 'LaunchArguments') {
                $launchArgs = $app.LaunchArguments
            }
            if ($launchArgs.Count -gt 0) {
                Start-Process -FilePath $exe -ArgumentList $launchArgs | Out-Null
            } else {
                Start-Process -FilePath $exe | Out-Null
            }
            $window = Wait-UiaWindow $app.ProcessNames $WindowWaitSeconds
            $windowFound = [bool]$window.Found
            $windowTitle = $window.Title
            if ($windowFound) {
                $uiText = Get-UiaTextForProcessNames $app.ProcessNames 20
                $prompt = Test-SignInPrompt $app $uiText
                $account = Get-RegexAccount $uiText $app.AccountPatterns
            }
        } catch {
            $windowFound = $false
        }
    }
    $signedIn = $installed -and $windowFound -and (-not $prompt) -and $account
    $reason = if (-not $installed) { 'missing' }
        elseif (-not $windowFound) { 'no-window' }
        elseif ($prompt) { "prompt: $prompt" }
        elseif (-not $account) { 'no-account-handle-in-accessibility-tree' }
        else { $null }
    $results += [pscustomobject]@{
        App = $app.Name
        Installed = [bool]$installed
        Version = $version
        Window = [bool]$windowFound
        WindowTitle = $windowTitle
        SignedIn = [bool]$signedIn
        Account = $account
        Reason = $reason
    }
    Stop-AppProcesses $app.ProcessNames
}

$oslExe = Get-FirstExistingPath $OslExeCandidates
$oslWindow = $false
$oslTitle = $null
if ($oslExe) {
    try {
        Stop-AppProcesses $OslProcessNames
        Start-Process -FilePath $oslExe | Out-Null
        $probe = Wait-UiaWindow $OslProcessNames $WindowWaitSeconds
        $oslWindow = [bool]$probe.Found
        $oslTitle = $probe.Title
        Stop-AppProcesses $OslProcessNames
    } catch {
        $oslWindow = $false
    }
}

$quietState = Get-QuietState

# Prove no picture was saved: this script links no capture API, and any image
# file under its state root, or freshly written to TEMP / the user's Desktop
# or Pictures since the script started, counts against readiness.
$imageExtensions = @('*.png', '*.jpg', '*.jpeg', '*.bmp', '*.gif', '*.tif', '*.tiff')
$imageFiles = @()
$imageFiles += @(Get-ChildItem -Path $StateRoot -Recurse -Include $imageExtensions -File -ErrorAction SilentlyContinue)
foreach ($freshRoot in @($env:TEMP, "$UserRoot\Desktop", "$UserRoot\Pictures")) {
    $imageFiles += @(Get-ChildItem -Path $freshRoot -Recurse -Include $imageExtensions -File -ErrorAction SilentlyContinue |
        Where-Object { $_.LastWriteTimeUtc -ge $ScriptStartUtc })
}
$imageCount = @($imageFiles).Count

$installedCount = @($results | Where-Object { $_.Installed }).Count
$signedInCount = @($results | Where-Object { $_.SignedIn }).Count
$windowCount = @($results | Where-Object { $_.Window }).Count

foreach ($row in $results) {
    $accountShown = if ($row.Account) { $row.Account } else { '-' }
    Write-Output ("VM-4958-APP {0} {1} installed={2} version={3} window={4} signedIn={5} account={6}" -f `
        $Machine, $row.App, $row.Installed, $row.Version, $row.Window, $row.SignedIn, $accountShown)
    if (-not $row.Installed) { Write-Output ("VM-4958-MISSING {0} {1}" -f $Machine, $row.App) }
    if ($row.Installed -and -not $row.Window) { Write-Output ("VM-4958-NO-WINDOW {0} {1}" -f $Machine, $row.App) }
    if ($row.SignedIn) {
        Write-Output ("VM-4958-ACCOUNT {0} {1} {2}" -f $Machine, $row.App, $row.Account)
    } else {
        Write-Output ("VM-4958-NOT-SIGNED-IN {0} {1} reason={2}" -f $Machine, $row.App, $row.Reason)
    }
}

Write-Output ("VM-4958-QUIET {0} {1}" -f $Machine, $quietState.Quiet.ToString().ToLowerInvariant())
Write-Output ("VM-4958-OSL-WINDOW {0} {1} exe={2} title={3}" -f $Machine, $oslWindow.ToString().ToLowerInvariant(), `
    $(if ($oslExe) { $oslExe } else { 'missing' }), $(if ($oslTitle) { $oslTitle } else { '-' }))
Write-Output ("VM-4958-INSTALLED {0} of 6" -f $installedCount)
Write-Output ("VM-4958-SIGNED-IN {0} of 6" -f $signedInCount)
Write-Output ("VM-4958-IMAGE-FILES {0}" -f $imageCount)

$summary = [pscustomobject]@{
    schema = 'readiness-check-4958/v1'
    machine = $Machine
    apps = $results
    quiet = $quietState
    oslExe = $oslExe
    oslWindow = [bool]$oslWindow
    oslWindowTitle = $oslTitle
    installedCount = $installedCount
    signedInCount = $signedInCount
    windowCount = $windowCount
    imageFileCount = $imageCount
    evidence = 'accessibility-tree-only'
}
$summary | ConvertTo-Json -Depth 6

$ready = ($installedCount -eq 6) -and ($signedInCount -eq 6) -and ($windowCount -eq 6) -and `
    $quietState.Quiet -and $oslWindow -and ($imageCount -eq 0)
if ($ready) {
    Write-Output ("VM-4958-READY {0}" -f $Machine)
    exit 0
}
Write-Output ("VM-4958-NOT-READY {0}" -f $Machine)
exit 1
