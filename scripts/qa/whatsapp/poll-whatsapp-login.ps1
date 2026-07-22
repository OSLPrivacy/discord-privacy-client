param(
  [Parameter(Mandatory = $true)]
  [ValidatePattern('^[a-z0-9][a-z0-9-]{7,63}$')]
  [string]$InvocationId
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$invocationRoot = Join-Path 'C:\ProgramData\OSL-QA\whatsapp-login-v1' $InvocationId
$requestPath = Join-Path $invocationRoot 'request.clixml'
$wrapperPath = Join-Path $invocationRoot 'run-interactive.ps1'
$resultPath = Join-Path $invocationRoot 'result.json'
if (-not (Test-Path -LiteralPath $requestPath -PathType Leaf) -or
    -not (Test-Path -LiteralPath $wrapperPath -PathType Leaf)) {
  throw 'unknown or incomplete WhatsApp login invocation'
}
$request = Import-Clixml -LiteralPath $requestPath
$wrapperSha256 = (Get-FileHash -LiteralPath $wrapperPath -Algorithm SHA256).Hash.ToLowerInvariant()
if ($request.InvocationId -cne $InvocationId -or $request.WrapperSha256 -cne $wrapperSha256) {
  throw 'WhatsApp login invocation identity mismatch'
}
if (-not (Test-Path -LiteralPath $resultPath -PathType Leaf)) {
  [pscustomobject]@{ InvocationId = $InvocationId; Terminal = $false; Status = 'running' } |
    ConvertTo-Json -Compress
  exit 0
}
$result = Get-Content -LiteralPath $resultPath -Raw | ConvertFrom-Json
if ($result.InvocationId -cne $InvocationId -or $result.PackageFamily -cne '5319275A.WhatsAppDesktop_cv1g1gvanyjgm' -or
    $result.Terminal -ne $true -or $result.BrowserFallbackUsed -ne $false -or $result.ProfileRead -ne $false -or
    $result.LinkStateTouched -ne $false -or $result.PrivateApiUsed -ne $false -or $result.ProcessesTerminated -ne 0) {
  throw 'WhatsApp login result violates the fixed safety contract'
}
$result | ConvertTo-Json -Depth 5 -Compress
