<#
Runs a PowerShell payload in the logged-on VM desktop session.

Azure VM run-command runs as SYSTEM in session 0. Window and accessibility work
must be delegated to a scheduled task registered with LogonType Interactive so
the payload executes where the desktop actually is.
#>
param(
  [Parameter(Mandatory = $true)]
  [ValidatePattern('^[a-z0-9][a-z0-9-]{7,63}$')]
  [string]$InvocationId,

  [uri]$PayloadUri,

  [ValidatePattern('^[A-Za-z0-9+/=]+$')]
  [string]$PayloadBase64 = '',

  [ValidatePattern('^[A-Za-z0-9+/=]+$')]
  [string]$PayloadGzipBase64 = '',

  [Parameter(Mandatory = $true)]
  [ValidatePattern('^[0-9a-fA-F]{64}$')]
  [string]$PayloadSha256,

  [ValidateRange(1, 128)]
  [int]$SessionId = 1,

  [ValidatePattern('^[A-Za-z0-9_.\\-]+$')]
  [string]$InteractiveUser = 'osladmin',

  [ValidateRange(5, 900)]
  [int]$WaitSeconds = 90
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
Add-Type -AssemblyName System.Net.Http

$root = 'C:\OSL\desktop-runner'
$outRoot = 'C:\OSL\out'
$allowedPayloadHost = 'osltestartifactsa7d5.blob.core.windows.net'
$invocationRoot = Join-Path $root $InvocationId
$payloadPath = Join-Path $invocationRoot 'payload.ps1'
$wrapperPath = Join-Path $invocationRoot 'wrapper.ps1'
$stdoutPath = Join-Path $invocationRoot 'stdout.txt'
$stderrPath = Join-Path $invocationRoot 'stderr.txt'
$taskLogPath = Join-Path $outRoot "$InvocationId.log"
$resultPath = Join-Path $invocationRoot 'result.json'
$resultTemporary = "$resultPath.tmp"
$taskName = "OSLQA_4951_$InvocationId"

if (Test-Path -LiteralPath $invocationRoot) { throw 'desktop-runner invocation already exists' }
[void](New-Item -ItemType Directory -Path $invocationRoot -Force)
[void](New-Item -ItemType Directory -Path $outRoot -Force)

function Save-ManagedIdentityArtifact([uri]$Uri, [string]$Destination) {
  if ($Uri.Scheme -cne 'https' -or $Uri.Host -cne $allowedPayloadHost -or $Uri.Query -or $Uri.Fragment) {
    throw 'payload URI is outside the exact trusted artifact host'
  }
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
    if (-not $response.IsSuccessStatusCode) { throw 'trusted payload download failed' }
    $bytes = $response.Content.ReadAsByteArrayAsync().GetAwaiter().GetResult()
    [IO.File]::WriteAllBytes($temporary, $bytes)
    [IO.File]::Move($temporary, $Destination)
  } finally {
    $client.Dispose()
  }
}

if ($PayloadUri) {
  Save-ManagedIdentityArtifact $PayloadUri $payloadPath
} elseif ($PayloadGzipBase64) {
  $compressedBytes = [Convert]::FromBase64String($PayloadGzipBase64)
  $inputStream = [IO.MemoryStream]::new($compressedBytes)
  $gzipStream = [IO.Compression.GzipStream]::new($inputStream, [IO.Compression.CompressionMode]::Decompress)
  $outputStream = [IO.MemoryStream]::new()
  try {
    $gzipStream.CopyTo($outputStream)
    [IO.File]::WriteAllBytes($payloadPath, $outputStream.ToArray())
  } finally {
    $outputStream.Dispose()
    $gzipStream.Dispose()
    $inputStream.Dispose()
  }
} elseif ($PayloadBase64) {
  $payloadBytes = [Convert]::FromBase64String($PayloadBase64)
  [IO.File]::WriteAllBytes($payloadPath, $payloadBytes)
} else {
  throw 'desktop-runner requires PayloadUri or PayloadBase64'
}
$actualPayloadSha = (Get-FileHash -LiteralPath $payloadPath -Algorithm SHA256).Hash.ToLowerInvariant()
if ($actualPayloadSha -cne $PayloadSha256.ToLowerInvariant()) { throw 'payload SHA-256 mismatch' }

$explorers = @(Get-CimInstance Win32_Process -Filter "Name = 'explorer.exe'" | Where-Object {
  [int]$_.SessionId -eq $SessionId
})
if ($explorers.Count -ne 1) { throw 'interactive Explorer session is unavailable or ambiguous' }
$owner = Invoke-CimMethod -InputObject $explorers[0] -MethodName GetOwner
if ($owner.ReturnValue -ne 0 -or $owner.User -cne $InteractiveUser) {
  throw "interactive session owner is not exact $InteractiveUser identity"
}

@"
`$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
`$started = [DateTime]::UtcNow.ToString('o')
`$exitCode = 1
`$runnerError = `$null
try {
  `$processInfo = [Diagnostics.ProcessStartInfo]::new()
  `$processInfo.FileName = 'powershell.exe'
  `$processInfo.Arguments = '-NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "$($payloadPath.Replace('"', '\"'))"'
  `$processInfo.UseShellExecute = `$false
  `$processInfo.RedirectStandardOutput = `$true
  `$processInfo.RedirectStandardError = `$true
  `$processInfo.CreateNoWindow = `$true
  `$process = [Diagnostics.Process]::Start(`$processInfo)
  `$capturedStdout = `$process.StandardOutput.ReadToEnd()
  `$capturedStderr = `$process.StandardError.ReadToEnd()
  `$process.WaitForExit()
  [IO.File]::WriteAllText('$($stdoutPath.Replace("'", "''"))', `$capturedStdout, [Text.UTF8Encoding]::new(`$false))
  [IO.File]::WriteAllText('$($stderrPath.Replace("'", "''"))', `$capturedStderr, [Text.UTF8Encoding]::new(`$false))
  `$exitCode = [int]`$process.ExitCode
  `$status = if (`$exitCode -eq 0) { 'completed' } else { 'payloadFailed' }
} catch {
  `$status = 'runnerFailed'
  `$runnerError = `$_.Exception.Message
}
`$completed = [DateTime]::UtcNow.ToString('o')
`$json = '{"InvocationId":"$InvocationId","Terminal":true,"Status":"' + `$status +
  '","StartedUtc":"' + `$started + '","CompletedUtc":"' + `$completed +
  '","PayloadExitCode":' + `$exitCode + '}'
[IO.File]::WriteAllText('$($resultTemporary.Replace("'", "''"))', `$json, [Text.UTF8Encoding]::new(`$false))
[IO.File]::Move('$($resultTemporary.Replace("'", "''"))', '$($resultPath.Replace("'", "''"))')
"@ | Set-Content -LiteralPath $wrapperPath -Encoding UTF8

$taskActionArguments = '/c powershell.exe -NoProfile -ExecutionPolicy Bypass -File "{0}" > "{1}" 2>&1' -f
  $wrapperPath,
  $taskLogPath
$taskAction = New-ScheduledTaskAction -Execute 'cmd.exe' -Argument $taskActionArguments
$principal = New-ScheduledTaskPrincipal -UserId $InteractiveUser -LogonType Interactive -RunLevel Highest
$settings = New-ScheduledTaskSettingsSet `
  -ExecutionTimeLimit ([TimeSpan]::FromSeconds($WaitSeconds + 30)) `
  -MultipleInstances IgnoreNew `
  -AllowStartIfOnBatteries `
  -DontStopIfGoingOnBatteries

Register-ScheduledTask -TaskName $taskName -Action $taskAction -Principal $principal -Settings $settings -Force | Out-Null
Start-ScheduledTask -TaskName $taskName

$deadline = [DateTime]::UtcNow.AddSeconds($WaitSeconds)
while ((-not (Test-Path -LiteralPath $resultPath -PathType Leaf)) -and [DateTime]::UtcNow -lt $deadline) {
  Start-Sleep -Milliseconds 500
}

if (-not (Test-Path -LiteralPath $resultPath -PathType Leaf)) {
  $taskInfo = Get-ScheduledTaskInfo -TaskName $taskName
  $task = Get-ScheduledTask -TaskName $taskName
  Write-Output "DESKTOP-RUNNER InvocationId=$InvocationId Terminal=false Status=timeout TaskName=$taskName TaskState=$($task.State) LastTaskResult=$($taskInfo.LastTaskResult) TaskLogonType=Interactive TaskRunLevel=Highest"
  Write-Output "DESKTOP-RUNNER ResultPath=$resultPath TaskLogPath=$taskLogPath"
  exit 2
}

$result = Get-Content -LiteralPath $resultPath -Raw | ConvertFrom-Json -ErrorAction Stop
if ($result.InvocationId -cne $InvocationId -or -not $result.Terminal) {
  throw 'desktop-runner terminal result identity mismatch'
}
$stdout = if (Test-Path -LiteralPath $stdoutPath -PathType Leaf) {
  Get-Content -LiteralPath $stdoutPath -Raw
} else { '' }
$stderr = if (Test-Path -LiteralPath $stderrPath -PathType Leaf) {
  Get-Content -LiteralPath $stderrPath -Raw
} else { '' }
Write-Output "DESKTOP-RUNNER InvocationId=$InvocationId Terminal=true Status=$($result.Status) TaskName=$taskName TaskPrincipalUser=$InteractiveUser TaskLogonType=Interactive TaskRunLevel=Highest TaskActionExecute=cmd.exe"
Write-Output "DESKTOP-RUNNER TaskActionArguments=$taskActionArguments PayloadExitCode=$($result.PayloadExitCode)"
if ($stdout) { Write-Output $stdout.TrimEnd() }
if ($stderr) {
  Write-Output 'DESKTOP-RUNNER STDERR-BEGIN'
  Write-Output $stderr.TrimEnd()
  Write-Output 'DESKTOP-RUNNER STDERR-END'
}
