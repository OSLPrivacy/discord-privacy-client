param(
  [Parameter(Mandatory = $true)]
  [string]$InvocationId,

  [Parameter(Mandatory = $true)]
  [uri]$HarnessUri,

  [Parameter(Mandatory = $true)]
  [string]$HarnessSha256,

  [Parameter(Mandatory = $true)]
  [ValidateSet(
    'Inventory',
    'Inspect',
    'OpenCurrent',
    'PrepareOverlay',
    'OpenVerifiedFriend',
    'SetDeterministic',
    'Send',
    'InspectInbound',
    'ToggleVisibility',
    'ExerciseWindowLifecycle'
  )]
  [string]$Action,

  [Parameter(Mandatory = $true)]
  [string]$OslExePath,

  [Parameter(Mandatory = $true)]
  [string]$OslExeSha256,

  [Parameter(Mandatory = $true)]
  [int]$SessionId,

  [ValidatePattern('^[A-Za-z0-9._-]{1,48}$')]
  [string]$CaseId = 'BASE',

  [ValidateSet('On', 'Off')]
  [string]$Visibility = 'On',

  [ValidateRange(10, 180)]
  [int]$TimeoutSeconds = 60
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
Add-Type -AssemblyName System.Net.Http

$allowedHarnessHost = 'osltestartifactsa7d5.blob.core.windows.net'
$root = 'C:\ProgramData\OSL-QA\discord-uia'

if ($InvocationId -cnotmatch '^[a-z0-9][a-z0-9-]{7,63}$') { throw 'invalid invocation ID' }
if ($HarnessSha256 -cnotmatch '^[0-9a-fA-F]{64}$') { throw 'invalid harness SHA-256' }
if ($OslExeSha256 -cnotmatch '^[0-9a-fA-F]{64}$') { throw 'invalid OSL executable SHA-256' }
if ($HarnessUri.Scheme -cne 'https' -or $HarnessUri.Host -cne $allowedHarnessHost) {
  throw 'harness URI is outside the exact trusted artifact host'
}

$invocationRoot = Join-Path $root $InvocationId
$requestPath = Join-Path $invocationRoot 'request.clixml'
$harnessPath = Join-Path $invocationRoot 'osl-vm-discord-uia-harness.ps1'
$wrapperPath = Join-Path $invocationRoot 'run-interactive.ps1'
$resultPath = Join-Path $invocationRoot 'result.json'
$taskName = "OSL-QA-Discord-$InvocationId"

if (Test-Path -LiteralPath $invocationRoot) { throw 'invocation ID already exists' }
[void](New-Item -ItemType Directory -Path $invocationRoot -Force)

function Save-ManagedIdentityArtifact([uri]$Uri, [string]$Destination) {
  $tokenUri = 'http://169.254.169.254/metadata/identity/oauth2/token' +
    '?api-version=2018-02-01&resource=https%3A%2F%2Fstorage.azure.com%2F'
  $token = Invoke-RestMethod -Method Get -Uri $tokenUri -Headers @{ Metadata = 'true' } -TimeoutSec 10
  if (-not $token.access_token) { throw 'managed identity storage token unavailable' }

  $temporary = "$Destination.download"
  $client = [Net.Http.HttpClient]::new()
  try {
    $client.Timeout = [TimeSpan]::FromSeconds(20)
    $client.DefaultRequestHeaders.Authorization = [Net.Http.Headers.AuthenticationHeaderValue]::new('Bearer', [string]$token.access_token)
    $client.DefaultRequestHeaders.Add('x-ms-version', '2023-11-03')
    $response = $client.GetAsync($Uri).GetAwaiter().GetResult()
    if (-not $response.IsSuccessStatusCode) { throw 'trusted harness download failed' }
    $bytes = $response.Content.ReadAsByteArrayAsync().GetAwaiter().GetResult()
    [IO.File]::WriteAllBytes($temporary, $bytes)
    $actual = (Get-FileHash -LiteralPath $temporary -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($actual -cne $HarnessSha256.ToLowerInvariant()) { throw 'downloaded harness hash mismatch' }
    [IO.File]::Move($temporary, $Destination)
  } finally {
    $client.Dispose()
  }
}

Save-ManagedIdentityArtifact $HarnessUri $harnessPath

$explorers = @(Get-CimInstance Win32_Process -Filter "Name = 'explorer.exe'" | Where-Object {
  [int]$_.SessionId -eq $SessionId
})
if ($explorers.Count -ne 1) { throw 'interactive Explorer session is unavailable or ambiguous' }
$owner = Invoke-CimMethod -InputObject $explorers[0] -MethodName GetOwner
if ($owner.ReturnValue -ne 0 -or $owner.User -cne 'osltest' -or -not $owner.Domain) {
  throw 'interactive session owner is not exact osltest identity'
}
$interactiveUser = "$($owner.Domain)\$($owner.User)"

$request = [ordered]@{
  InvocationId = $InvocationId
  HarnessPath = $harnessPath
  HarnessSha256 = $HarnessSha256.ToLowerInvariant()
  Action = $Action
  OslExePath = [IO.Path]::GetFullPath($OslExePath)
  OslExeSha256 = $OslExeSha256.ToLowerInvariant()
  SessionId = $SessionId
  CaseId = $CaseId
  Visibility = $Visibility
  TimeoutSeconds = $TimeoutSeconds
  ResultPath = $resultPath
}
$requestTemporary = "$requestPath.tmp"
$request | Export-Clixml -LiteralPath $requestTemporary -Depth 4
[IO.File]::Move($requestTemporary, $requestPath)

$escapedRequestPath = $requestPath.Replace("'", "''")
$wrapper = @"
`$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
`$request = Import-Clixml -LiteralPath '$escapedRequestPath'
`$resultTemporary = "`$(`$request.ResultPath).tmp"
try {
  `$actualHarnessSha = (Get-FileHash -LiteralPath `$request.HarnessPath -Algorithm SHA256).Hash.ToLowerInvariant()
  if (`$actualHarnessSha -cne `$request.HarnessSha256) { throw 'armed harness hash mismatch' }
  `$arguments = @{
    Action = `$request.Action
    OslExePath = `$request.OslExePath
    OslExeSha256 = `$request.OslExeSha256
    SessionId = [int]`$request.SessionId
    CaseId = `$request.CaseId
    Visibility = `$request.Visibility
    TimeoutSeconds = [int]`$request.TimeoutSeconds
  }
  `$started = [DateTime]::UtcNow.ToString('o')
  `$raw = (& `$request.HarnessPath @arguments | Out-String).Trim()
  `$harnessExitCode = if (Test-Path Variable:LASTEXITCODE) { [int]`$LASTEXITCODE } else { 0 }
  `$harnessResult = `$raw | ConvertFrom-Json -ErrorAction Stop
  `$terminal = [ordered]@{
    InvocationId = `$request.InvocationId
    Terminal = `$true
    Status = if (`$harnessResult.Ok) { 'completed' } else { 'harnessFailed' }
    StartedUtc = `$started
    CompletedUtc = [DateTime]::UtcNow.ToString('o')
    HarnessExitCode = `$harnessExitCode
    HarnessResult = `$harnessResult
  }
} catch {
  `$terminal = [ordered]@{
    InvocationId = `$request.InvocationId
    Terminal = `$true
    Status = 'runnerFailed'
    CompletedUtc = [DateTime]::UtcNow.ToString('o')
    HarnessExitCode = 1
    Error = 'interactive-runner-failed-closed'
    ExceptionType = `$_.Exception.GetType().Name
    Detail = if (`$_.Exception.Message.Length -le 160) { `$_.Exception.Message } else { `$_.Exception.Message.Substring(0, 160) }
  }
}
`$json = `$terminal | ConvertTo-Json -Depth 10 -Compress
[IO.File]::WriteAllText(`$resultTemporary, `$json, [Text.UTF8Encoding]::new(`$false))
[IO.File]::Move(`$resultTemporary, `$request.ResultPath)
"@
[IO.File]::WriteAllText($wrapperPath, $wrapper, [Text.UTF8Encoding]::new($false))

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
  ResultPath = $resultPath
} | ConvertTo-Json -Compress
