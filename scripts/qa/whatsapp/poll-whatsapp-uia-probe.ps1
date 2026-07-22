param(
  [Parameter(Mandatory = $true)]
  [ValidatePattern('^[a-z0-9][a-z0-9-]{7,63}$')]
  [string]$InvocationId
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$root = 'C:\ProgramData\OSL-QA\whatsapp-uia-structure-v1'
$invocationRoot = Join-Path $root $InvocationId
$requestPath = Join-Path $invocationRoot 'request.clixml'
$resultPath = Join-Path $invocationRoot 'result.json'
$taskName = "OSL-QA-WhatsApp-Uia-$InvocationId"

if (-not (Test-Path -LiteralPath $requestPath -PathType Leaf)) { throw 'exact UIA invocation does not exist' }
$request = Import-Clixml -LiteralPath $requestPath
if ($request.InvocationId -cne $InvocationId -or $request.ResultPath -cne $resultPath -or
    [string]$request.WrapperSha256 -cnotmatch '^[a-f0-9]{64}$') { throw 'UIA invocation identity mismatch' }
$task = Get-ScheduledTask -TaskName $taskName -ErrorAction SilentlyContinue
$taskInfo = if ($task) { Get-ScheduledTaskInfo -TaskName $taskName } else { $null }

if (Test-Path -LiteralPath $resultPath -PathType Leaf) {
  $result = Get-Content -LiteralPath $resultPath -Raw | ConvertFrom-Json -ErrorAction Stop
  if ($result.InvocationId -cne $InvocationId -or -not $result.Terminal -or
      $result.WrapperSha256 -cne $request.WrapperSha256 -or
      $result.PackageFamily -cne '5319275A.WhatsAppDesktop_cv1g1gvanyjgm' -or
      [int]$result.NodeCount -gt 512 -or @($result.Nodes).Count -ne [int]$result.NodeCount -or
      $result.ForegroundChanged -ne $false -or $result.InputInjected -ne $false -or
      $result.ProviderStorageRead -ne $false -or $result.ContentPropertiesRead -ne $false -or
      $result.AppLaunched -ne $false -or [int]$result.ProcessesTerminated -ne 0) {
    throw 'UIA terminal result contract mismatch'
  }
  [pscustomobject]@{
    InvocationId = $InvocationId
    Terminal = $true
    TaskState = if ($task) { [string]$task.State } else { 'MissingAfterTerminalResult' }
    LastTaskResult = if ($taskInfo) { [int]$taskInfo.LastTaskResult } else { $null }
    Result = $result
  } | ConvertTo-Json -Depth 10 -Compress
  exit 0
}

if (-not $task -or ($task.State -eq 'Ready' -and $taskInfo.LastRunTime.Year -gt 2000)) {
  [pscustomobject]@{
    InvocationId = $InvocationId
    Terminal = $true
    Status = if ($task) { 'runnerExitedWithoutResult' } else { 'runnerMissingWithoutResult' }
  } | ConvertTo-Json -Compress
  exit 0
}

[pscustomobject]@{
  InvocationId = $InvocationId
  Terminal = $false
  Status = 'runningOrQueued'
  TaskState = [string]$task.State
} | ConvertTo-Json -Compress
