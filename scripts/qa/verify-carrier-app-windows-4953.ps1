<#
Launches each task 4953 carrier app once and proves a top-level UIA window title
appears inside 60 seconds. Run through scripts/qa/osl-vm-desktop-runner.ps1.
#>
param(
  [ValidateSet('Discord', 'Telegram Desktop', 'Signal Desktop', 'WhatsApp Desktop', 'Chrome', 'Firefox')]
  [string[]]$Apps = @('Discord', 'Telegram Desktop', 'Signal Desktop', 'WhatsApp Desktop', 'Chrome', 'Firefox'),

  [ValidateRange(5, 60)]
  [int]$WaitSeconds = 60
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes

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

function Resolve-CarrierLaunch($Name) {
  switch ($Name) {
    'Discord' {
      [pscustomobject]@{
        Path = (Get-HighestVersionPath (Join-Path $env:LOCALAPPDATA 'Discord\app-*') 'Discord.exe')
        Args = ''
        ProcessNames = @('Discord')
        Shell = $false
      }
    }
    'Telegram Desktop' {
      $path = @(
        (Join-Path $env:APPDATA 'Telegram Desktop\Telegram.exe'),
        (Join-Path $env:LOCALAPPDATA 'Programs\Telegram Desktop\Telegram.exe')
      ) | Where-Object { Test-Path -LiteralPath $_ -PathType Leaf } | Select-Object -First 1
      [pscustomobject]@{ Path = $path; Args = ''; ProcessNames = @('Telegram'); Shell = $false }
    }
    'Signal Desktop' {
      [pscustomobject]@{
        Path = (Join-Path $env:LOCALAPPDATA 'Programs\signal-desktop\Signal.exe')
        Args = ''
        ProcessNames = @('Signal')
        Shell = $false
      }
    }
    'WhatsApp Desktop' {
      [pscustomobject]@{
        Path = 'shell:AppsFolder\5319275A.WhatsAppDesktop_cv1g1gvanyjgm!App'
        Args = ''
        ProcessNames = @('WhatsApp.Root', 'WhatsApp')
        Shell = $true
      }
    }
    'Chrome' {
      [pscustomobject]@{
        Path = 'C:\Program Files\Google\Chrome\Application\chrome.exe'
        Args = '--new-window about:blank --no-first-run'
        ProcessNames = @('chrome')
        Shell = $false
      }
    }
    'Firefox' {
      [pscustomobject]@{
        Path = 'C:\Program Files\Mozilla Firefox\firefox.exe'
        Args = '-new-window about:blank'
        ProcessNames = @('firefox')
        Shell = $false
      }
    }
    default {
      throw "unknown carrier app $Name"
    }
  }
}

function Stop-CarrierProcesses($Launch) {
  foreach ($processName in $Launch.ProcessNames) {
    Get-Process -Name $processName -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
  }
}

function Get-TopLevelWindowForProcesses([string[]]$ProcessNames) {
  $processIds = @{}
  foreach ($processName in $ProcessNames) {
    Get-Process -Name $processName -ErrorAction SilentlyContinue | ForEach-Object {
      $processIds[[int]$_.Id] = $true
    }
  }
  if ($processIds.Count -eq 0) {
    return $null
  }

  $windows = [System.Windows.Automation.AutomationElement]::RootElement.FindAll(
    [System.Windows.Automation.TreeScope]::Children,
    [System.Windows.Automation.Condition]::TrueCondition
  )
  foreach ($window in $windows) {
    try {
      $current = $window.Current
      $title = [string]$current.Name
      $pid = [int]$current.ProcessId
      if ($processIds.ContainsKey($pid) -and $title.Trim().Length -gt 0) {
        return [pscustomobject]@{
          Title = $title
          ProcessId = $pid
          ControlType = [string]$current.ControlType.ProgrammaticName
        }
      }
    } catch {
      continue
    }
  }
  return $null
}

function Start-Carrier($Name, $Launch) {
  if ($Launch.Shell) {
    Start-Process -FilePath 'explorer.exe' -ArgumentList $Launch.Path | Out-Null
  } else {
    if (-not (Test-Path -LiteralPath $Launch.Path -PathType Leaf)) {
      throw "launch path missing for $Name"
    }
    if ($Launch.Args) {
      Start-Process -FilePath $Launch.Path -ArgumentList $Launch.Args | Out-Null
    } else {
      Start-Process -FilePath $Launch.Path | Out-Null
    }
  }
}

foreach ($app in $Apps) {
  $launch = Resolve-CarrierLaunch $app
  Stop-CarrierProcesses $launch
  Start-Sleep -Seconds 1
  $started = [DateTime]::UtcNow
  Start-Carrier $app $launch
  Write-Output "VM-4953-LAUNCH-STARTED $env:COMPUTERNAME $app"
  $deadline = $started.AddSeconds($WaitSeconds)
  $window = $null
  while ([DateTime]::UtcNow -lt $deadline -and -not $window) {
    Start-Sleep -Milliseconds 500
    $window = Get-TopLevelWindowForProcesses $launch.ProcessNames
  }
  if (-not $window) {
    Write-Output "VM-4953-NO-WINDOW $env:COMPUTERNAME $app"
    exit 1
  }
  $elapsed = [Math]::Round(([DateTime]::UtcNow - $started).TotalSeconds, 3)
  Write-Output "VM-4953-LAUNCH-WINDOW $env:COMPUTERNAME $app title=""$($window.Title)"" pid=$($window.ProcessId) control=$($window.ControlType) seconds=$elapsed"
}

Write-Output "VM-4953-LAUNCH-COUNT $env:COMPUTERNAME $(@($Apps).Count)"
