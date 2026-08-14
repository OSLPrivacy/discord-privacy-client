param(
    [ValidateSet('Audit','Break','Restore')][string]$Action = 'Audit',
    [string]$BreakApp = 'Firefox',
    [switch]$AfterDeallocate,
    [switch]$SummaryOnly
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$UserRoot = 'C:\Users\osladmin'
$StateRoot = 'C:\OSL\qa-4956'
$BaselinePath = Join-Path $StateRoot 'baseline.json'
$BreakRoot = Join-Path $StateRoot 'break-backups'
New-Item -ItemType Directory -Force -Path $StateRoot, $BreakRoot | Out-Null

Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes

function ConvertTo-Sha256Text([string]$Text) {
    $bytes = [Text.UTF8Encoding]::new($false).GetBytes($Text)
    $sha = [Security.Cryptography.SHA256]::Create()
    try { return ([BitConverter]::ToString($sha.ComputeHash($bytes)) -replace '-', '').ToLowerInvariant() }
    finally { $sha.Dispose() }
}

function Get-FirstExistingPath([string[]]$Candidates) {
    foreach ($candidate in $Candidates) {
        $item = Get-ChildItem -Path $candidate -ErrorAction SilentlyContinue | Select-Object -First 1
        if ($item) { return [string]$item.FullName }
    }
    return $null
}

function Get-FolderFingerprint([string]$Path) {
    if (-not (Test-Path -LiteralPath $Path)) { return 'missing' }
    $items = Get-ChildItem -LiteralPath $Path -Force -Recurse -ErrorAction SilentlyContinue |
        Where-Object { -not $_.PSIsContainer } |
        Sort-Object FullName |
        Select-Object -First 200
    $material = ($items | ForEach-Object {
        '{0}|{1}|{2}' -f $_.FullName.Substring($Path.Length), $_.Length, $_.LastWriteTimeUtc.Ticks
    }) -join "`n"
    return ConvertTo-Sha256Text $material
}

function Stop-AppProcesses([string[]]$Names) {
    foreach ($name in $Names) {
        Get-Process -Name $name -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
    }
}

function Start-App([pscustomobject]$App) {
    $exe = Get-FirstExistingPath $App.ExeCandidates
    if (-not $exe) { return $null }
    $args = @()
    if ($App.PSObject.Properties.Name -contains 'LaunchArguments' -and $App.LaunchArguments) {
        $args = $App.LaunchArguments
    }
    Start-Process -FilePath $exe -ArgumentList $args | Out-Null
    return $exe
}

function Get-UiaTextForProcessNames([string[]]$ProcessNames) {
    $deadline = [DateTime]::UtcNow.AddSeconds(25)
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

function Read-JsonFile([string]$Path) {
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { return $null }
    try { return Get-Content -LiteralPath $Path -Raw | ConvertFrom-Json } catch { return $null }
}

function Get-ChromeAccount([string]$Folder) {
    $localState = Read-JsonFile (Join-Path $Folder 'Local State')
    if ($localState -and $localState.profile -and $localState.profile.info_cache) {
        foreach ($profile in $localState.profile.info_cache.PSObject.Properties) {
            $entry = $profile.Value
            foreach ($field in @('user_name','gaia_name','name')) {
                if ($entry.PSObject.Properties.Name -contains $field -and -not [string]::IsNullOrWhiteSpace([string]$entry.$field)) {
                    return [string]$entry.$field
                }
            }
        }
    }
    return $null
}

function Get-FirefoxAccount([string]$Folder) {
    $profileRoot = Get-ChildItem -LiteralPath $Folder -Directory -ErrorAction SilentlyContinue | Select-Object -First 1
    if (-not $profileRoot) { return $null }
    $signed = Read-JsonFile (Join-Path $profileRoot.FullName 'signedInUser.json')
    if ($signed) {
        foreach ($field in @('email','uid','sessionTokenContext')) {
            if ($signed.PSObject.Properties.Name -contains $field -and -not [string]::IsNullOrWhiteSpace([string]$signed.$field)) {
                return [string]$signed.$field
            }
        }
    }
    return $null
}

function Get-RegexAccount([string]$Text, [string[]]$Patterns) {
    foreach ($pattern in $Patterns) {
        $m = [regex]::Match($Text, $pattern, [Text.RegularExpressions.RegexOptions]::IgnoreCase)
        if ($m.Success -and $m.Groups.Count -gt 1) { return $m.Groups[1].Value.Trim() }
    }
    return $null
}

$Apps = @(
    [pscustomobject]@{
        Name='Discord'
        ExeCandidates=@("$UserRoot\AppData\Local\Discord\Update.exe", "$UserRoot\AppData\Local\Discord\app-*\Discord.exe")
        LaunchArguments=@('--processStart','Discord.exe')
        ProcessNames=@('Discord')
        SignInFolder="$UserRoot\AppData\Roaming\discord"
        KeepSignedInControl='Log in form checkbox: This is a private computer'
        SignInPrompts=@('Log In','Email or Phone Number','Password','Forgot your password')
        AccountPatterns=@('(?m)^@?([A-Za-z0-9_.-]{2,32}#[0-9]{4})$','(?m)^@([A-Za-z0-9_.-]{2,32})$')
        SessionEnds='User logout, password reset, token revocation, or Discord remote session/device invalidation.'
    },
    [pscustomobject]@{
        Name='Telegram'
        ExeCandidates=@("$UserRoot\AppData\Roaming\Telegram Desktop\Telegram.exe", "$UserRoot\AppData\Local\Programs\Telegram Desktop\Telegram.exe", 'C:\Program Files\Telegram Desktop\Telegram.exe')
        LaunchArguments=@()
        ProcessNames=@('Telegram')
        SignInFolder="$UserRoot\AppData\Roaming\Telegram Desktop\tdata"
        KeepSignedInControl='Login flow checkbox: Keep me signed in'
        SignInPrompts=@('Start Messaging','Log in by phone Number','Please enter your phone number','QR code')
        AccountPatterns=@('(?m)^\+?[0-9][0-9 ()-]{6,}$','(?m)^@([A-Za-z0-9_]{5,32})$')
        SessionEnds='User logout, active-session termination from another Telegram client, password reset, or account/device revocation.'
    },
    [pscustomobject]@{
        Name='Signal'
        ExeCandidates=@("$UserRoot\AppData\Local\Programs\signal-desktop\Signal.exe")
        LaunchArguments=@()
        ProcessNames=@('Signal')
        SignInFolder="$UserRoot\AppData\Roaming\Signal"
        KeepSignedInControl='Linked-device flow: Link Device approval on the phone keeps this desktop linked'
        SignInPrompts=@('Link your Signal account','Scan the QR code','Set up Signal','Link Device')
        AccountPatterns=@('(?m)^\+?[0-9][0-9 ()-]{6,}$')
        SessionEnds='Signal Desktop is a linked device; it ends if the phone/account unlinks this desktop, the desktop is logged out, or Signal expires/revokes the link.'
    },
    [pscustomobject]@{
        Name='WhatsApp'
        ExeCandidates=@("$UserRoot\AppData\Local\WhatsApp\WhatsApp.exe", 'C:\Program Files\WindowsApps\*WhatsApp*\WhatsApp.exe')
        LaunchArguments=@()
        ProcessNames=@('WhatsApp')
        SignInFolder="$UserRoot\AppData\Local\Packages\5319275A.WhatsAppDesktop_cv1g1gvanyjgm\LocalState"
        KeepSignedInControl='Linked-device flow checkbox: Keep me signed in'
        SignInPrompts=@('Link with phone number','Link a device','Use WhatsApp on your phone','QR code')
        AccountPatterns=@('(?m)^\+?[0-9][0-9 ()-]{6,}$')
        SessionEnds='WhatsApp Desktop drops a linked device that has not seen its phone for about 14 days; user logout or phone-side unlink also ends it.'
    },
    [pscustomobject]@{
        Name='Chrome'
        ExeCandidates=@('C:\Program Files\Google\Chrome\Application\chrome.exe', 'C:\Program Files (x86)\Google\Chrome\Application\chrome.exe')
        LaunchArguments=@('--new-window','chrome://settings/people')
        ProcessNames=@('chrome')
        SignInFolder="$UserRoot\AppData\Local\Google\Chrome\User Data"
        KeepSignedInControl='Chrome sign-in confirmation button: Yes, I''m in'
        SignInPrompts=@('Sign in to Chrome','Turn on sync','Add account','Email or phone')
        AccountPatterns=@('[A-Z0-9._%+-]+@[A-Z0-9.-]+\.[A-Z]{2,}')
        SessionEnds='User sign-out, Google account/session revocation, browser profile deletion, or enterprise/browser policy disabling browser sign-in.'
    },
    [pscustomobject]@{
        Name='Firefox'
        ExeCandidates=@('C:\Program Files\Mozilla Firefox\firefox.exe', 'C:\Program Files (x86)\Mozilla Firefox\firefox.exe')
        LaunchArguments=@('-new-window','about:preferences#sync')
        ProcessNames=@('firefox')
        SignInFolder="$UserRoot\AppData\Roaming\Mozilla\Firefox\Profiles"
        KeepSignedInControl='Firefox Sync checkbox: Stay signed in'
        SignInPrompts=@('Sign in to sync','Enter your email','Firefox account','Mozilla account')
        AccountPatterns=@('[A-Z0-9._%+-]+@[A-Z0-9.-]+\.[A-Z]{2,}')
        SessionEnds='User sign-out, Mozilla account/session revocation, profile deletion, or sync credentials being invalidated.'
    }
)

function Get-AppAccount([pscustomobject]$App, [string]$UiText) {
    if ($App.Name -eq 'Chrome') {
        $account = Get-ChromeAccount $App.SignInFolder
        if ($account) { return $account }
    }
    if ($App.Name -eq 'Firefox') {
        $account = Get-FirefoxAccount $App.SignInFolder
        if ($account) { return $account }
    }
    $fromUi = Get-RegexAccount $UiText $App.AccountPatterns
    if ($fromUi) { return $fromUi }
    if ((Test-Path -LiteralPath $App.SignInFolder) -and ((Get-ChildItem -LiteralPath $App.SignInFolder -Force -Recurse -ErrorAction SilentlyContinue | Select-Object -First 1))) {
        return 'folder-fingerprint:' + (Get-FolderFingerprint $App.SignInFolder).Substring(0, 16)
    }
    return $null
}

function Test-SignInPrompt([pscustomobject]$App, [string]$UiText) {
    foreach ($prompt in $App.SignInPrompts) {
        if ($UiText -match [regex]::Escape($prompt)) { return $prompt }
    }
    return $null
}

function Invoke-Audit {
    $results = @()
    foreach ($app in $Apps) {
        Stop-AppProcesses $app.ProcessNames
    }
    Start-Sleep -Seconds 2

    foreach ($app in $Apps) {
        $exe = Start-App $app
        Start-Sleep -Seconds 4
        $uiText = Get-UiaTextForProcessNames $app.ProcessNames
        $prompt = Test-SignInPrompt $app $uiText
        $account = Get-AppAccount $app $uiText
        $folderExists = Test-Path -LiteralPath $app.SignInFolder
        $signedIn = $folderExists -and $account -and -not $prompt
        if ($prompt) { $signedIn = $false }
        $results += [pscustomobject]@{
            app=$app.Name
            signedIn=[bool]$signedIn
            account=$account
            asksAgain=$(if ($prompt) { $prompt } else { $null })
            executable=$exe
            keepSignedInControl=$app.KeepSignedInControl
            signInFolder=$app.SignInFolder
            signInFolderExists=[bool]$folderExists
            signInFolderFingerprint=Get-FolderFingerprint $app.SignInFolder
            sessionEnds=$app.SessionEnds
        }
    }

    foreach ($app in $Apps) {
        Stop-AppProcesses $app.ProcessNames
    }

    $baseline = $null
    if (Test-Path -LiteralPath $BaselinePath -PathType Leaf) {
        $baseline = Get-Content -LiteralPath $BaselinePath -Raw | ConvertFrom-Json
    } else {
        $baseline = [pscustomobject]@{
            createdUtc=[DateTime]::UtcNow.ToString('o')
            apps=$results
        }
        $baseline | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $BaselinePath -Encoding UTF8
    }

    $survived = 0
    $compared = @()
    foreach ($result in $results) {
        $base = @($baseline.apps | Where-Object { $_.app -eq $result.app } | Select-Object -First 1)
        $same = $false
        if ($base.Count -eq 1) {
            $same = [bool]$result.signedIn -and [string]$result.account -eq [string]$base[0].account
        }
        if ($same) { $survived += 1 }
        $compared += [pscustomobject]@{
            app=$result.app
            survived=[bool]$same
            account=$result.account
            baselineAccount=$(if ($base.Count -eq 1) { $base[0].account } else { $null })
            asksAgain=$result.asksAgain
            keepSignedInControl=$result.keepSignedInControl
            signInFolder=$result.signInFolder
            signInFolderExists=$result.signInFolderExists
            sessionEnds=$result.sessionEnds
        }
    }

    $phase = if ($AfterDeallocate) { 'after-deallocate-start' } else { 'after-app-restart' }
    $summary = [pscustomobject]@{
        schema='signin-survival-4956/v1'
        machine=$env:COMPUTERNAME
        action='Audit'
        phase=$phase
        survived=$survived
        total=6
        apps=$compared
        baselinePath=$BaselinePath
    }
    $summary | ConvertTo-Json -Depth 8
    Write-Output ("VM-4956-SURVIVED {0} of 6" -f $survived)
    foreach ($row in $compared | Where-Object { -not $_.survived }) {
        if ($row.asksAgain) {
            Write-Output ("VM-4956-ASKS-AGAIN {0} {1}" -f $row.app, $row.asksAgain)
        } else {
            Write-Output ("VM-4956-NOT-SURVIVED {0}" -f $row.app)
        }
    }
    if ($survived -eq 6) { exit 0 }
    exit 1
}

function Invoke-Break {
    $app = $Apps | Where-Object { $_.Name -eq $BreakApp } | Select-Object -First 1
    if (-not $app) { throw "unknown break app $BreakApp" }
    Stop-AppProcesses $app.ProcessNames
    $source = $app.SignInFolder
    if (-not (Test-Path -LiteralPath $source)) { throw "break source missing for ${BreakApp}: $source" }
    $target = Join-Path $BreakRoot $BreakApp
    if (Test-Path -LiteralPath $target) { Remove-Item -LiteralPath $target -Recurse -Force }
    Copy-Item -LiteralPath $source -Destination $target -Recurse -Force
    Remove-Item -LiteralPath $source -Recurse -Force
    Write-Output ("VM-4956-BREAK-REMOVED {0} {1}" -f $BreakApp, $source)
}

function Invoke-Restore {
    $app = $Apps | Where-Object { $_.Name -eq $BreakApp } | Select-Object -First 1
    if (-not $app) { throw "unknown restore app $BreakApp" }
    Stop-AppProcesses $app.ProcessNames
    $source = Join-Path $BreakRoot $BreakApp
    if (-not (Test-Path -LiteralPath $source)) { throw "restore backup missing for ${BreakApp}: $source" }
    if (Test-Path -LiteralPath $app.SignInFolder) { Remove-Item -LiteralPath $app.SignInFolder -Recurse -Force }
    $parent = Split-Path -Parent $app.SignInFolder
    New-Item -ItemType Directory -Force -Path $parent | Out-Null
    Copy-Item -LiteralPath $source -Destination $app.SignInFolder -Recurse -Force
    Write-Output ("VM-4956-RESTORED {0} {1}" -f $BreakApp, $app.SignInFolder)
}

switch ($Action) {
    'Audit' { Invoke-Audit }
    'Break' { Invoke-Break }
    'Restore' { Invoke-Restore }
}
