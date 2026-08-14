param(
  [ValidatePattern('^[A-Za-z0-9._-]{1,48}$')]
  [string]$RunId = ('task4951-' + [DateTime]::UtcNow.ToString('yyyyMMddHHmmss')),

  [ValidatePattern('^[A-Za-z0-9._-]{1,64}$')]
  [string]$PreferredUser = 'osltest',

  [ValidateRange(10, 180)]
  [int]$TimeoutSeconds = 60
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$root = Join-Path 'C:\ProgramData\OSL-QA\task-4951' $RunId
$probePath = Join-Path $root 'uia-root-walk.ps1'
$interactiveWrapperPath = Join-Path $root 'run-interactive.ps1'
$wrongWrapperPath = Join-Path $root 'run-wrong.ps1'
$interactiveResult = Join-Path $root 'interactive-result.json'
$wrongResult = Join-Path $root 'wrong-logon-result.json'
$rightsBackupPath = Join-Path $root 'user-rights-before.inf'
$rightsGrantPath = Join-Path $root 'user-rights-grant.inf'
$rightsDatabasePath = Join-Path $root 'user-rights.sdb'
$rightsRestoreDatabasePath = Join-Path $root 'user-rights-restore.sdb'
$interactiveTask = "OSL-QA-4951-Interactive-$RunId"
$wrongTask = "OSL-QA-4951-WrongLogon-$RunId"
$wrongLocalUser = 'osl4951b'
$wrongPrincipal = "$env:COMPUTERNAME\$wrongLocalUser"

function Find-InteractiveUser {
  $explorers = @(Get-CimInstance Win32_Process -Filter "Name = 'explorer.exe'" | Where-Object {
    [int]$_.SessionId -gt 0
  })
  if ($explorers.Count -eq 0) { throw 'no interactive Explorer session is available' }

  $candidates = @()
  foreach ($explorer in $explorers) {
    $owner = Invoke-CimMethod -InputObject $explorer -MethodName GetOwner
    if ($owner.ReturnValue -ne 0 -or -not $owner.User -or -not $owner.Domain) { continue }
    $candidates += [pscustomobject]@{
      User = [string]$owner.User
      Domain = [string]$owner.Domain
      SessionId = [int]$explorer.SessionId
      Principal = ('{0}\{1}' -f $owner.Domain, $owner.User)
    }
  }
  if ($candidates.Count -eq 0) { throw 'no owned interactive Explorer session is available' }

  $preferred = @($candidates | Where-Object { $_.User -ceq $PreferredUser })
  if ($preferred.Count -eq 1) { return $preferred[0] }
  if ($preferred.Count -gt 1) { throw "interactive $PreferredUser session is ambiguous" }
  if ($candidates.Count -ne 1) { throw 'interactive session owner is ambiguous' }
  return $candidates[0]
}

function Wait-ProbeResult([string]$Path, [string]$TaskName, [datetime]$Deadline) {
  while ([DateTime]::UtcNow -lt $Deadline) {
    if (Test-Path -LiteralPath $Path -PathType Leaf) {
      $result = Get-Content -LiteralPath $Path -Raw | ConvertFrom-Json -ErrorAction Stop
      if (-not $result.Ok) { throw "probe $TaskName failed: $($result.Error)" }
      return $result
    }
    Start-Sleep -Milliseconds 250
  }
  $info = Get-ScheduledTaskInfo -TaskName $TaskName -ErrorAction SilentlyContinue
  $last = if ($info) { [int]$info.LastTaskResult } else { -1 }
  throw "probe $TaskName timed out; LastTaskResult=$last"
}

function New-Task4951Password {
  $bytes = [byte[]]::new(16)
  $rng = [Security.Cryptography.RNGCryptoServiceProvider]::new()
  try {
    $rng.GetBytes($bytes)
  } finally {
    $rng.Dispose()
  }
  return 'Aa1!Task4951b-' + (([BitConverter]::ToString($bytes)) -replace '-', '')
}

function Invoke-NativeChecked([string]$Program, [string[]]$Arguments, [string]$Failure) {
  $previousPreference = $ErrorActionPreference
  $ErrorActionPreference = 'Continue'
  try {
    $output = & $Program @Arguments 2>&1
    $exitCode = $LASTEXITCODE
  } finally {
    $ErrorActionPreference = $previousPreference
  }
  if ($exitCode -ne 0) {
    throw "$Failure`: $($output -join ' ')"
  }
}

function Grant-BatchLogonRight([string]$Sid) {
  Invoke-NativeChecked 'secedit.exe' @('/export', '/cfg', $rightsBackupPath, '/areas', 'USER_RIGHTS', '/quiet') 'user-right export failed'
  $lines = @(Get-Content -LiteralPath $rightsBackupPath -ErrorAction Stop)
  $target = "*$Sid"
  $out = New-Object System.Collections.Generic.List[string]
  $inPrivilegeRights = $false
  $wroteRight = $false
  $insertedBeforeNextSection = $false

  foreach ($line in $lines) {
    if ($line -cmatch '^\[Privilege Rights\]') {
      $inPrivilegeRights = $true
      $out.Add($line)
      continue
    }
    if ($inPrivilegeRights -and $line -cmatch '^\[') {
      if (-not $wroteRight) {
        $out.Add("SeBatchLogonRight = $target")
        $wroteRight = $true
        $insertedBeforeNextSection = $true
      }
      $inPrivilegeRights = $false
    }
    if ($inPrivilegeRights -and $line -cmatch '^SeBatchLogonRight\s*=(.*)$') {
      $values = @($Matches[1].Split(',') | ForEach-Object { $_.Trim() } | Where-Object { $_ })
      if ($values -cnotcontains $target) { $values += $target }
      $out.Add('SeBatchLogonRight = ' + ($values -join ','))
      $wroteRight = $true
      continue
    }
    $out.Add($line)
  }
  if (-not $wroteRight) {
    if (-not ($lines -match '^\[Privilege Rights\]')) {
      $out.Add('[Privilege Rights]')
    } elseif (-not $insertedBeforeNextSection) {
      $out.Add("SeBatchLogonRight = $target")
      $wroteRight = $true
    }
    if (-not $wroteRight) { $out.Add("SeBatchLogonRight = $target") }
  }

  [IO.File]::WriteAllLines($rightsGrantPath, [string[]]$out, [Text.UnicodeEncoding]::new($false, $true))
  Invoke-NativeChecked 'secedit.exe' @('/configure', '/db', $rightsDatabasePath, '/cfg', $rightsGrantPath, '/areas', 'USER_RIGHTS', '/quiet') 'batch-logon grant failed'
}

function Restore-UserRights {
  if (Test-Path -LiteralPath $rightsBackupPath -PathType Leaf) {
    Invoke-NativeChecked 'secedit.exe' @('/configure', '/db', $rightsRestoreDatabasePath, '/cfg', $rightsBackupPath, '/areas', 'USER_RIGHTS', '/quiet') 'user-right restore failed'
  }
}

New-Item -ItemType Directory -Path $root -Force | Out-Null
& icacls $root /grant '*S-1-5-11:(OI)(CI)M' | Out-Null

$probe = @'
param(
  [Parameter(Mandatory = $true)]
  [string]$ResultPath,

  [Parameter(Mandatory = $true)]
  [ValidateSet('real', 'wrong')]
  [string]$Mode
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$session = [Diagnostics.Process]::GetCurrentProcess().SessionId
$line = "VM-4951-SESSION $session"
$temporary = "$ResultPath.tmp"
try {
  $walkTimedOut = $false
  if ($session -eq 0) {
    $job = Start-Job -ScriptBlock {
      Add-Type -AssemblyName UIAutomationClient
      Add-Type -AssemblyName UIAutomationTypes
      $rootElement = [Windows.Automation.AutomationElement]::RootElement
      if ($null -eq $rootElement) { return 0 }
      return [int]$rootElement.FindAll(
        [Windows.Automation.TreeScope]::Descendants,
        [Windows.Automation.Condition]::TrueCondition
      ).Count
    }
    try {
      if (Wait-Job -Job $job -Timeout 25) {
        $count = [int](Receive-Job -Job $job -ErrorAction Stop)
      } else {
        $walkTimedOut = $true
        $count = 0
        Stop-Job -Job $job -ErrorAction SilentlyContinue
      }
    } finally {
      Remove-Job -Job $job -Force -ErrorAction SilentlyContinue
    }
  } else {
    Add-Type -AssemblyName UIAutomationClient
    Add-Type -AssemblyName UIAutomationTypes
    $rootElement = [Windows.Automation.AutomationElement]::RootElement
    $elements = if ($null -eq $rootElement) {
      $null
    } else {
      $rootElement.FindAll(
        [Windows.Automation.TreeScope]::Descendants,
        [Windows.Automation.Condition]::TrueCondition
      )
    }
    $count = if ($null -eq $elements) { 0 } else { [int]$elements.Count }
  }
  $out = [ordered]@{
    Ok = $true
    Mode = $Mode
    Line = $line
    Session = [int]$session
    ElementCount = $count
    ExitCode = 0
    WalkTimedOut = $walkTimedOut
  }
} catch {
  $out = [ordered]@{
    Ok = $false
    Mode = $Mode
    Line = $line
    Session = [int]$session
    ElementCount = 0
    ExitCode = 1
    Error = $_.Exception.Message
  }
}
$json = $out | ConvertTo-Json -Compress
[IO.File]::WriteAllText($temporary, $json, [Text.UTF8Encoding]::new($false))
[IO.File]::Move($temporary, $ResultPath)
if ($out.Ok) { exit 0 } else { exit 1 }
'@
[IO.File]::WriteAllText($probePath, $probe, [Text.UTF8Encoding]::new($false))

$escapedProbePath = $probePath.Replace("'", "''")
$escapedInteractiveResult = $interactiveResult.Replace("'", "''")
$escapedWrongResult = $wrongResult.Replace("'", "''")
$interactiveWrapper = @"
`$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
& '$escapedProbePath' -ResultPath '$escapedInteractiveResult' -Mode real
exit `$LASTEXITCODE
"@
$wrongWrapper = @"
`$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
& '$escapedProbePath' -ResultPath '$escapedWrongResult' -Mode wrong
exit `$LASTEXITCODE
"@
[IO.File]::WriteAllText($interactiveWrapperPath, $interactiveWrapper, [Text.UTF8Encoding]::new($false))
[IO.File]::WriteAllText($wrongWrapperPath, $wrongWrapper, [Text.UTF8Encoding]::new($false))

$user = Find-InteractiveUser
$actionInteractive = New-ScheduledTaskAction -Execute 'powershell.exe' -Argument (
  '-NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "{0}"' -f
  $interactiveWrapperPath
)
$settings = New-ScheduledTaskSettingsSet -ExecutionTimeLimit ([TimeSpan]::FromMinutes(4)) -MultipleInstances IgnoreNew
$principalInteractive = New-ScheduledTaskPrincipal -UserId $user.Principal -LogonType Interactive -RunLevel Limited
$wrongPassword = New-Task4951Password

$wrongStillExists = $true
$wrongUserStillExists = $true
$rightsRestored = $false
try {
  if (Get-LocalUser -Name $wrongLocalUser -ErrorAction SilentlyContinue) {
    & net.exe user $wrongLocalUser /delete | Out-Null
  }
  Invoke-NativeChecked 'net.exe' @('user', $wrongLocalUser, $wrongPassword, '/add', '/Y', '/active:yes', '/expires:never') 'throwaway user creation failed'
  $wrongSid = (Get-LocalUser -Name $wrongLocalUser -ErrorAction Stop).SID.Value
  Grant-BatchLogonRight $wrongSid

  Register-ScheduledTask -TaskName $interactiveTask -Action $actionInteractive -Principal $principalInteractive -Settings $settings | Out-Null
  $wrongCommand = 'powershell.exe -NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "{0}"' -f $wrongWrapperPath
  $wrongStart = (Get-Date).AddMinutes(5).ToString('HH:mm')
  Invoke-NativeChecked 'schtasks.exe' @(
    '/Create', '/TN', $wrongTask,
    '/TR', $wrongCommand,
    '/SC', 'ONCE',
    '/ST', $wrongStart,
    '/RU', $wrongPrincipal,
    '/RP', $wrongPassword,
    '/F',
    '/RL', 'LIMITED'
  ) 'wrong-logon task registration failed'

  Invoke-NativeChecked 'schtasks.exe' @('/Run', '/TN', $wrongTask) 'wrong-logon task start failed'
  $wrong = Wait-ProbeResult $wrongResult $wrongTask ([DateTime]::UtcNow.AddSeconds($TimeoutSeconds))

  Start-ScheduledTask -TaskName $interactiveTask
  $real = Wait-ProbeResult $interactiveResult $interactiveTask ([DateTime]::UtcNow.AddSeconds($TimeoutSeconds))
} finally {
  Unregister-ScheduledTask -TaskName $wrongTask -Confirm:$false -ErrorAction SilentlyContinue
  Unregister-ScheduledTask -TaskName $interactiveTask -Confirm:$false -ErrorAction SilentlyContinue
  Restore-UserRights
  $rightsRestored = $true
  if (Get-LocalUser -Name $wrongLocalUser -ErrorAction SilentlyContinue) {
    & net.exe user $wrongLocalUser /delete | Out-Null
  }
  $wrongStillExists = [bool](Get-ScheduledTask -TaskName $wrongTask -ErrorAction SilentlyContinue)
  $wrongUserStillExists = [bool](Get-LocalUser -Name $wrongLocalUser -ErrorAction SilentlyContinue)
  $wrongPassword = $null
}

$wrongOk = $wrong.Line -ceq 'VM-4951-SESSION 0' -and [int]$wrong.ElementCount -eq 0 -and [int]$wrong.ExitCode -eq 0
$realOk = $real.Line -ceq 'VM-4951-SESSION 1' -and [int]$real.ElementCount -ge 200 -and [int]$real.ExitCode -eq 0
$cleanupOk = -not $wrongStillExists

[pscustomobject]@{
  Schema = 'task-4951-session0-runner-proof/v1'
  RunId = $RunId
  InteractiveUser = $user.Principal
  ExpectedInteractiveSession = [int]$user.SessionId
  WrongTaskName = $wrongTask
  InteractiveTaskName = $interactiveTask
  WrongTaskUser = $wrongPrincipal
  WrongLogonType = 'Password'
  InteractiveLogonType = 'Interactive'
  Wrong = $wrong
  Real = $real
  WrongTaskExistsAfterCleanup = $wrongStillExists
  WrongUserExistsAfterCleanup = $wrongUserStillExists
  UserRightsRestored = $rightsRestored
  Checks = [ordered]@{
    WrongPrintedSession0 = $wrong.Line
    WrongReturnedElements = [int]$wrong.ElementCount
    WrongExitCode = [int]$wrong.ExitCode
    RealPrintedSession1 = $real.Line
    RealReturnedElements = [int]$real.ElementCount
    RealExitCode = [int]$real.ExitCode
    ThrowawayTaskNoLongerExists = $cleanupOk
  }
  Ok = ($wrongOk -and $realOk -and $cleanupOk)
} | ConvertTo-Json -Depth 8

if ($wrongOk -and $realOk -and $cleanupOk) { exit 0 }
exit 1
