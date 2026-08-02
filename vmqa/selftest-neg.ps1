[CmdletBinding()]
param(
  [string]$ResultsDirectory = (Join-Path $PSScriptRoot "results")
)

$ErrorActionPreference = "Stop"
New-Item -ItemType Directory -Force -Path $ResultsDirectory | Out-Null

# This is intentionally three independent bad observations. Do not collapse
# them into a generic failure: a driver that cannot distinguish executable,
# route and DOM evidence is not a useful release harness.
$checks = @(
  [ordered]@{ id = "wrong_app"; expected = "C:\\__vmqa_deliberately_absent__.exe"; actual = $false },
  [ordered]@{ id = "wrong_route"; expected = "/__vmqa_deliberately_absent__"; actual = $false },
  [ordered]@{ id = "absent_element"; expected = "#__vmqa_deliberately_absent__"; actual = $false }
)
$failures = foreach ($check in $checks) {
  [ordered]@{ id = $check.id; status = "FAILURE"; expected = $check.expected; observed = "absent" }
}
if ($failures.Count -ne 3 -or @($failures.id | Select-Object -Unique).Count -ne 3) {
  throw "VMQA_SELFTEST_NEG_VACUOUS: each deliberate failure must be distinct"
}
$result = [ordered]@{ schema = "vmqa-negative-selftest/v1"; overall = "FAILURE"; failures = $failures }
$path = Join-Path $ResultsDirectory "selftest-neg.json"
$result | ConvertTo-Json -Depth 5 | Set-Content -NoNewline -Encoding utf8 $path
Write-Output $path
exit 1
