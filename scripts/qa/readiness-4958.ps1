<#
TASK 4958 fail-closed VM readiness check.

Run this payload through osl-vm-desktop-runner.ps1. All readiness claims are
derived from the Windows accessibility tree. Paths and process names are used
only to launch an app and associate its accessible top-level window; file or
profile contents are never accepted as evidence of install, version, account,
sign-in, quietness, or window state.
#>
param(
  [Parameter(Mandatory = $true)]
  [ValidatePattern('^[A-Za-z0-9][A-Za-z0-9-]{1,62}$')]
  [string]$Machine,

  [ValidateRange(5, 60)]
  [int]$WaitSeconds = 60,

  [string]$OslPath = ''
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
Add-Type -AssemblyName System.Windows.Forms

if ($env:COMPUTERNAME -cne $Machine) {
  Write-Output "VM-4958-NOT-READY machine=$Machine reason=machine-identity-mismatch actual=$env:COMPUTERNAME"
  exit 1
}

function Quote-Field([AllowNull()][string]$Value) {
  if ($null -eq $Value) { $Value = '' }
  return '"' + ($Value -replace '\\', '\\' -replace '"', '\"' -replace "`r|`n", ' ') + '"'
}

function Get-ProcessIds([string[]]$Names) {
  $ids = @{}
  foreach ($name in $Names) {
    foreach ($process in @(Get-Process -Name $name -ErrorAction SilentlyContinue)) {
      $ids[[int]$process.Id] = $true
    }
  }
  return $ids
}

function Get-AccessibleTopWindows([AllowNull()][hashtable]$ProcessIds) {
  $found = New-Object System.Collections.Generic.List[object]
  $windows = [Windows.Automation.AutomationElement]::RootElement.FindAll(
    [Windows.Automation.TreeScope]::Children,
    [Windows.Automation.Condition]::TrueCondition
  )
  foreach ($window in $windows) {
    try {
      $current = $window.Current
      $pid = [int]$current.ProcessId
      if ($ProcessIds -and -not $ProcessIds.ContainsKey($pid)) { continue }
      $title = [string]$current.Name
      if ([string]::IsNullOrWhiteSpace($title)) { continue }
      $found.Add([pscustomobject]@{
        Element = $window
        Title = $title.Trim()
        ProcessId = $pid
        ControlType = [string]$current.ControlType.ProgrammaticName
        ClassName = [string]$current.ClassName
      })
    } catch { }
  }
  return $found.ToArray()
}

function Wait-AccessibleWindow([string[]]$ProcessNames, [int]$Seconds) {
  $started = [DateTime]::UtcNow
  $deadline = $started.AddSeconds($Seconds)
  while ([DateTime]::UtcNow -lt $deadline) {
    $ids = Get-ProcessIds $ProcessNames
    if ($ids.Count -gt 0) {
      $windows = @(Get-AccessibleTopWindows $ids)
      if ($windows.Count -gt 0) {
        return [pscustomobject]@{
          Window = $windows[0]
          Seconds = [Math]::Round(([DateTime]::UtcNow - $started).TotalSeconds, 3)
        }
      }
    }
    Start-Sleep -Milliseconds 500
  }
  return $null
}

function Add-AccessiblePropertyText($Element, $Values) {
  foreach ($property in @(
    [Windows.Automation.AutomationElement]::NameProperty,
    [Windows.Automation.AutomationElement]::HelpTextProperty,
    [Windows.Automation.AutomationElement]::ItemStatusProperty,
    [Windows.Automation.AutomationElement]::AutomationIdProperty
  )) {
    try {
      $value = [string]$Element.GetCurrentPropertyValue($property, $true)
      if (-not [string]::IsNullOrWhiteSpace($value)) { [void]$Values.Add($value.Trim()) }
    } catch { }
  }
  try {
    $pattern = $Element.GetCurrentPattern([Windows.Automation.ValuePattern]::Pattern)
    $value = [string]$pattern.Current.Value
    if (-not [string]::IsNullOrWhiteSpace($value)) { [void]$Values.Add($value.Trim()) }
  } catch { }
  try {
    $legacy = $Element.GetCurrentPattern([Windows.Automation.LegacyIAccessiblePattern]::Pattern)
    foreach ($value in @($legacy.Current.Name, $legacy.Current.Value, $legacy.Current.Description)) {
      if (-not [string]::IsNullOrWhiteSpace([string]$value)) { [void]$Values.Add(([string]$value).Trim()) }
    }
  } catch { }
}

function Get-AccessibleTreeSnapshot($Root, [int]$MaximumNodes = 1800) {
  $values = New-Object 'System.Collections.Generic.HashSet[string]' ([StringComparer]::Ordinal)
  $queue = New-Object 'System.Collections.Generic.Queue[Windows.Automation.AutomationElement]'
  $queue.Enqueue($Root)
  $nodes = 0
  $walker = [Windows.Automation.TreeWalker]::RawViewWalker
  while ($queue.Count -gt 0 -and $nodes -lt $MaximumNodes) {
    $element = $queue.Dequeue()
    $nodes += 1
    Add-AccessiblePropertyText $element $values
    try {
      $child = $walker.GetFirstChild($element)
      while ($child) {
        $queue.Enqueue($child)
        $child = $walker.GetNextSibling($child)
      }
    } catch { }
  }
  return [pscustomobject]@{
    Text = (($values | ForEach-Object { $_ }) -join "`n")
    NodeCount = $nodes
  }
}

function Invoke-AccessibleNamedElement($Root, [string[]]$Names) {
  $queue = New-Object 'System.Collections.Generic.Queue[Windows.Automation.AutomationElement]'
  $queue.Enqueue($Root)
  $walker = [Windows.Automation.TreeWalker]::ControlViewWalker
  $seen = 0
  while ($queue.Count -gt 0 -and $seen -lt 1400) {
    $element = $queue.Dequeue()
    $seen += 1
    try {
      $name = [string]$element.Current.Name
      foreach ($wanted in $Names) {
        if ($name -notmatch $wanted) { continue }
        try {
          $invoke = $element.GetCurrentPattern([Windows.Automation.InvokePattern]::Pattern)
          $invoke.Invoke()
          return $true
        } catch { }
        try {
          $selection = $element.GetCurrentPattern([Windows.Automation.SelectionItemPattern]::Pattern)
          $selection.Select()
          return $true
        } catch { }
        try {
          $expand = $element.GetCurrentPattern([Windows.Automation.ExpandCollapsePattern]::Pattern)
          $expand.Expand()
          return $true
        } catch { }
        try {
          $legacy = $element.GetCurrentPattern([Windows.Automation.LegacyIAccessiblePattern]::Pattern)
          $legacy.DoDefaultAction()
          return $true
        } catch { }
      }
      $child = $walker.GetFirstChild($element)
      while ($child) {
        $queue.Enqueue($child)
        $child = $walker.GetNextSibling($child)
      }
    } catch { }
  }
  return $false
}

function Send-WindowKeys($Window, [string]$Keys) {
  try { $Window.Element.SetFocus() } catch { }
  Start-Sleep -Milliseconds 250
  [Windows.Forms.SendKeys]::SendWait($Keys)
  Start-Sleep -Seconds 2
}

function Navigate-AccountSurface([string]$Name, $Window) {
  switch ($Name) {
    'Discord' {
      Send-WindowKeys $Window '^,'
      [void](Invoke-AccessibleNamedElement $Window.Element @('^(My Account|Profiles)$'))
    }
    'Telegram Desktop' {
      [void](Invoke-AccessibleNamedElement $Window.Element @('(?i)^(Open navigation menu|Main menu|Menu)$'))
      Start-Sleep -Seconds 1
      [void](Invoke-AccessibleNamedElement $Window.Element @('(?i)^Settings$'))
      Start-Sleep -Seconds 1
      [void](Invoke-AccessibleNamedElement $Window.Element @('(?i)^(My Profile|Edit profile|Profile)$'))
    }
    'Signal Desktop' {
      Send-WindowKeys $Window '^,'
      [void](Invoke-AccessibleNamedElement $Window.Element @('(?i)^(Profile|Account)$'))
    }
    'WhatsApp Desktop' {
      [void](Invoke-AccessibleNamedElement $Window.Element @('(?i)^Settings$'))
      Start-Sleep -Seconds 1
      [void](Invoke-AccessibleNamedElement $Window.Element @('(?i)^Profile$'))
    }
  }
  Start-Sleep -Seconds 2
}

function Navigate-VersionSurface([string]$Name, $Window) {
  switch ($Name) {
    'Discord' {
      Send-WindowKeys $Window '^,'
      Send-WindowKeys $Window '{END}'
    }
    'Telegram Desktop' {
      [void](Invoke-AccessibleNamedElement $Window.Element @('(?i)^(Open navigation menu|Main menu|Menu)$'))
      Start-Sleep -Seconds 1
      [void](Invoke-AccessibleNamedElement $Window.Element @('(?i)^Settings$'))
      Send-WindowKeys $Window '{END}'
    }
    'Signal Desktop' {
      [void](Invoke-AccessibleNamedElement $Window.Element @('(?i)^Help$'))
      Start-Sleep -Seconds 1
      [void](Invoke-AccessibleNamedElement $Window.Element @('(?i)^About( Signal Desktop)?$'))
    }
    'WhatsApp Desktop' {
      [void](Invoke-AccessibleNamedElement $Window.Element @('(?i)^Settings$'))
      Start-Sleep -Seconds 1
      [void](Invoke-AccessibleNamedElement $Window.Element @('(?i)^Help$'))
    }
    'Chrome' { Send-WindowKeys $Window '^lchrome://version{ENTER}' }
    'Firefox' { Send-WindowKeys $Window '^labout:support{ENTER}' }
  }
  Start-Sleep -Seconds 2
}

function Get-RegexCapture([string]$Text, [string[]]$Patterns) {
  foreach ($pattern in $Patterns) {
    $match = [regex]::Match($Text, $pattern, [Text.RegularExpressions.RegexOptions]::IgnoreCase)
    if ($match.Success) {
      if ($match.Groups.Count -gt 1) { return $match.Groups[1].Value.Trim() }
      return $match.Value.Trim()
    }
  }
  return ''
}

function Test-AnyPattern([string]$Text, [string[]]$Patterns) {
  foreach ($pattern in $Patterns) {
    if ([regex]::IsMatch($Text, $pattern, [Text.RegularExpressions.RegexOptions]::IgnoreCase)) { return $true }
  }
  return $false
}

function Resolve-Launch($App) {
  $path = ''
  foreach ($candidate in $App.Paths) {
    $resolved = @(Get-ChildItem -Path $candidate -File -ErrorAction SilentlyContinue | Sort-Object FullName -Descending | Select-Object -First 1)
    if ($resolved.Count -gt 0) { $path = [string]$resolved[0].FullName; break }
  }
  return $path
}

function Start-ReadinessApp($App) {
  if ($App.Shell) {
    Start-Process -FilePath 'explorer.exe' -ArgumentList $App.ShellTarget | Out-Null
    return $true
  }
  $path = Resolve-Launch $App
  if (-not $path) { return $false }
  if ($App.Arguments.Count -gt 0) {
    Start-Process -FilePath $path -ArgumentList $App.Arguments | Out-Null
  } else {
    Start-Process -FilePath $path | Out-Null
  }
  return $true
}

$UserRoot = 'C:\Users\osladmin'
$Apps = @(
  [pscustomobject]@{
    Name='Discord'; Paths=@("$UserRoot\AppData\Local\Discord\app-*\Discord.exe"); Arguments=@(); Shell=$false; ShellTarget=''; ProcessNames=@('Discord')
    SignedOut=@('(?m)^Log In$','Email or Phone Number','Forgot your password','Create an account')
    Accounts=@('(?m)^@([A-Za-z0-9_.-]{2,32})$','(?m)^([A-Za-z0-9_.-]{2,32}#[0-9]{4})$')
    Versions=@('(?m)^(?:Stable|PTB|Canary)?\s*v?(\d+\.\d+(?:\.\d+){1,3})')
  },
  [pscustomobject]@{
    Name='Telegram Desktop'; Paths=@("$UserRoot\AppData\Roaming\Telegram Desktop\Telegram.exe", "$UserRoot\AppData\Local\Programs\Telegram Desktop\Telegram.exe", 'C:\Program Files\Telegram Desktop\Telegram.exe'); Arguments=@(); Shell=$false; ShellTarget=''; ProcessNames=@('Telegram')
    SignedOut=@('Start Messaging','Log in by phone','Please enter your phone number','QR code')
    Accounts=@('(?m)^@([A-Za-z0-9_]{5,32})$','(?m)^(\+?[0-9][0-9 ()-]{6,})$')
    Versions=@('(?im)(?:Telegram Desktop|version)\s*(?:x64\s*)?v?(\d+\.\d+(?:\.\d+){1,3})')
  },
  [pscustomobject]@{
    Name='Signal Desktop'; Paths=@("$UserRoot\AppData\Local\Programs\signal-desktop\Signal.exe"); Arguments=@(); Shell=$false; ShellTarget=''; ProcessNames=@('Signal')
    SignedOut=@('Link your Signal account','Scan the QR code','Set up Signal','Link Device')
    Accounts=@('(?m)^@([A-Za-z0-9_.-]{2,32})$','(?m)^(\+?[0-9][0-9 ()-]{6,})$')
    Versions=@('(?im)(?:Signal Desktop|version)\s*v?(\d+\.\d+(?:\.\d+){1,3})')
  },
  [pscustomobject]@{
    Name='WhatsApp Desktop'; Paths=@(); Arguments=@(); Shell=$true; ShellTarget='shell:AppsFolder\5319275A.WhatsAppDesktop_cv1g1gvanyjgm!App'; ProcessNames=@('WhatsApp.Root','WhatsApp')
    SignedOut=@('Link with phone number','Link a device','Use WhatsApp on your phone','QR code')
    Accounts=@('(?m)^@([A-Za-z0-9_.-]{2,32})$','(?m)^(\+?[0-9][0-9 ()-]{6,})$')
    Versions=@('(?im)(?:WhatsApp|version)\s*v?(\d+\.\d+(?:\.\d+){1,3})')
  },
  [pscustomobject]@{
    Name='Chrome'; Paths=@('C:\Program Files\Google\Chrome\Application\chrome.exe','C:\Program Files (x86)\Google\Chrome\Application\chrome.exe'); Arguments=@('--new-window','chrome://settings/people','--no-first-run'); Shell=$false; ShellTarget=''; ProcessNames=@('chrome')
    SignedOut=@('Sign in to Chrome','Turn on sync','Add account','Email or phone')
    Accounts=@('(?m)^([A-Z0-9._%+-]+@[A-Z0-9.-]+\.[A-Z]{2,})$')
    Versions=@('(?im)(?:Google Chrome|Chrome)\s+(\d+\.\d+\.\d+\.\d+)')
  },
  [pscustomobject]@{
    Name='Firefox'; Paths=@('C:\Program Files\Mozilla Firefox\firefox.exe','C:\Program Files (x86)\Mozilla Firefox\firefox.exe'); Arguments=@('-new-window','about:preferences#sync'); Shell=$false; ShellTarget=''; ProcessNames=@('firefox')
    SignedOut=@('Sign in to sync','Enter your email','Firefox account','Mozilla account')
    Accounts=@('(?m)^([A-Z0-9._%+-]+@[A-Z0-9.-]+\.[A-Z]{2,})$')
    Versions=@('(?im)(?:Firefox|version)\s+(\d+\.\d+(?:\.\d+){0,2})')
  }
)

$rows = New-Object System.Collections.Generic.List[object]
foreach ($app in $Apps) {
  $launched = Start-ReadinessApp $app
  $wait = Wait-AccessibleWindow $app.ProcessNames $WaitSeconds
  if (-not $wait) {
    $reason = if ($launched) { 'no-window-inside-60-seconds' } else { 'missing-or-not-launchable' }
    $rows.Add([pscustomobject]@{ App=$app.Name; Installed=$false; Version=''; Window=$false; SignedIn=$false; Account=''; Seconds=$WaitSeconds; Nodes=0; Reason=$reason })
    continue
  }

  $window = $wait.Window
  Navigate-AccountSurface $app.Name $window
  $accountTree = Get-AccessibleTreeSnapshot $window.Element
  $signedOut = Test-AnyPattern $accountTree.Text $app.SignedOut
  $account = Get-RegexCapture $accountTree.Text $app.Accounts

  Navigate-VersionSurface $app.Name $window
  $versionTree = Get-AccessibleTreeSnapshot $window.Element
  $version = Get-RegexCapture $versionTree.Text $app.Versions
  $signedIn = (-not $signedOut) -and (-not [string]::IsNullOrWhiteSpace($account))
  $reason = if (-not $version) { 'version-not-in-accessibility-tree' } elseif ($signedOut) { 'not-signed-in' } elseif (-not $account) { 'account-not-in-accessibility-tree' } else { 'ok' }
  $rows.Add([pscustomobject]@{
    App=$app.Name; Installed=$true; Version=$version; Window=$true; SignedIn=$signedIn; Account=$account
    Seconds=$wait.Seconds; Nodes=($accountTree.NodeCount + $versionTree.NodeCount); Reason=$reason
  })
}

function Resolve-OslLaunchPath([string]$Requested) {
  if ($Requested) { return $Requested }
  $candidates = @(
    'C:\OSL\osl-privacy-hub.exe',
    "$UserRoot\Desktop\OSL Privacy\OSL Privacy.exe",
    "$UserRoot\Desktop\OSL Privacy\osl-privacy-hub.exe"
  )
  foreach ($candidate in $candidates) {
    if (Test-Path -LiteralPath $candidate -PathType Leaf) { return $candidate }
  }
  $staged = @(Get-ChildItem -Path 'C:\OSL-VMQA\*\osl-privacy-hub.exe' -File -ErrorAction SilentlyContinue | Sort-Object LastWriteTimeUtc -Descending | Select-Object -First 1)
  if ($staged.Count -gt 0) { return [string]$staged[0].FullName }
  return ''
}

$oslLaunchPath = Resolve-OslLaunchPath $OslPath
$oslStarted = $false
if ($oslLaunchPath -and (Test-Path -LiteralPath $oslLaunchPath -PathType Leaf)) {
  Start-Process -FilePath $oslLaunchPath | Out-Null
  $oslStarted = $true
}
$oslWait = Wait-AccessibleWindow @('osl-privacy-hub','OSL Privacy','discord-privacy-client') $WaitSeconds
$oslWindow = $false
$oslTitle = ''
$oslSeconds = $WaitSeconds
$oslNodes = 0
if ($oslWait) {
  $oslSnapshot = Get-AccessibleTreeSnapshot $oslWait.Window.Element
  $oslWindow = $oslSnapshot.NodeCount -gt 0
  $oslTitle = $oslWait.Window.Title
  $oslSeconds = $oslWait.Seconds
  $oslNodes = $oslSnapshot.NodeCount
}
$oslReason = if ($oslWindow) { 'ok' } elseif ($oslStarted) { 'no-window-inside-60-seconds' } else { 'missing-or-not-launchable' }

$interruptions = New-Object System.Collections.Generic.List[string]
$allTop = @(Get-AccessibleTopWindows $null)
$interruptionPatterns = @(
  '^Windows Security$', '^User Account Control$', '^Restart required$',
  '^Let.s finish setting up your device$', '^Choose privacy settings for your device$',
  '^Get even more out of Windows$', '^Microsoft Windows.*not responding$'
)
foreach ($top in $allTop) {
  foreach ($pattern in $interruptionPatterns) {
    if ($top.Title -match $pattern) { [void]$interruptions.Add($top.Title); break }
  }
}
$quiet = $interruptions.Count -eq 0

foreach ($row in $rows) {
  Write-Output ("VM-4958-APP machine={0} app={1} installed={2} version={3} window={4} signedIn={5} account={6} seconds={7} accessibilityNodes={8} reason={9}" -f
    $Machine, (Quote-Field $row.App), ([string]$row.Installed).ToLowerInvariant(), (Quote-Field $row.Version),
    ([string]$row.Window).ToLowerInvariant(), ([string]$row.SignedIn).ToLowerInvariant(), (Quote-Field $row.Account),
    $row.Seconds, $row.Nodes, (Quote-Field $row.Reason))
}

$installedCount = @($rows | Where-Object { $_.Installed -and $_.Window }).Count
$signedInCount = @($rows | Where-Object { $_.SignedIn -and $_.Account }).Count
$versionCount = @($rows | Where-Object { $_.Version }).Count
$notSignedIn = @($rows | Where-Object { -not $_.SignedIn } | ForEach-Object { $_.App })
Write-Output "VM-4958-INSTALLED $installedCount of 6"
Write-Output "VM-4958-VERSIONS $versionCount of 6"
Write-Output "VM-4958-SIGNED-IN $signedInCount of 6"
Write-Output ("VM-4958-NOT-SIGNED-IN machine={0} apps={1}" -f $Machine, (Quote-Field ($notSignedIn -join ',')))
Write-Output ("VM-4958-QUIET machine={0} quiet={1} interruptions={2}" -f $Machine, ([string]$quiet).ToLowerInvariant(), (Quote-Field ($interruptions -join ',')))
Write-Output ("VM-4958-OSL-WINDOW machine={0} window={1} title={2} seconds={3} accessibilityNodes={4} reason={5}" -f
  $Machine, ([string]$oslWindow).ToLowerInvariant(), (Quote-Field $oslTitle), $oslSeconds, $oslNodes, (Quote-Field $oslReason))
Write-Output 'VM-4958-IMAGE-FILES 0'

$ready = $installedCount -eq 6 -and $versionCount -eq 6 -and $signedInCount -eq 6 -and $quiet -and $oslWindow
if ($ready) {
  Write-Output "VM-4958-READY machine=$Machine"
  exit 0
}
Write-Output "VM-4958-NOT-READY machine=$Machine"
exit 1
