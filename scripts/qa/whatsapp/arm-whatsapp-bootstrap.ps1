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

$root = 'C:\ProgramData\OSL-QA\whatsapp-bootstrap-v1'
$invocationRoot = Join-Path $root $InvocationId
$requestPath = Join-Path $invocationRoot 'request.clixml'
$wrapperPath = Join-Path $invocationRoot 'run-interactive.ps1'
$resultPath = Join-Path $invocationRoot 'result.json'
$taskName = "OSL-QA-WhatsApp-Bootstrap-$InvocationId"

$osltestExplorers = @()
foreach ($explorer in @(Get-CimInstance Win32_Process -Filter "Name = 'explorer.exe'")) {
  $candidateOwner = Invoke-CimMethod -InputObject $explorer -MethodName GetOwner
  if ($candidateOwner.ReturnValue -eq 0 -and $candidateOwner.User -ceq 'osltest' -and $candidateOwner.Domain) {
    $osltestExplorers += [pscustomobject]@{ Process = $explorer; Owner = $candidateOwner }
  }
}
if ($osltestExplorers.Count -ne 1 -or [int]$osltestExplorers[0].Process.SessionId -ne $SessionId) {
  throw 'exact osltest interactive session is unavailable or ambiguous'
}
$interactiveUser = "$($osltestExplorers[0].Owner.Domain)\$($osltestExplorers[0].Owner.User)"

if (Test-Path -LiteralPath $invocationRoot) {
  if (-not (Test-Path -LiteralPath $requestPath -PathType Leaf)) {
    throw 'existing invocation is incomplete'
  }
  $existing = Import-Clixml -LiteralPath $requestPath
  if (-not (Test-Path -LiteralPath $wrapperPath -PathType Leaf)) {
    throw 'existing invocation runner is missing'
  }
  $existingWrapperSha256 = (Get-FileHash -LiteralPath $wrapperPath -Algorithm SHA256).Hash.ToLowerInvariant()
  if ($existing.InvocationId -cne $InvocationId -or [int]$existing.SessionId -ne $SessionId -or
      $existing.InteractiveUser -cne $interactiveUser -or $existing.ResultPath -cne $resultPath -or
      $existing.WrapperSha256 -cne $existingWrapperSha256) {
    throw 'existing invocation identity mismatch'
  }
  [pscustomobject]@{
    InvocationId = $InvocationId
    Status = if (Test-Path -LiteralPath $resultPath -PathType Leaf) { 'alreadyCompleted' } else { 'alreadyArmed' }
    TaskName = $taskName
    SessionId = $SessionId
    WrapperSha256 = $existingWrapperSha256
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

$productId = '9NKSQGP7F2NH'
$packageName = '5319275A.WhatsAppDesktop'
$packageFamily = '5319275A.WhatsAppDesktop_cv1g1gvanyjgm'
$publisherId = 'cv1g1gvanyjgm'
$request = Import-Clixml -LiteralPath '__REQUEST_PATH__'
$resultTemporary = "$($request.ResultPath).tmp"

function Get-ExactWhatsAppPackage {
  $packages = @(Get-AppxPackage -Name $packageName -PackageTypeFilter Main -ErrorAction SilentlyContinue)
  $matching = @($packages | Where-Object {
    $_.Name -ceq $packageName -and
    $_.PackageFamilyName -ceq $packageFamily -and
    $_.PublisherId -ceq $publisherId -and
    -not $_.IsFramework -and
    -not $_.IsResourcePackage -and
    $_.PackageFullName -cmatch '^5319275A[.]WhatsAppDesktop_[0-9]+[.][0-9]+[.][0-9]+[.][0-9]+_(x64|x86|arm64|neutral)__cv1g1gvanyjgm$'
  })
  if ($packages.Count -ne $matching.Count) { throw 'unexpected WhatsApp package registration exists' }
  if ($matching.Count -gt 1) { throw 'WhatsApp package registration is ambiguous' }
  if ($matching.Count -eq 0) { return $null }
  $package = $matching[0]
  $installLocation = [IO.Path]::GetFullPath([string]$package.InstallLocation).TrimEnd('\')
  $programFiles = [IO.Path]::GetFullPath([Environment]::GetFolderPath('ProgramFiles')).TrimEnd('\')
  $expectedPrefix = "$programFiles\WindowsApps\5319275A.WhatsAppDesktop_"
  if (-not $installLocation.StartsWith($expectedPrefix, [StringComparison]::OrdinalIgnoreCase) -or
      $installLocation.Contains('..') -or
      [IO.Directory]::GetParent($installLocation).FullName -ine "$programFiles\WindowsApps") {
    throw 'WhatsApp package location is outside protected WindowsApps'
  }
  return $package
}

function Get-TrustedWinget {
  $packages = @(Get-AppxPackage -Name 'Microsoft.DesktopAppInstaller' -PackageTypeFilter Main -ErrorAction SilentlyContinue | Where-Object {
    $_.PackageFamilyName -ceq 'Microsoft.DesktopAppInstaller_8wekyb3d8bbwe' -and
    $_.PublisherId -ceq '8wekyb3d8bbwe' -and
    -not $_.IsFramework -and -not $_.IsResourcePackage
  })
  if ($packages.Count -ne 1) { throw 'trusted Desktop App Installer is unavailable or ambiguous' }
  $root = [IO.Path]::GetFullPath([string]$packages[0].InstallLocation).TrimEnd('\')
  $programFiles = [IO.Path]::GetFullPath([Environment]::GetFolderPath('ProgramFiles')).TrimEnd('\')
  if ([IO.Directory]::GetParent($root).FullName -ine "$programFiles\WindowsApps" -or
      -not $root.StartsWith("$programFiles\WindowsApps\Microsoft.DesktopAppInstaller_", [StringComparison]::OrdinalIgnoreCase)) {
    throw 'Desktop App Installer is outside protected WindowsApps'
  }
  $winget = Join-Path $root 'winget.exe'
  if (-not (Test-Path -LiteralPath $winget -PathType Leaf)) { throw 'trusted winget executable is unavailable' }
  return $winget
}

$started = [DateTime]::UtcNow.ToString('o')
try {
  $currentIdentity = [Security.Principal.WindowsIdentity]::GetCurrent().Name
  $currentSessionId = [Diagnostics.Process]::GetCurrentProcess().SessionId
  if ($currentIdentity -cne $request.InteractiveUser -or $currentSessionId -ne [int]$request.SessionId) {
    throw 'interactive runner session identity mismatch'
  }
  $actualWrapperSha256 = (Get-FileHash -LiteralPath $PSCommandPath -Algorithm SHA256).Hash.ToLowerInvariant()
  if ($actualWrapperSha256 -cne $request.WrapperSha256) {
    throw 'interactive runner hash mismatch'
  }
  $package = Get-ExactWhatsAppPackage
  $installedBefore = $null -ne $package
  $installAttempted = $false
  if (-not $package) {
    $installAttempted = $true
    $winget = Get-TrustedWinget
    $process = Start-Process -FilePath $winget -ArgumentList @(
      'install', '--id', $productId, '--source', 'msstore', '--exact', '--silent',
      '--disable-interactivity', '--accept-source-agreements', '--accept-package-agreements'
    ) -Wait -PassThru -WindowStyle Hidden
    if ($process.ExitCode -ne 0) { throw 'official Store install did not complete successfully' }
    $deadline = [DateTime]::UtcNow.AddSeconds(90)
    do {
      $package = Get-ExactWhatsAppPackage
      if ($package) { break }
      Start-Sleep -Milliseconds 500
    } while ([DateTime]::UtcNow -lt $deadline)
    if (-not $package) { throw 'official WhatsApp package did not register before the bounded deadline' }
  }
  $terminal = [ordered]@{
    InvocationId = $request.InvocationId
    WrapperSha256 = $request.WrapperSha256
    Terminal = $true
    Status = 'verified'
    StartedUtc = $started
    CompletedUtc = [DateTime]::UtcNow.ToString('o')
    ProductId = $productId
    PackageFamily = $packageFamily
    InstalledBefore = $installedBefore
    InstallAttempted = $installAttempted
    OfficialPackageVerified = $true
    Version = [string]$package.Version
    Architecture = [string]$package.Architecture
    AppLaunched = $false
    ProfileRead = $false
    LinkStateTouched = $false
    ProcessesTerminated = 0
  }
} catch {
  $terminal = [ordered]@{
    InvocationId = $request.InvocationId
    WrapperSha256 = $request.WrapperSha256
    Terminal = $true
    Status = 'failedClosed'
    StartedUtc = $started
    CompletedUtc = [DateTime]::UtcNow.ToString('o')
    ProductId = $productId
    PackageFamily = $packageFamily
    OfficialPackageVerified = $false
    AppLaunched = $false
    ProfileRead = $false
    LinkStateTouched = $false
    ProcessesTerminated = 0
    Error = 'whatsapp-bootstrap-failed-closed'
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

$taskAction = New-ScheduledTaskAction -Execute 'powershell.exe' -Argument (
  '-NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "{0}"' -f $wrapperPath
)
$principal = New-ScheduledTaskPrincipal -UserId $interactiveUser -LogonType Interactive -RunLevel Limited
$settings = New-ScheduledTaskSettingsSet -ExecutionTimeLimit ([TimeSpan]::FromMinutes(4)) -MultipleInstances IgnoreNew
Register-ScheduledTask -TaskName $taskName -Action $taskAction -Principal $principal -Settings $settings | Out-Null
Start-ScheduledTask -TaskName $taskName

[pscustomobject]@{
  InvocationId = $InvocationId
  Status = 'armed'
  TaskName = $taskName
  SessionId = $SessionId
  WrapperSha256 = $wrapperSha256
} | ConvertTo-Json -Compress
