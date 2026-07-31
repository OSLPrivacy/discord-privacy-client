# OSL-QA-Capability: signal-store-install-poll/v1
param([Parameter(Mandatory)][ValidatePattern('^[a-z0-9][a-z0-9-]{7,63}$')][string]$InvocationId)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$root = Join-Path 'C:\ProgramData\OSL-QA\signal-store-install' $InvocationId
$requestPath = Join-Path $root 'request.clixml'
$resultPath = Join-Path $root 'result.json'
if (-not (Test-Path -LiteralPath $requestPath -PathType Leaf)) { throw 'exact invocation request is missing' }
$request = Import-Clixml -LiteralPath $requestPath
if ($request.InvocationId -cne $InvocationId -or $request.ResultPath -cne $resultPath -or $request.ProductId -cne 'XP89119P9F2PCQ' -or $request.StoreSource -cne 'msstore' -or $request.Mode -notin @('install','audit')) { throw 'invocation identity mismatch' }
$taskName = "OSL-QA-Signal-Store-$InvocationId"
$task = Get-ScheduledTask -TaskName $taskName -ErrorAction SilentlyContinue
$taskInfo = if ($task) { Get-ScheduledTaskInfo -TaskName $taskName -ErrorAction SilentlyContinue } else { $null }
if (Test-Path -LiteralPath $resultPath -PathType Leaf) {
  $result = Get-Content -LiteralPath $resultPath -Raw | ConvertFrom-Json
  if (-not $result.Terminal -or $result.InvocationId -cne $InvocationId -or $result.ProductId -cne 'XP89119P9F2PCQ' -or $result.Source -cne 'msstore') { throw 'terminal result identity mismatch' }
  $result | ConvertTo-Json -Depth 8 -Compress
  exit 0
}
if (-not $task) {
  [pscustomobject]@{ SchemaVersion=1; Status='runner-missing'; Terminal=$false; InvocationId=$InvocationId } | ConvertTo-Json -Compress
  exit 0
}
[pscustomobject]@{ SchemaVersion=1; Status='pending'; Terminal=$false; InvocationId=$InvocationId; TaskState=[string]$task.State; LastTaskResult=if($taskInfo){[int]$taskInfo.LastTaskResult}else{$null} } | ConvertTo-Json -Compress
