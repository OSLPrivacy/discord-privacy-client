# OSL-QA-Capability: signal-uia-poll/v1
param([Parameter(Mandatory)][ValidatePattern('^[a-z0-9][a-z0-9-]{7,63}$')][string]$InvocationId)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$root = Join-Path 'C:\ProgramData\OSL-QA\signal-uia' $InvocationId
$requestPath = Join-Path $root 'request.clixml'
$resultPath = Join-Path $root 'result.json'
$resultTemporary = "$resultPath.tmp"
if (-not (Test-Path -LiteralPath $requestPath -PathType Leaf)) { throw 'exact invocation request is missing' }
$request = Import-Clixml -LiteralPath $requestPath
if ($request.InvocationId -cne $InvocationId -or $request.ResultPath -cne $resultPath) { throw 'result identity mismatch' }
$taskName = "OSL-QA-Signal-$InvocationId"
$task = Get-ScheduledTask -TaskName $taskName -ErrorAction SilentlyContinue
$taskInfo = if ($task) { Get-ScheduledTaskInfo -TaskName $taskName -ErrorAction SilentlyContinue } else { $null }
if (Test-Path -LiteralPath $resultPath -PathType Leaf) {
  $result = Get-Content -LiteralPath $resultPath -Raw | ConvertFrom-Json
  if (-not $result.Terminal -or $result.InvocationId -cne $InvocationId) { throw 'result identity mismatch' }
  [pscustomobject]@{ Status = [string]$result.Status; Terminal = $true; Result = $result.HarnessResult; TaskState = if ($task) {[string]$task.State}else{'MissingAfterTerminalResult'} } | ConvertTo-Json -Depth 12 -Compress
  exit 0
}
if (-not $task) {
  [pscustomobject]@{ Status='runnerMissingWithoutResult'; Terminal=$false; ResultTemporaryPresent=(Test-Path -LiteralPath $resultTemporary) } | ConvertTo-Json -Compress
  exit 0
}
[pscustomobject]@{ Status='pending'; Terminal=$false; TaskState=[string]$task.State; LastTaskResult=if($taskInfo){[int]$taskInfo.LastTaskResult}else{$null}; ResultTemporaryPresent=(Test-Path -LiteralPath $resultTemporary) } | ConvertTo-Json -Compress
