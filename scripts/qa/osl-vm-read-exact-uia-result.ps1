param(
  [Parameter(Mandatory = $true)]
  [ValidatePattern('^[a-z0-9][a-z0-9-]{7,63}$')]
  [string]$InvocationId,

  [ValidateRange(1, 120)]
  [int]$WaitSeconds = 75
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$root = 'C:\ProgramData\OSL-QA\discord-uia'
$path = Join-Path (Join-Path $root $InvocationId) 'result.json'
$deadline = [DateTime]::UtcNow.AddSeconds($WaitSeconds)

while ((-not (Test-Path -LiteralPath $path)) -and ([DateTime]::UtcNow -lt $deadline)) {
  Start-Sleep -Milliseconds 500
}

if (-not (Test-Path -LiteralPath $path)) {
  throw 'exact-result-timeout'
}

Get-Content -LiteralPath $path -Raw
