param(
  [Parameter(Mandatory=$true)][string]$Binary,
  [Parameter(Mandatory=$true)][string]$InitialFront,
  [Parameter(Mandatory=$true)][scriptblock]$SendControl,
  [Parameter(Mandatory=$true)][scriptblock]$ReceiverMarkedCount,
  [Parameter(Mandatory=$true)][scriptblock]$IcloudSentCount,
  [Parameter(Mandatory=$true)][scriptblock]$PrivateDraft,
  [Parameter(Mandatory=$true)][scriptblock]$Recipient,
  [Parameter(Mandatory=$true)][scriptblock]$Subject,
  [string]$ComposerName = "Message",
  [ValidateSet('iCloud')][string]$App = 'iCloud'
)
$ErrorActionPreference = 'Stop'
$steps = @('empty-readback','marked-paste','exact-readback','clear')

function Read-ExactState {
  $draft = [string](& $PrivateDraft)
  if ([string]::IsNullOrEmpty($draft)) { throw 'private draft probe returned no exact value' }
  [pscustomobject]@{
    ReceiverMarked = [int](& $ReceiverMarkedCount)
    IcloudSent = [int](& $IcloudSentCount)
    PrivateDraft = $draft
    Recipient = [string](& $Recipient)
    Subject = [string](& $Subject)
  }
}

$before = Read-ExactState
if ($before.ReceiverMarked -ne 0 -or $before.IcloudSent -ne 0) {
  throw "fresh control requires receiver_marked=0 and icloud_sent=0; got $($before.ReceiverMarked),$($before.IcloudSent)"
}
& $SendControl
$control = Read-ExactState
if ($control.ReceiverMarked -ne 1 -or $control.IcloudSent -ne 1) {
  throw "live iCloud control must produce receiver_marked=1 and icloud_sent=1; got $($control.ReceiverMarked),$($control.IcloudSent)"
}
if ($control.PrivateDraft -cne $before.PrivateDraft -or $control.Recipient -cne $before.Recipient -or $control.Subject -cne $before.Subject) {
  throw 'live control changed the exact private draft, recipient, or subject'
}
Write-Output 'TASK3600_CONTROL receiver_marked=0->1 icloud_sent=0->1'

foreach ($step in $steps) {
  $out = Join-Path $env:TEMP "task3600-$step-$PID.out"
  $err = Join-Path $env:TEMP "task3600-$step-$PID.err"
  $mark = "OSL-3600-$step-$([guid]::NewGuid().ToString('N'))"
  $p = Start-Process -FilePath $Binary -ArgumentList @('--initial-front',$InitialFront,'--app','iCloud','--composer-name',$ComposerName,'--text',$mark,'--private-canary',$control.PrivateDraft,'--pause-after-step',$step) -RedirectStandardOutput $out -RedirectStandardError $err -PassThru
  $deadline = [DateTime]::UtcNow.AddSeconds(15); $pause = $null
  while ([DateTime]::UtcNow -lt $deadline -and -not $pause) {
    Start-Sleep -Milliseconds 50
    if (Test-Path $out) { $pause = Get-Content $out | Where-Object { $_ -like 'TASK3585_PAUSED provider=iCloud *' } | Select-Object -Last 1 }
  }
  if (-not $pause -or $pause -notmatch 'provider=iCloud pid=(\d+) step=(.+)$' -or $Matches[2] -cne $step) { throw "no exact iCloud pause at $step" }
  Stop-Process -Id ([int]$Matches[1]) -Force
  $p.WaitForExit()
  $combined = (Get-Content $out -Raw) + (Get-Content $err -Raw)
  if ($p.ExitCode -eq 0 -or $combined -notmatch 'covers_sent=0' -or $combined -notmatch 'iCloud closed during' -or $combined -notmatch 'Retry placement in iCloud') { throw "unsafe iCloud close result at ${step}: $combined" }
  $after = Read-ExactState
  if ($after.ReceiverMarked -ne 1 -or $after.IcloudSent -ne 1) { throw "iCloud close at $step changed counts to receiver_marked=$($after.ReceiverMarked) icloud_sent=$($after.IcloudSent)" }
  if ($after.PrivateDraft -cne $control.PrivateDraft -or $after.Recipient -cne $control.Recipient -or $after.Subject -cne $control.Subject) { throw "iCloud close at $step changed the exact private draft, recipient, or subject" }
  Write-Output "TASK3600 step=$step receiver_marked=1 icloud_sent=1 covers_sent=0 private_draft=exact recipient=before subject=before retry=iCloud"
}
Write-Output 'TASK3600_SUMMARY control_receiver_marked=0->1 control_icloud_sent=0->1 close_attempts=4 receiver_marked_after_each=1 icloud_sent_after_each=1'
