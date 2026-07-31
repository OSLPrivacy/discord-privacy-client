param(
  [Parameter(Mandatory = $true)]
  [ValidatePattern('^[a-z0-9][a-z0-9-]{7,63}$')]
  [string]$InvocationId
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$root = 'C:\ProgramData\OSL-QA\whatsapp-bootstrap-v1'
$invocationRoot = Join-Path $root $InvocationId
$requestPath = Join-Path $invocationRoot 'request.clixml'
$resultPath = Join-Path $invocationRoot 'result.json'
$resultTemporary = "$resultPath.tmp"
$taskName = "OSL-QA-WhatsApp-Bootstrap-$InvocationId"

if (-not (Test-Path -LiteralPath $requestPath -PathType Leaf)) {
  throw 'exact bootstrap invocation does not exist'
}
$request = Import-Clixml -LiteralPath $requestPath
if ($request.InvocationId -cne $InvocationId -or $request.ResultPath -cne $resultPath -or
    [string]$request.WrapperSha256 -cnotmatch '^[a-f0-9]{64}$') {
  throw 'bootstrap invocation identity mismatch'
}
$task = Get-ScheduledTask -TaskName $taskName -ErrorAction SilentlyContinue
$taskInfo = if ($task) { Get-ScheduledTaskInfo -TaskName $taskName } else { $null }

if (Test-Path -LiteralPath $resultPath -PathType Leaf) {
  $result = Get-Content -LiteralPath $resultPath -Raw | ConvertFrom-Json -ErrorAction Stop
  if ($result.InvocationId -cne $InvocationId -or -not $result.Terminal -or
      $result.WrapperSha256 -cne $request.WrapperSha256 -or
      $result.ProductId -cne '9NKSQGP7F2NH' -or
      $result.PackageFamily -cne '5319275A.WhatsAppDesktop_cv1g1gvanyjgm' -or
      $result.AppLaunched -ne $false -or $result.ProfileRead -ne $false -or
      $result.LinkStateTouched -ne $false -or [int]$result.ProcessesTerminated -ne 0) {
    throw 'bootstrap terminal result contract mismatch'
  }
  [pscustomobject]@{
    InvocationId = $InvocationId
    Terminal = $true
    TaskState = if ($task) { [string]$task.State } else { 'MissingAfterTerminalResult' }
    LastTaskResult = if ($taskInfo) { [int]$taskInfo.LastTaskResult } else { $null }
    Result = $result
  } | ConvertTo-Json -Depth 8 -Compress
  exit 0
}

if (-not $task) {
  [pscustomobject]@{
    InvocationId = $InvocationId
    Terminal = $true
    Status = 'runnerMissingWithoutResult'
    ResultTemporaryPresent = Test-Path -LiteralPath $resultTemporary -PathType Leaf
  } | ConvertTo-Json -Compress
  exit 0
}

if ($task.State -eq 'Ready' -and $taskInfo.LastRunTime.Year -gt 2000) {
  [pscustomobject]@{
    InvocationId = $InvocationId
    Terminal = $true
    Status = 'runnerExitedWithoutResult'
    TaskState = [string]$task.State
    LastTaskResult = [int]$taskInfo.LastTaskResult
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
  ResultTemporaryPresent = Test-Path -LiteralPath $resultTemporary -PathType Leaf
} | ConvertTo-Json -Compress
