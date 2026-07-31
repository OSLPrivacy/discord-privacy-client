# OSL-QA-Capability: signal-store-install-arm/v1
param(
  [Parameter(Mandatory)][ValidatePattern('^[a-z0-9][a-z0-9-]{7,63}$')][string]$InvocationId,
  [Parameter(Mandatory)][ValidateRange(1,128)][int]$SessionId,
  [Parameter(Mandatory)][ValidateSet('osltest')][string]$WindowsUser,
  [Parameter(Mandatory)][ValidateSet('install','audit')][string]$Mode
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$ProductId = 'XP89119P9F2PCQ'
$StoreSource = 'msstore'
$root = Join-Path 'C:\ProgramData\OSL-QA\signal-store-install' $InvocationId
if (Test-Path -LiteralPath $root) { throw 'invocation ID already exists' }
[void](New-Item -ItemType Directory -Path $root)
$requestPath = Join-Path $root 'request.clixml'
$runnerPath = Join-Path $root 'runner.ps1'
$resultPath = Join-Path $root 'result.json'
$taskName = "OSL-QA-Signal-Store-$InvocationId"

function Get-ExactInteractiveOwner {
  $explorers = @(Get-CimInstance Win32_Process -Filter "Name = 'explorer.exe'" | Where-Object { [int]$_.SessionId -eq $SessionId })
  if ($explorers.Count -ne 1) { throw 'interactive Explorer session is unavailable or ambiguous' }
  $owner = Invoke-CimMethod -InputObject $explorers[0] -MethodName GetOwner
  if ($owner.ReturnValue -ne 0 -or $owner.User -cne $WindowsUser -or -not $owner.Domain) { throw 'interactive session owner mismatch' }
  return "$($owner.Domain)\$($owner.User)"
}

$interactiveUser = Get-ExactInteractiveOwner
$request = [pscustomobject]@{
  InvocationId = $InvocationId
  SessionId = $SessionId
  WindowsUser = $WindowsUser
  ProductId = $ProductId
  StoreSource = $StoreSource
  Mode = $Mode
  ResultPath = $resultPath
}
$request | Export-Clixml -LiteralPath $requestPath

$runner = @'
param([Parameter(Mandatory)][string]$RequestPath)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$r = Import-Clixml -LiteralPath $RequestPath
$temporary = "$($r.ResultPath).tmp"

function Write-Terminal([string]$Status, [hashtable]$Detail) {
  $receipt = @{
    SchemaVersion = 1
    Terminal = $true
    InvocationId = [string]$r.InvocationId
    Status = $Status
    ProductId = [string]$r.ProductId
    Source = [string]$r.StoreSource
    Detail = $Detail
  }
  [IO.File]::WriteAllText($temporary, ($receipt | ConvertTo-Json -Depth 8 -Compress), [Text.UTF8Encoding]::new($false))
  [IO.File]::Move($temporary, [string]$r.ResultPath)
}

try {
  if ($r.ProductId -cne 'XP89119P9F2PCQ' -or $r.StoreSource -cne 'msstore' -or $r.Mode -notin @('install','audit')) { throw 'fixed Store identity mismatch' }
  $explorers = @(Get-CimInstance Win32_Process -Filter "Name = 'explorer.exe'" | Where-Object { [int]$_.SessionId -eq [int]$r.SessionId })
  if ($explorers.Count -ne 1) { throw 'interactive Explorer session is unavailable or ambiguous' }
  $owner = Invoke-CimMethod -InputObject $explorers[0] -MethodName GetOwner
  if ($owner.ReturnValue -ne 0 -or $owner.User -cne [string]$r.WindowsUser -or -not $owner.Domain) { throw 'interactive session owner mismatch' }
  if ($r.Mode -ceq 'install') {
    if (@(Get-Process -Name Signal -ErrorAction SilentlyContinue).Count -ne 0) { throw 'Signal must remain closed before installation' }

    # Fresh Windows profiles may have App Installer registered before the
    # per-user WindowsApps execution alias is materialized. Resolve only the
    # exact registered Microsoft Store package instead of searching PATH.
    $appInstallers = @(Get-AppxPackage -Name 'Microsoft.DesktopAppInstaller' -ErrorAction Stop)
    if ($appInstallers.Count -ne 1) { throw 'App Installer package is unavailable or ambiguous' }
    $appInstaller = $appInstallers[0]
    if ($appInstaller.Status -cne 'Ok' -or $appInstaller.SignatureKind -cne 'Store' -or
        $appInstaller.Publisher -cne 'CN=Microsoft Corporation, O=Microsoft Corporation, L=Redmond, S=Washington, C=US') {
      throw 'App Installer package identity validation failed'
    }
    $winget = Join-Path ([string]$appInstaller.InstallLocation) 'winget.exe'
    if (-not (Test-Path -LiteralPath $winget -PathType Leaf)) { throw 'App Installer winget executable is unavailable' }
    $wingetSignature = Get-AuthenticodeSignature -LiteralPath $winget
    if ($wingetSignature.Status -cne 'Valid' -or -not $wingetSignature.SignerCertificate -or
        $wingetSignature.SignerCertificate.Subject -cne 'CN=Microsoft Corporation, O=Microsoft Corporation, L=Redmond, S=Washington, C=US') {
      throw 'App Installer winget signature validation failed'
    }
    $arguments = @(
      'install', '--id', 'XP89119P9F2PCQ', '--exact', '--source', 'msstore',
      '--accept-package-agreements', '--accept-source-agreements', '--silent', '--disable-interactivity'
    )
    $process = Start-Process -FilePath $winget -ArgumentList $arguments -PassThru -WindowStyle Hidden
    if (-not $process.WaitForExit(480000)) {
      Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue
      throw 'bounded Store installation timed out'
    }
    if ($process.ExitCode -ne 0) { throw "Store installation failed with exit code $($process.ExitCode)" }
  }

  # OSL's documented official classic-app candidate. Do not enumerate or inspect
  # any other user/profile paths; absence or ambiguity fails closed.
  $candidate = Join-Path ([Environment]::GetFolderPath('LocalApplicationData')) 'Programs\signal-desktop\Signal.exe'
  if (-not (Test-Path -LiteralPath $candidate -PathType Leaf)) { throw 'documented Signal executable candidate was not found' }
  $signature = Get-AuthenticodeSignature -LiteralPath $candidate
  if ($signature.Status -cne 'Valid' -or -not $signature.SignerCertificate -or -not $signature.SignerCertificate.Subject) { throw 'Signal executable Authenticode validation failed' }
  $item = Get-Item -LiteralPath $candidate
  $version = [string]$item.VersionInfo.FileVersion
  if ([string]::IsNullOrWhiteSpace($version)) { throw 'Signal executable has no file version' }
  $hash = (Get-FileHash -LiteralPath $candidate -Algorithm SHA256).Hash.ToLowerInvariant()
  $signalProcesses = @(Get-Process -Name Signal -ErrorAction SilentlyContinue)
  foreach ($signalProcess in $signalProcesses) {
    if (-not $signalProcess.Path -or [IO.Path]::GetFullPath($signalProcess.Path) -cne [IO.Path]::GetFullPath($candidate)) {
      throw 'running Signal process identity mismatch'
    }
  }
  $processState = if ($signalProcesses.Count -eq 0) { 'closed' } elseif ($r.Mode -ceq 'install') { 'installer-auto-launched-exact-candidate' } else { 'running-exact-candidate' }
  Write-Terminal 'completed' @{
    Version = $version
    Sha256 = $hash
    Publisher = [string]$signature.SignerCertificate.Subject
    PathClass = 'LocalAppDataProgramsSignalDesktop'
    ProcessState = $processState
  }
} catch {
  $code = switch -Regex ($_.Exception.Message) {
    'session|owner' { 'session-validation-failed'; break }
    'App Installer|winget' { 'winget-unavailable'; break }
    'timed out' { 'install-timeout'; break }
    'Store installation failed' { 'store-install-failed'; break }
    'Signal must remain closed' { 'preexisting-signal-process'; break }
    'process identity' { 'signal-process-identity-failed'; break }
    'candidate' { 'documented-candidate-missing'; break }
    'Authenticode' { 'signature-validation-failed'; break }
    default { 'install-audit-failed' }
  }
  Write-Terminal 'failed' @{ FailureCode = $code }
}
'@
[IO.File]::WriteAllText($runnerPath, $runner, [Text.UTF8Encoding]::new($false))
$action = New-ScheduledTaskAction -Execute 'powershell.exe' -Argument ('-NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "{0}" -RequestPath "{1}"' -f $runnerPath,$requestPath)
$principal = New-ScheduledTaskPrincipal -UserId $interactiveUser -LogonType Interactive -RunLevel Limited
$settings = New-ScheduledTaskSettingsSet -ExecutionTimeLimit ([TimeSpan]::FromMinutes(10)) -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries
Register-ScheduledTask -TaskName $taskName -Action $action -Principal $principal -Settings $settings | Out-Null
Start-ScheduledTask -TaskName $taskName
[pscustomobject]@{ SchemaVersion=1; Status='armed'; Terminal=$false; InvocationId=$InvocationId; ProductId=$ProductId; Source=$StoreSource; SessionId=$SessionId; Mode=$Mode } | ConvertTo-Json -Compress
