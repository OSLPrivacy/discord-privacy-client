<#
.SYNOPSIS
    Cross-installs the two QA identities' public offers so each instance will
    pair with the other on its next start. Copies two files and nothing else.
    Emits %TEMP%\osl-p2p-pair.json.

.DESCRIPTION
    WHAT THE PRODUCT ALREADY DOES
      A `discord-qa-shell` build, on every start, runs
      publish_and_consume_pairing (apps/osl-hub/src/main.rs:5913 ->
      apps/osl-hub/src/discord_qa_identity.rs:261):
        1. exports its own friend code and writes discord-qa-offer.v1.json
        2. if discord-qa-peer-offer.v1.json is present, adds that friend code,
           re-derives the osl_user_id and safety number, REFUSES if either
           disagrees with the signed code, verifies the safety number is stable,
           and only then writes discord-qa-pairing-status.v1.json.
      So pairing is a file exchange the product performs for itself. This script
      does step 0: put each side's offer where the other side will look.

    WHAT THIS SCRIPT DOES NOT DO
      * It does not create an identity. Both offers must already exist, which
        means both instances have already been started once by the owner.
      * It does not contact the keyserver, the cipher store or Discord.
      * It does not modify an offer. The bytes are copied verbatim, and the
        sha256 of source and destination is recorded so a partial copy is
        visible rather than silent.
      * It never prints a friend code or a safety number. Only the opaque
        osl_user_id is compared, and only its equality is reported.

    WHY BOTH INSTANCES MUST BE CLOSED
      The peer offer is consumed during setup, before the window exists. Writing
      it under a running instance does nothing at all until the next start, and
      leaves a rig that looks paired on disk but is not paired in memory -- the
      exact kind of half-state that produces a confident wrong diagnosis. So
      this script refuses while either instance is running.
#>

[CmdletBinding()]
param(
    [string]$BundleA = 'org.oslprivacy.hub',
    [Parameter(Mandatory = $true)][string]$BundleB,
    [string]$JsonOut = (Join-Path $env:TEMP 'osl-p2p-pair.json'),
    [switch]$Quiet
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Off

$here = Split-Path -Parent $MyInvocation.MyCommand.Path
. (Join-Path $here 'osl-p2p-win32.ps1')

$runStart = Get-Date
$steps = @()
function Say { param([string]$M, [string]$C = 'Gray') if (-not $Quiet) { Write-Host $M -ForegroundColor $C } }
function Add-S { param([string]$N, [string]$R, [string]$D)
    $script:steps += [pscustomobject][ordered]@{ step = $N; result = $R; detail = $D }
    $c = switch ($R) { 'ok' { 'Green' } 'failed' { 'Red' } default { 'Yellow' } }
    Say ('[{0,-22}] {1,-8} {2}' -f $N, $R, $D) $c
}
function Finish {
    param([string]$Verdict, [string]$Diagnosis, [string]$Remedy, $Extra = $null)
    $p = [ordered]@{
        schemaVersion = 1; tool = 'osl-p2p-pair'; runStartedAt = $runStart.ToString('o')
        overall = [ordered]@{ verdict = $Verdict; diagnosis = $Diagnosis; remedy = $Remedy }
        steps = $script:steps
        diffKey = ($Verdict.ToUpper() + ':' + $Diagnosis)
    }
    if ($Extra) { foreach ($k in $Extra.Keys) { $p[$k] = $Extra[$k] } }
    $p | ConvertTo-Json -Depth 10 | Out-File -LiteralPath $JsonOut -Encoding utf8
    $c = switch ($Verdict) { 'ok' { 'Green' } 'failed' { 'Red' } default { 'Yellow' } }
    Say ''
    Say ('PAIR VERDICT: {0} -- {1}' -f $Verdict.ToUpper(), $Diagnosis) $c
    if ($Remedy) { Say ('remedy: {0}' -f $Remedy) 'Cyan' }
    Say ('JSON: {0}' -f $JsonOut) 'DarkGray'
    if ($Verdict -eq 'ok') { exit 0 } elseif ($Verdict -eq 'failed') { exit 1 } else { exit 2 }
}

function Resolve-Root {
    param([string]$Bundle, [string]$Side)
    $base = Join-Path $env:APPDATA $Bundle
    $cands = @((Join-Path $base 'discord-qa-shell-v1\osl-core'), (Join-Path $base 'osl-core'))
    $live = @($cands | Where-Object { Test-Path -LiteralPath (Join-Path $_ 'discord-qa-offer.v1.json') })
    if ($live.Count -eq 1) { return $live[0] }
    if ($live.Count -gt 1) {
        Add-S ('resolve/' + $Side) 'failed' ('AMBIGUOUS: both candidate roots hold discord-qa-offer.v1.json ({0}).' -f ($live -join ' AND '))
        Finish 'blocked' ('Instance {0}''s profile root is ambiguous.' -f $Side) 'Remove the stale profile so exactly one root holds discord-qa-offer.v1.json.'
    }
    Add-S ('resolve/' + $Side) 'failed' ('No discord-qa-offer.v1.json under {0}.' -f ($cands -join ' | '))
    Finish 'blocked' `
        ('Instance {0} ({1}) has never published an offer, so there is nothing to exchange. Every discord-qa-shell build writes discord-qa-offer.v1.json on startup (apps/osl-hub/src/discord_qa_identity.rs:261), so either this instance has never been started, or it was not built with --features discord-qa-shell.' -f $Side, $Bundle) `
        'Start the instance once, let it finish setup, close it, and re-run.'
}

Say '=== OSL QA pairing: offer exchange ===' 'Cyan'

if ($BundleA -eq $BundleB) {
    Add-S 'gate/distinct' 'failed' ('Both bundles are "{0}".' -f $BundleA)
    Finish 'blocked' 'The two identifiers are identical, so there is one identity and nothing to pair.' 'Build B with a different identifier.'
}

# Refuse while either instance is running: the peer offer is consumed during
# setup, so a copy made now would take effect at an unpredictable later restart.
$map = Get-P2PBundleMap
$runA = @($map.Keys | Where-Object { $map[$_] -eq $BundleA })
$runB = @($map.Keys | Where-Object { $map[$_] -eq $BundleB })
if ($runA.Count -gt 0 -or $runB.Count -gt 0) {
    $who = @()
    if ($runA.Count -gt 0) { $who += ('A ({0}, pid {1})' -f $BundleA, ($runA -join ',')) }
    if ($runB.Count -gt 0) { $who += ('B ({0}, pid {1})' -f $BundleB, ($runB -join ',')) }
    Add-S 'gate/both-closed' 'failed' ('Still running: {0}.' -f ($who -join ' and '))
    Finish 'blocked' `
        ('{0} is still running. The peer offer is consumed during setup (apps/osl-hub/src/main.rs:5913), before the window exists, so copying it under a live process changes nothing now and takes effect at some unpredictable later start -- a rig that looks paired on disk and is not paired in memory.' -f ($who -join ' and ')) `
        'Close both instances, re-run this script, then start both again.'
}
Add-S 'gate/both-closed' 'ok' 'Neither instance is running (no -sic marker for either identifier), so an offer copied now will be consumed at the next start.'

$rootA = Resolve-Root -Bundle $BundleA -Side 'A'
$rootB = Resolve-Root -Bundle $BundleB -Side 'B'
Add-S 'resolve/A' 'ok' $rootA
Add-S 'resolve/B' 'ok' $rootB
if ($rootA -eq $rootB) {
    Add-S 'gate/separate-roots' 'failed' ('Both resolved to {0}.' -f $rootA)
    Finish 'blocked' 'Both bundles resolved to the same profile root, so they are one identity.' 'Confirm the identifiers really differ.'
}

$offA = Join-Path $rootA 'discord-qa-offer.v1.json'
$offB = Join-Path $rootB 'discord-qa-offer.v1.json'
$jA = Get-Content -LiteralPath $offA -Raw | ConvertFrom-Json
$jB = Get-Content -LiteralPath $offB -Raw | ConvertFrom-Json
if (-not $jA.osl_user_id -or -not $jB.osl_user_id) {
    Add-S 'gate/offers-readable' 'failed' 'At least one offer has no osl_user_id.'
    Finish 'blocked' 'An offer file is present but does not carry an osl_user_id, so it cannot be validated.' 'Delete it and restart that instance so the bootstrap rewrites it.'
}
if ($jA.osl_user_id -eq $jB.osl_user_id) {
    Add-S 'gate/two-identities' 'failed' 'Both offers carry the same osl_user_id.'
    Finish 'failed' `
        'BOTH INSTANCES PUBLISHED THE SAME osl_user_id. They are one identity wearing two processes -- almost certainly because the two profile roots are not actually separate, or because one profile was copied from the other. Pairing them would produce a rig in which every "cross-identity" result is a self-test, which is the most dangerous false green available here.' `
        'Delete instance B''s profile root entirely, start B once so it creates a fresh identity, then re-run.'
}
Add-S 'gate/two-identities' 'ok' 'The two offers carry different osl_user_ids, so there really are two identities. (Only the opaque IDs were compared; no friend code or safety number is read out.)'

# The copy. Verbatim bytes; sha256 of source and destination compared so a
# partial or intercepted write is visible rather than silent.
$dstAtB = Join-Path $rootB 'discord-qa-peer-offer.v1.json'
$dstBtA = Join-Path $rootA 'discord-qa-peer-offer.v1.json'
$copies = @()
foreach ($pair in @(@{ src = $offA; dst = $dstAtB; what = 'A''s offer into B' }, @{ src = $offB; dst = $dstBtA; what = 'B''s offer into A' })) {
    Copy-Item -LiteralPath $pair.src -Destination $pair.dst -Force -ErrorAction Stop
    $hs = (Get-FileHash -LiteralPath $pair.src -Algorithm SHA256).Hash
    $hd = (Get-FileHash -LiteralPath $pair.dst -Algorithm SHA256).Hash
    $ok = ($hs -eq $hd)
    $copies += [ordered]@{ what = $pair.what; source = $pair.src; destination = $pair.dst; sha256 = $hs; identical = $ok }
    Add-S 'copy' $(if ($ok) { 'ok' } else { 'failed' }) ('{0}: {1} -> {2} (sha256 match: {3})' -f $pair.what, $pair.src, $pair.dst, $ok)
    if (-not $ok) {
        Finish 'failed' ('The copy of {0} did not land byte-identically.' -f $pair.what) 'Check for a file lock or an antivirus filter on the destination and re-run.'
    }
}

Finish 'ok' `
    'Both peer offers are installed. Neither instance is running, so each will consume its peer offer at its next start, add the friend code, re-derive and check the osl_user_id and safety number against the signed code, and write discord-qa-pairing-status.v1.json only if all of that holds.' `
    'Start instance A, then run scripts\qa\osl-launch-instance-b.ps1, then scripts\qa\osl-p2p-loop.ps1. The harness''s mutual-pairing gate will confirm the result.' `
    @{ copies = $copies; instanceA = [ordered]@{ bundle = $BundleA; root = $rootA }; instanceB = [ordered]@{ bundle = $BundleB; root = $rootB } }
