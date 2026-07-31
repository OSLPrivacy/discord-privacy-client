param(
  [Parameter(Mandatory = $true)]
  [ValidatePattern('^[a-z0-9][a-z0-9-]{7,63}$')]
  [string]$InvocationId,

  [Parameter(Mandatory = $true)]
  [ValidateRange(1, 65535)]
  [int]$SessionId
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$root = 'C:\ProgramData\OSL-QA\whatsapp-login-v1'
$invocationRoot = Join-Path $root $InvocationId
$requestPath = Join-Path $invocationRoot 'request.clixml'
$wrapperPath = Join-Path $invocationRoot 'run-interactive.ps1'
$resultPath = Join-Path $invocationRoot 'result.json'
$taskName = "OSL-QA-WhatsApp-Login-$InvocationId"

$explorers = @()
foreach ($explorer in @(Get-CimInstance Win32_Process -Filter "Name = 'explorer.exe'")) {
  $owner = Invoke-CimMethod -InputObject $explorer -MethodName GetOwner
  if ($owner.ReturnValue -eq 0 -and $owner.User -ceq 'osltest' -and $owner.Domain) {
    $explorers += [pscustomobject]@{ Process = $explorer; Owner = $owner }
  }
}
if ($explorers.Count -ne 1 -or [int]$explorers[0].Process.SessionId -ne $SessionId) {
  throw 'exact osltest interactive session is unavailable or ambiguous'
}
$interactiveUser = "$($explorers[0].Owner.Domain)\$($explorers[0].Owner.User)"

if (Test-Path -LiteralPath $invocationRoot) {
  if (-not (Test-Path -LiteralPath $requestPath -PathType Leaf) -or
      -not (Test-Path -LiteralPath $wrapperPath -PathType Leaf)) {
    throw 'existing invocation is incomplete'
  }
  $existing = Import-Clixml -LiteralPath $requestPath
  $wrapperHash = (Get-FileHash -LiteralPath $wrapperPath -Algorithm SHA256).Hash.ToLowerInvariant()
  if ($existing.InvocationId -cne $InvocationId -or [int]$existing.SessionId -ne $SessionId -or
      $existing.InteractiveUser -cne $interactiveUser -or $existing.WrapperSha256 -cne $wrapperHash) {
    throw 'existing invocation identity mismatch'
  }
  [pscustomobject]@{
    InvocationId = $InvocationId
    Status = if (Test-Path -LiteralPath $resultPath -PathType Leaf) { 'alreadyCompleted' } else { 'alreadyArmed' }
    SessionId = $SessionId
    WrapperSha256 = $wrapperHash
  } | ConvertTo-Json -Compress
  exit 0
}

[void](New-Item -ItemType Directory -Path $invocationRoot -Force)
$request = [ordered]@{
  InvocationId = $InvocationId
  SessionId = $SessionId
  InteractiveUser = $interactiveUser
  ResultPath = $resultPath
}
$requestTemporary = "$requestPath.tmp"
$request | Export-Clixml -LiteralPath $requestTemporary -Depth 3
[IO.File]::Move($requestTemporary, $requestPath)

$escapedRequestPath = $requestPath.Replace("'", "''")
$wrapper = @'
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$packageName = '5319275A.WhatsAppDesktop'
$packageFamily = '5319275A.WhatsAppDesktop_cv1g1gvanyjgm'
$publisherId = 'cv1g1gvanyjgm'
$appUserModelId = '5319275A.WhatsAppDesktop_cv1g1gvanyjgm!App'
$request = Import-Clixml -LiteralPath '__REQUEST_PATH__'
$resultTemporary = "$($request.ResultPath).tmp"

Add-Type -TypeDefinition @"
using System;
using System.Text;
using System.Runtime.InteropServices;
public static class OslWhatsAppLoginWindow {
  [DllImport("user32.dll", CharSet = CharSet.Unicode)]
  public static extern int GetClassName(IntPtr window, StringBuilder value, int capacity);
}
"@

function Get-ExactPackage {
  $all = @(Get-AppxPackage -Name $packageName -PackageTypeFilter Main -ErrorAction SilentlyContinue)
  $matching = @($all | Where-Object {
    $_.Name -ceq $packageName -and $_.PackageFamilyName -ceq $packageFamily -and
    $_.PublisherId -ceq $publisherId -and -not $_.IsFramework -and -not $_.IsResourcePackage -and
    $_.PackageFullName -cmatch '^5319275A[.]WhatsAppDesktop_[0-9]+[.][0-9]+[.][0-9]+[.][0-9]+_(x64|x86|arm64|neutral)__cv1g1gvanyjgm$'
  })
  if ($all.Count -ne 1 -or $matching.Count -ne 1) { throw 'exact WhatsApp Store package is unavailable or ambiguous' }
  $location = [IO.Path]::GetFullPath([string]$matching[0].InstallLocation).TrimEnd('\')
  $programFiles = [IO.Path]::GetFullPath([Environment]::GetFolderPath('ProgramFiles')).TrimEnd('\')
  if ([IO.Directory]::GetParent($location).FullName -ine "$programFiles\WindowsApps" -or
      -not $location.StartsWith("$programFiles\WindowsApps\5319275A.WhatsAppDesktop_", [StringComparison]::OrdinalIgnoreCase)) {
    throw 'WhatsApp package is outside protected WindowsApps'
  }
  return [pscustomobject]@{ Package = $matching[0]; Location = $location }
}

function Get-ExactWindow([string]$location) {
  $processes = @(Get-Process -Name 'WhatsApp.Root' -ErrorAction SilentlyContinue | Where-Object {
    $_.SessionId -eq [int]$request.SessionId -and $_.Path -and
    [IO.Path]::GetFullPath($_.Path).StartsWith("$location\", [StringComparison]::OrdinalIgnoreCase)
  })
  $windows = @()
  foreach ($process in $processes) {
    if ($process.MainWindowHandle -eq 0) { continue }
    $className = [Text.StringBuilder]::new(128)
    [void][OslWhatsAppLoginWindow]::GetClassName($process.MainWindowHandle, $className, $className.Capacity)
    if ($className.ToString() -ceq 'WinUIDesktopWin32WindowClass' -and $process.MainWindowTitle -ceq 'WhatsApp') {
      $windows += $process
    }
  }
  if ($windows.Count -gt 1) { throw 'exact WhatsApp main window is ambiguous' }
  return $windows
}

$started = [DateTime]::UtcNow.ToString('o')
try {
  if ([Security.Principal.WindowsIdentity]::GetCurrent().Name -cne $request.InteractiveUser -or
      [Diagnostics.Process]::GetCurrentProcess().SessionId -ne [int]$request.SessionId) {
    throw 'interactive runner session identity mismatch'
  }
  if ((Get-FileHash -LiteralPath $PSCommandPath -Algorithm SHA256).Hash.ToLowerInvariant() -cne $request.WrapperSha256) {
    throw 'interactive runner hash mismatch'
  }
  $package = Get-ExactPackage
  $windows = @(Get-ExactWindow $package.Location)
  $launched = $false
  if ($windows.Count -eq 0) {
    $launched = $true
    Start-Process -FilePath 'explorer.exe' -ArgumentList "shell:AppsFolder\$appUserModelId"
    $deadline = [DateTime]::UtcNow.AddSeconds(60)
    do {
      Start-Sleep -Milliseconds 250
      $windows = @(Get-ExactWindow $package.Location)
      if ($windows.Count -eq 1) { break }
    } while ([DateTime]::UtcNow -lt $deadline)
  }
  if ($windows.Count -ne 1) { throw 'exact WhatsApp main window did not become ready' }
  $terminal = [ordered]@{
    InvocationId = $request.InvocationId; Terminal = $true; Status = 'readyForUserLogin'
    StartedUtc = $started; CompletedUtc = [DateTime]::UtcNow.ToString('o')
    PackageFamily = $packageFamily; Version = [string]$package.Package.Version
    SessionId = [int]$request.SessionId; ProcessId = [int]$windows[0].Id
    ExactWindowVerified = $true; ExistingWindowReused = -not $launched
    AppLaunched = $launched; BrowserFallbackUsed = $false; ProfileRead = $false
    LinkStateTouched = $false; PrivateApiUsed = $false; ProcessesTerminated = 0
  }
} catch {
  $terminal = [ordered]@{
    InvocationId = $request.InvocationId; Terminal = $true; Status = 'failedClosed'
    StartedUtc = $started; CompletedUtc = [DateTime]::UtcNow.ToString('o')
    PackageFamily = $packageFamily; ExactWindowVerified = $false
    BrowserFallbackUsed = $false; ProfileRead = $false; LinkStateTouched = $false
    PrivateApiUsed = $false; ProcessesTerminated = 0; Error = 'whatsapp-login-launch-failed-closed'
    ExceptionType = $_.Exception.GetType().Name
  }
}
$json = $terminal | ConvertTo-Json -Depth 5 -Compress
[IO.File]::WriteAllText($resultTemporary, $json, [Text.UTF8Encoding]::new($false))
[IO.File]::Move($resultTemporary, $request.ResultPath)
'@
$wrapper = $wrapper.Replace('__REQUEST_PATH__', $escapedRequestPath)
[IO.File]::WriteAllText($wrapperPath, $wrapper, [Text.UTF8Encoding]::new($false))
$wrapperSha256 = (Get-FileHash -LiteralPath $wrapperPath -Algorithm SHA256).Hash.ToLowerInvariant()
$request['WrapperSha256'] = $wrapperSha256
$request | Export-Clixml -LiteralPath $requestTemporary -Depth 3
[IO.File]::Delete($requestPath)
[IO.File]::Move($requestTemporary, $requestPath)

$action = New-ScheduledTaskAction -Execute 'powershell.exe' -Argument (
  '-NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "{0}"' -f $wrapperPath
)
$principal = New-ScheduledTaskPrincipal -UserId $interactiveUser -LogonType Interactive -RunLevel Limited
$settings = New-ScheduledTaskSettingsSet -ExecutionTimeLimit ([TimeSpan]::FromMinutes(2)) -MultipleInstances IgnoreNew
Register-ScheduledTask -TaskName $taskName -Action $action -Principal $principal -Settings $settings | Out-Null
Start-ScheduledTask -TaskName $taskName

[pscustomobject]@{
  InvocationId = $InvocationId; Status = 'armed'; SessionId = $SessionId; WrapperSha256 = $wrapperSha256
} | ConvertTo-Json -Compress
