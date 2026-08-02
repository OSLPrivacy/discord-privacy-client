$script = Join-Path $PSScriptRoot "selftest-neg.ps1"
$resultDir = Join-Path ([IO.Path]::GetTempPath()) ("vmqa-neg-" + [guid]::NewGuid())
try {
  & $script -ResultsDirectory $resultDir
  if ($LASTEXITCODE -ne 1) { throw "negative control did not fail" }
  $result = Get-Content (Join-Path $resultDir "selftest-neg.json") -Raw | ConvertFrom-Json
  if ($result.overall -ne "FAILURE" -or @($result.failures).Count -ne 3) { throw "negative result is not measurable" }
  if (@($result.failures.id | Select-Object -Unique).Count -ne 3) { throw "failure types were merged" }
} finally { Remove-Item -Recurse -Force $resultDir -ErrorAction SilentlyContinue }
