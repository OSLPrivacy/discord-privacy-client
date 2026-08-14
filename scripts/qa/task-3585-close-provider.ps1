param(
  [Parameter(Mandatory=$true)][string]$Binary,
  [Parameter(Mandatory=$true)][string]$App,
  [Parameter(Mandatory=$true)][string]$InitialFront,
  [string]$ComposerName = "Message"
)
$ErrorActionPreference = 'Stop'
$steps = @('empty-readback','marked-paste','exact-readback','clear')
$privateCanary = 'TASK3585-PRIVATE-DRAFT-CANARY'
$runs = @()
foreach ($step in $steps) {
  $out = Join-Path $env:TEMP "task3585-$step-$PID.out"
  $err = Join-Path $env:TEMP "task3585-$step-$PID.err"
  $mark = "OSL-3585-$step-$([guid]::NewGuid().ToString('N'))"
  $p = Start-Process -FilePath $Binary -ArgumentList @('--initial-front',$InitialFront,'--app',$App,'--composer-name',$ComposerName,'--text',$mark,'--private-canary',$privateCanary,'--pause-after-step',$step) -RedirectStandardOutput $out -RedirectStandardError $err -PassThru
  $deadline = [DateTime]::UtcNow.AddSeconds(15); $pause = $null
  while ([DateTime]::UtcNow -lt $deadline -and -not $pause) { Start-Sleep -Milliseconds 50; if (Test-Path $out) { $pause = Get-Content $out | Where-Object { $_ -like 'TASK3585_PAUSED *' } | Select-Object -Last 1 } }
  if (-not $pause) { throw "no runtime pause at $step; stdout=$(Get-Content $out -Raw -ErrorAction SilentlyContinue) stderr=$(Get-Content $err -Raw -ErrorAction SilentlyContinue)" }
  if ($pause -notmatch 'provider=(.+) pid=(\d+) step=(.+)$') { throw "unparseable pause: $pause" }
  $provider = $Matches[1]; $pidValue = [int]$Matches[2]
  $before = tasklist.exe /FI "PID eq $pidValue" /FO CSV /NH
  if ($before -notmatch ('"' + $pidValue + '"')) { throw "tasklist.exe did not prove $provider PID $pidValue live" }
  Stop-Process -Id $pidValue -Force
  Start-Sleep -Milliseconds 150
  $after = tasklist.exe /FI "PID eq $pidValue" /FO CSV /NH
  if ($after -match ('"' + $pidValue + '"')) { throw "tasklist.exe still found stopped $provider PID $pidValue" }
  $p.WaitForExit(); $combined = (Get-Content $out -Raw) + (Get-Content $err -Raw)
  if ($p.ExitCode -eq 0 -or $combined -notmatch 'covers_sent=0' -or $combined -notmatch 'private_draft_fingerprint=' -or $combined -notmatch [regex]::Escape($provider) -or $combined -notmatch 'Your message was not sent anywhere.') { throw "unsafe close result at $step: $combined" }
  $runs += "step=$step provider=$provider pid=$pidValue tasklist_before=true tasklist_after=false covers_sent=0"
}
$runs | ForEach-Object { Write-Output "TASK3585 $_" }
Write-Output 'TASK3585_SUMMARY close_attempts=4 live_controls_required=1 provider_process_fixtures=false saved_step_reports=false'
