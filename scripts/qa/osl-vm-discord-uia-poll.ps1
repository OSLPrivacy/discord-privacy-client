param(
  [Parameter(Mandatory = $true)]
  [string]$InvocationId
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

if ($InvocationId -cnotmatch '^[a-z0-9][a-z0-9-]{7,63}$') { throw 'invalid invocation ID' }

$root = 'C:\ProgramData\OSL-QA\discord-uia'
$invocationRoot = Join-Path $root $InvocationId
$requestPath = Join-Path $invocationRoot 'request.clixml'
$resultPath = Join-Path $invocationRoot 'result.json'
$resultTemporary = "$resultPath.tmp"
$taskName = "OSL-QA-Discord-$InvocationId"

if (-not (Test-Path -LiteralPath $requestPath -PathType Leaf)) {
  throw 'exact invocation request does not exist'
}
$request = Import-Clixml -LiteralPath $requestPath
if ($request.InvocationId -cne $InvocationId -or $request.ResultPath -cne $resultPath) {
  throw 'invocation request identity mismatch'
}

$task = Get-ScheduledTask -TaskName $taskName -ErrorAction SilentlyContinue
$taskInfo = if ($task) { Get-ScheduledTaskInfo -TaskName $taskName } else { $null }

if (Test-Path -LiteralPath $resultPath -PathType Leaf) {
  $result = Get-Content -LiteralPath $resultPath -Raw | ConvertFrom-Json -ErrorAction Stop
  if ($result.InvocationId -cne $InvocationId -or -not $result.Terminal) {
    throw 'terminal result identity mismatch'
  }
  [pscustomobject]@{
    InvocationId = $InvocationId
    Terminal = $true
    TaskState = if ($task) { [string]$task.State } else { 'MissingAfterTerminalResult' }
    LastTaskResult = if ($taskInfo) { [int]$taskInfo.LastTaskResult } else { $null }
    Result = $result
  } | ConvertTo-Json -Depth 12 -Compress
  exit 0
}

if (-not $task) {
  [pscustomobject]@{
    InvocationId = $InvocationId
    Terminal = $true
    Status = 'runnerMissingWithoutResult'
    TaskState = 'Missing'
    ResultTemporaryPresent = Test-Path -LiteralPath $resultTemporary -PathType Leaf
  } | ConvertTo-Json -Compress
  exit 0
}

[pscustomobject]@{
  InvocationId = $InvocationId
  Terminal = $false
  Status = 'runningOrQueued'
  TaskState = [string]$task.State
  LastTaskResult = [int]$taskInfo.LastTaskResult
  LastRunTimeUtc = if ($taskInfo.LastRunTime -gt [DateTime]::MinValue) { $taskInfo.LastRunTime.ToUniversalTime().ToString('o') } else { $null }
  ResultTemporaryPresent = Test-Path -LiteralPath $resultTemporary -PathType Leaf
} | ConvertTo-Json -Compress
