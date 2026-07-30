$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$telegramPath = 'C:\Users\osltest\AppData\Roaming\Telegram Desktop\Telegram.exe'
$taskRoot = 'C:\ProgramData\OSL-QA\telegram-login-inspection'
$scriptPath = Join-Path $taskRoot 'inspect.ps1'
$resultPath = Join-Path $taskRoot 'result.json'
$taskName = 'OSL-Telegram-QA-Inspect-Login'

if (-not (Test-Path -LiteralPath $telegramPath -PathType Leaf)) { throw 'exact Telegram executable is absent' }
$signature = Get-AuthenticodeSignature -LiteralPath $telegramPath
if ($signature.Status -ne [Management.Automation.SignatureStatus]::Valid) { throw 'Telegram signature is invalid' }
$signer = [string]$signature.SignerCertificate.Subject
if ($signer -notmatch '(?:^|,\s*)CN=(?:Telegram FZ-LLC|Telegram Messenger LLP)(?:,|$)') {
  throw 'Telegram signer is not allowlisted'
}
$explorers = @(Get-CimInstance Win32_Process -Filter "Name = 'explorer.exe'")
if ($explorers.Count -ne 1) { throw 'interactive Explorer session is unavailable or ambiguous' }
$sessionId = [int]$explorers[0].SessionId
$owner = Invoke-CimMethod -InputObject $explorers[0] -MethodName GetOwner
if ($owner.ReturnValue -ne 0 -or $owner.User -cne 'osltest' -or -not $owner.Domain) {
  throw 'interactive session owner is not exact osltest identity'
}
$interactiveUser = "$($owner.Domain)\$($owner.User)"

[void](New-Item -ItemType Directory -Path $taskRoot -Force)
if (Test-Path -LiteralPath $resultPath) { Remove-Item -LiteralPath $resultPath -Force }
$escapedTelegramPath = $telegramPath.Replace("'", "''")
$escapedResultPath = $resultPath.Replace("'", "''")
$innerScript = @"
`$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
function Get-TelegramA11yControlTypeName {
  param([Windows.Automation.AutomationElement]`$Element)
  `$controlType = `$Element.Current.ControlType
  if (-not `$controlType) { return 'Unknown' }
  return ([string]`$controlType.ProgrammaticName).Replace('ControlType.', '')
}
function Test-TelegramA11yTextExposure {
  param([Windows.Automation.AutomationElement]`$Element)
  if (([string]`$Element.Current.Name).Trim().Length -gt 0) { return `$true }
  try {
    `$valuePattern = `$Element.GetCurrentPattern([Windows.Automation.ValuePattern]::Pattern)
    if (`$valuePattern -and ([string]`$valuePattern.Current.Value).Trim().Length -gt 0) { return `$true }
  } catch {
  }
  return `$false
}
function Invoke-TelegramA11yRowProbe {
  param(
    [Windows.Automation.AutomationElement[]]`$VisibleElements,
    [bool]`$IsLoginPhase
  )
  `$probeClock = [Diagnostics.Stopwatch]::StartNew()
  `$rowControlTypes = @('DataItem', 'ListItem', 'TreeItem')
  `$candidateRows = @(`$VisibleElements | Where-Object {
    `$controlTypeName = Get-TelegramA11yControlTypeName `$_
    `$bounds = `$_.Current.BoundingRectangle
    (`$rowControlTypes -contains `$controlTypeName) -and
      `$bounds.Width -ge 120 -and
      `$bounds.Height -ge 12 -and
      `$bounds.Height -le 180
  })
  `$textExposedRows = @(`$candidateRows | Where-Object { Test-TelegramA11yTextExposure `$_ })
  `$controlTypeCounts = @{}
  `$candidateRows | ForEach-Object {
    `$key = Get-TelegramA11yControlTypeName `$_
    if (`$controlTypeCounts.ContainsKey(`$key)) {
      `$controlTypeCounts[`$key] = [int]`$controlTypeCounts[`$key] + 1
    } else {
      `$controlTypeCounts[`$key] = 1
    }
  }
  `$hasStableRows = `$candidateRows.Count -ge 2 -and `$textExposedRows.Count -ge 2
  `$exposure = if (`$hasStableRows) {
    'stable-accessible-rows'
  } elseif (`$candidateRows.Count -gt 0) {
    'geometry-only-or-unnamed-rows'
  } else {
    'no-accessible-rows'
  }
  `$verdict = if (`$hasStableRows) {
    'supported'
  } elseif (`$IsLoginPhase) {
    'unsupported'
  } else {
    'externally blocked'
  }
  `$probeClock.Stop()
  return @{
    Probe = 'TelegramA11yRowProbe'
    Status = 'bench-ran'
    Verdict = `$verdict
    Exposure = `$exposure
    CandidateRowCount = [int]`$candidateRows.Count
    TextExposedRowCount = [int]`$textExposedRows.Count
    ControlTypeCounts = `$controlTypeCounts
    DurationMs = [int]`$probeClock.ElapsedMilliseconds
  }
}
try {
  `$windowProcesses = @(Get-CimInstance Win32_Process -Filter "Name = 'Telegram.exe'" | Where-Object {
    [int]`$_.SessionId -eq $sessionId -and `$_.ExecutablePath -and
    [IO.Path]::GetFullPath([string]`$_.ExecutablePath) -ceq '$escapedTelegramPath'
  } | ForEach-Object { Get-Process -Id ([int]`$_.ProcessId) -ErrorAction Stop } | Where-Object {
    `$_.MainWindowHandle -ne 0
  })
  if (`$windowProcesses.Count -ne 1) { throw 'exact Telegram login window is unavailable or ambiguous' }
  `$root = [Windows.Automation.AutomationElement]::FromHandle(`$windowProcesses[0].MainWindowHandle)
  if (-not `$root) { throw 'Telegram automation root is unavailable' }
  `$rootTitle = [string]`$root.Current.Name
  `$rootClass = [string]`$root.Current.ClassName
  `$visible = @(`$root.FindAll([Windows.Automation.TreeScope]::Descendants, [Windows.Automation.Condition]::TrueCondition) |
    Where-Object { -not `$_.Current.IsOffscreen -and -not `$_.Current.BoundingRectangle.IsEmpty })
  `$names = @(`$visible | ForEach-Object { [string]`$_.Current.Name } | Where-Object { `$_ })
  `$hasStart = @(`$names | Where-Object { `$_ -match '(?i)^start messaging`$' }).Count -eq 1
  `$hasPhone = @(`$names | Where-Object { `$_ -match '(?i)^(log in by phone number|phone number)`$' }).Count -ge 1
  `$hasQr = @(`$names | Where-Object { `$_ -match '(?i)(log in by qr code|scan.*qr|quick log in using qr)' }).Count -ge 1
  `$phase = if (`$hasStart -or `$hasPhone -or `$hasQr) { 'freshLogin' } else { 'unrecognized' }
  `$rowProbe = Invoke-TelegramA11yRowProbe -VisibleElements `$visible -IsLoginPhase (`$phase -eq 'freshLogin')
  `$result = @{
    Status = 'inspected-without-focus'
    Phase = `$phase
    TelegramPid = [int]`$windowProcesses[0].Id
    SessionId = [int]`$windowProcesses[0].SessionId
    WindowHandle = [int64]`$windowProcesses[0].MainWindowHandle
    VisibleElementCount = `$visible.Count
    HasStartMessaging = `$hasStart
    HasPhoneLogin = `$hasPhone
    HasQrLogin = `$hasQr
    A11yRowProbe = `$rowProbe
    WindowClass = `$rootClass
    TitleEqualsTelegram = `$rootTitle -ceq 'Telegram'
    TitleLength = `$rootTitle.Length
  }
} catch {
  `$detail = [string]`$_.Exception.Message
  if (`$detail.Length -gt 160) { `$detail = `$detail.Substring(0, 160) }
  `$result = @{ Status = 'failed'; Detail = `$detail }
}
[IO.File]::WriteAllText('$escapedResultPath', (`$result | ConvertTo-Json -Depth 6 -Compress), [Text.UTF8Encoding]::new(`$false))
"@
[IO.File]::WriteAllText($scriptPath, $innerScript, [Text.UTF8Encoding]::new($false))

$action = New-ScheduledTaskAction -Execute 'powershell.exe' -Argument "-NoProfile -NonInteractive -ExecutionPolicy Bypass -File `"$scriptPath`""
$principal = New-ScheduledTaskPrincipal -UserId $interactiveUser -LogonType Interactive -RunLevel Limited
$settings = New-ScheduledTaskSettingsSet -ExecutionTimeLimit ([TimeSpan]::FromMinutes(1)) -MultipleInstances IgnoreNew
Register-ScheduledTask -TaskName $taskName -Action $action -Principal $principal -Settings $settings -Force | Out-Null
try {
  Start-ScheduledTask -TaskName $taskName
  $deadline = [DateTime]::UtcNow.AddSeconds(30)
  while (-not (Test-Path -LiteralPath $resultPath -PathType Leaf) -and [DateTime]::UtcNow -lt $deadline) {
    Start-Sleep -Milliseconds 100
  }
  if (-not (Test-Path -LiteralPath $resultPath -PathType Leaf)) { throw 'Telegram login inspection timed out' }
  Get-Content -LiteralPath $resultPath -Raw
} finally {
  Unregister-ScheduledTask -TaskName $taskName -Confirm:$false -ErrorAction SilentlyContinue
}
