<#
.SYNOPSIS
    Cross-VM sibling of osl-p2p-pair.ps1. Moves one side's pairing offer to the
    other side over the existing Azure Blob rendezvous (container `vmqa` in
    `osltestartifactsa7d5`, the same one scripts/vmqa/** already uses), because
    the crypto pair's two identities live on two separate VMs
    (OSL-Azure-Client-1 / -2) with two separate %APPDATA%s that osl-p2p-pair.ps1's
    plain Copy-Item cannot bridge.

.DESCRIPTION
    DOES NOT REPLACE osl-p2p-pair.ps1. That script stays correct and is still
    what you run when both identities are reachable from one filesystem (e.g.
    same-host QA per docs/qa/two-identity-p2p-verification.md). This script only
    ever moves bytes between two machines; the actual pairing decision --
    deriving osl_user_id and safety_number from the signed offer and refusing on
    mismatch -- happens in the product itself, in publish_and_consume_pairing
    (apps/osl-hub/src/main.rs:5913 -> discord_qa_identity.rs:261), exactly once,
    at the NEXT START of the instance that receives a peer offer. That is true
    whether the peer offer arrived via Copy-Item on one host or via this script
    across two. THE OFFER IS NOT TRUSTED BECAUSE IT CAME FROM THIS BLOB. It is
    trusted only because the product re-derives and checks it on consumption --
    identical code path, identical distrust, regardless of transport.

    WHAT ACTUALLY AUTHORIZES A BLOB WRITE HERE -- READ THIS BEFORE ASSUMING
    "NOTHING DOES":
      Shared-key auth is OFF on this storage account (azure-vm-qa-workflow.md
      §"3. Drive it by blob rendezvous"). Something still authorizes every
      write: Entra ID RBAC. Each crypto VM (OSL-Azure-Client-1 and -2) carries a
      system-assigned managed identity; this script exchanges that identity for
      an OAuth token over IMDS (http://169.254.169.254/metadata/identity/...,
      link-local, unspoofable off-Azure) and presents it as a Bearer token on
      every blob request -- the exact mechanism scripts/vmqa/vmqa-agent.ps1
      already proves works from these machines (Get-StorageToken /
      Invoke-BlobRequest there; duplicated here in miniature rather than
      dot-sourcing that file, because it is an agent daemon with its own
      top-level param() and main loop, not a library). The role granted is
      `Storage Blob Data Contributor`, and it is scoped to the WHOLE `vmqa`
      CONTAINER, not to a path prefix inside it (azure-vm-qa-workflow.md:118-119
      names the container as the scope; Azure RBAC role assignments do not
      narrow to a blob-name prefix without an ABAC condition, and none is
      documented here). LOUD FINDING: this means either crypto VM's identity
      can read or write ANY path under vmqa/ -- runs/, builds/, agent/, and
      both sides of pairing/ -- not just its own. Nothing in Azure stops
      Client-1 from overwriting the object Client-2 is about to push, or from
      reading a path that "belongs" to the peer. The path-naming convention
      below (pairing/<runId>/<side>-offer.json) is a CONVENTION, not an ACL.
      This is exactly why the offer must never be trusted merely for having
      arrived at the expected path from the expected side -- and it is exactly
      why the product's own re-derivation on consumption is the only real
      authority, never this script's transport plumbing.

      NOTE ON THE RUNBOOK'S ORIGINAL SUGGESTION: two-identity-proof-runbook.md
      §3 sketches `az storage blob upload --auth-mode login`. That is the HOST
      pattern (scripts/vmqa/vmqa-share.sh, run from WSL under the operator's
      own interactively-logged-in `az` session). It is not available on the
      crypto VMs themselves: nothing here installs or configures `az` CLI on
      OSL-Azure-Client-1/-2, and the whole point of the fleet's IMDS design
      (azure-vm-qa-workflow.md:113-116) is that the VM-side transport needs no
      interactive login and no stored credential at all. This script therefore
      uses the VM-side IMDS+REST pattern, not `az`, and that discrepancy from
      the runbook sketch is intentional -- flagged here rather than silently
      "fixed" without comment.

    REPLAY / STALENESS REFUSAL -- what "already consumed" means, concretely:
      The product itself, on a successful pairing, writes
      discord-qa-pairing-status.v1.json containing (among other fields)
      `peer_offer_sha256` -- the sha256 of the exact peer-offer bytes it
      consumed (discord_qa_identity.rs:54-61). -Pull reads that file, if
      present, BEFORE writing anything, and compares its `peer_offer_sha256`
      against the sha256 of the bytes it just downloaded (verified in turn
      against the blob's own `.ready` sentinel, same pattern as
      scripts/vmqa/vmqa-share.sh's put-atomic/get-atomic and
      builds/<sha256>/...):
        - identical sha256 AND `verified == true`  -> REFUSED. This exact offer
          was already consumed and the pairing it produced was already
          verified; re-installing it is a no-op at best and a confusing
          "did it re-pair?" question at worst. The script exits non-zero and
          writes nothing.
        - a locally-resolved discord-qa-peer-offer.v1.json already exists with
          this same sha256 but no pairing-status yet (not started since) ->
          treated as already-installed/idempotent-ok; the script does not
          rewrite it and says so plainly. Not an error, not a fresh pull.
        - different sha256 (peer republished, e.g. after a reset) -> a
          genuinely new offer; proceeds, and the receipt says so.
      This is why a re-run cannot double-pair or silently reuse a stale file:
      the check is against the PRODUCT's own consumption record, not a
      transport-layer guess.

    WHY BOTH INSTANCES MUST BE CLOSED (identical reasoning to osl-p2p-pair.ps1):
      the peer offer is consumed at startup; writing it under a live process
      changes nothing until an unpredictable later restart. -Push and -Pull
      both refuse while THIS side's instance is running. Neither this script
      nor osl-p2p-pair.ps1 can see whether the PEER's instance is closed --
      that remains the operator's responsibility on the other RDP session; see
      docs/qa/cross-vm-pairing.md for exactly what stays unproven here.

.PARAMETER Side
    Which identity this VM hosts: 'A' or 'B'. Selects the local bundle default
    and the blob path this VM publishes under with -Push.

.PARAMETER PeerSide
    Required with -Pull. The side whose offer to fetch. Must differ from
    -Side; pulling your own published offer as "the peer's" is refused.

.PARAMETER RunId
    Blob path segment shared by both invocations of this script across the two
    VMs: pairing/<RunId>/<side>-offer.json. Defaults to a single well-known
    slot ('exchange') matching the fleet's one-crypto-pair-at-a-time design,
    so two operators typing the same default need no separate coordination.
    Pass an explicit value to start a clean slate for a retry without
    disturbing -- or being confused by -- a prior attempt's objects.
#>

[CmdletBinding()]
param(
    [ValidateSet('A', 'B')][string]$Side,
    [ValidateSet('A', 'B')][string]$PeerSide,
    [string]$RunId = 'exchange',
    [string]$Bundle = 'org.oslprivacy.hub',
    [switch]$Push,
    [switch]$Pull,
    [int]$TimeoutSec = 300,
    [string]$JsonOut = (Join-Path ([IO.Path]::GetTempPath()) 'osl-p2p-pair-blob.json'),
    [switch]$Quiet,
    # Test-only filesystem implementation of the blob API.  It exists so the
    # transport's fail-closed behavior is executable without Azure credentials.
    [string]$TestBlobRoot = '',
    [switch]$RunScriptSelfTests
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Off

$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$script:PairBlobScriptPath = $PSCommandPath
if ([string]::IsNullOrWhiteSpace($TestBlobRoot) -and -not $RunScriptSelfTests) {
    . (Join-Path $here 'osl-p2p-win32.ps1')
}

$StorageAccount = 'osltestartifactsa7d5'
$BlobContainer = 'vmqa'
$BlobContainerUri = "https://$StorageAccount.blob.core.windows.net/$BlobContainer"
$AllowedVms = @(
    'OSL-Azure-Client-1', 'OSL-Azure-Client-2',
    'OSL-Independent-Client-1', 'OSL-Independent-Client-2',
    'OSL-WhatsApp-Client-1', 'OSL-WhatsApp-Client-2',
    'OSL-Telegram-QA-1', 'OSL-Telegram-QA-2',
    'OSL-Signal-Client-1', 'OSL-Signal-Client-2'
)
$script:StorageToken = $null
$script:StorageTokenExpiresUtc = [datetime]::MinValue

$runStart = Get-Date
$steps = @()
function Say { param([string]$M, [string]$C = 'Gray') if (-not $Quiet) { Write-Host $M -ForegroundColor $C } }
function Add-S { param([string]$N, [string]$R, [string]$D)
    $script:steps += [pscustomobject][ordered]@{ step = $N; result = $R; detail = $D }
    $c = switch ($R) { 'ok' { 'Green' } 'failed' { 'Red' } default { 'Yellow' } }
    Say ('[{0,-24}] {1,-8} {2}' -f $N, $R, $D) $c
}
function Finish {
    param([string]$Verdict, [string]$Diagnosis, [string]$Remedy, $Extra = $null)
    $p = [ordered]@{
        schemaVersion = 1; tool = 'osl-p2p-pair-blob'; runStartedAt = $runStart.ToString('o')
        side = $Side; peerSide = $PeerSide; runId = $RunId
        overall = [ordered]@{ verdict = $Verdict; diagnosis = $Diagnosis; remedy = $Remedy }
        steps = $script:steps
        diffKey = ($Verdict.ToUpper() + ':' + $Diagnosis)
    }
    if ($Extra) { foreach ($k in $Extra.Keys) { $p[$k] = $Extra[$k] } }
    $p | ConvertTo-Json -Depth 10 | Out-File -LiteralPath $JsonOut -Encoding utf8
    $c = switch ($Verdict) { 'ok' { 'Green' } 'failed' { 'Red' } default { 'Yellow' } }
    Say ''
    Say ('PAIR-BLOB VERDICT: {0} -- {1}' -f $Verdict.ToUpper(), $Diagnosis) $c
    if ($Remedy) { Say ('remedy: {0}' -f $Remedy) 'Cyan' }
    Say ('JSON: {0}' -f $JsonOut) 'DarkGray'
    if ($Verdict -eq 'ok') { exit 0 } elseif ($Verdict -eq 'failed') { exit 1 } else { exit 2 }
}

# --- IMDS + raw REST blob transport, mirroring scripts/vmqa/vmqa-agent.ps1 in miniature. ---
# Duplicated rather than dot-sourced: vmqa-agent.ps1 is a daemon with its own param() block and
# main loop, not a library, and pulling it in would run its own IMDS allow-list guard, log
# rotation and heartbeat plumbing for no benefit here. The IMDS-allow-list guard itself IS
# duplicated below on purpose -- it is the one cheap thing standing between "this only runs on a
# fleet VM with the granted role" and "this runs anywhere and fails opaquely on 403".
if (-not [string]::IsNullOrWhiteSpace($TestBlobRoot) -or $RunScriptSelfTests) {
    Add-S 'gate/on-fleet-vm' 'ok' 'Test-only filesystem blob transport selected.'
} else {
try {
    $imdsName = ([string](Invoke-RestMethod -Method Get -TimeoutSec 5 `
        -Uri 'http://169.254.169.254/metadata/instance/compute/name?api-version=2021-02-01&format=text' `
        -Headers @{ Metadata = 'true' })).Trim()
} catch {
    Add-S 'gate/on-fleet-vm' 'failed' 'Azure IMDS did not answer; this is not a fleet VM.'
    Finish 'blocked' 'This script only runs on a VM with the fleet''s managed identity (IMDS-verified).' 'Run this on OSL-Azure-Client-1 or -2, not the host.'
}
if ($AllowedVms -notcontains $imdsName) {
    Add-S 'gate/on-fleet-vm' 'failed' ("IMDS reports '{0}', not in the fleet allow-list." -f $imdsName)
    Finish 'blocked' 'This VM is not in the fleet allow-list, so it was not granted the vmqa container role.' 'Run this on OSL-Azure-Client-1 or -2.'
}
Add-S 'gate/on-fleet-vm' 'ok' ('IMDS confirms this is {0}.' -f $imdsName)
}

function Get-StorageToken {
    if (-not [string]::IsNullOrWhiteSpace($script:StorageToken) -and
        $script:StorageTokenExpiresUtc -gt [datetime]::UtcNow.AddMinutes(5)) { return $script:StorageToken }
    $uri = 'http://169.254.169.254/metadata/identity/oauth2/token?api-version=2019-08-01&resource=https%3A%2F%2Fstorage.azure.com%2F'
    $t = Invoke-RestMethod -Method Get -Uri $uri -Headers @{ Metadata = 'true' } -TimeoutSec 10
    $script:StorageToken = [string]$t.access_token
    $script:StorageTokenExpiresUtc = [DateTimeOffset]::FromUnixTimeSeconds([int64]$t.expires_on).UtcDateTime
    return $script:StorageToken
}
function ConvertTo-BlobPath { param([string]$Name) return (($Name -split '/') | ForEach-Object { [uri]::EscapeDataString($_) }) -join '/' }
function Get-HttpStatusCode {
    param($ErrorRecord)
    $ex = $ErrorRecord.Exception
    while ($null -ne $ex) {
        $rp = $ex.PSObject.Properties['Response']
        if ($null -ne $rp -and $null -ne $rp.Value) {
            $sp = $rp.Value.PSObject.Properties['StatusCode']
            if ($null -ne $sp -and $null -ne $sp.Value) {
                if ($sp.Value -is [int]) { return [int]$sp.Value }
                $ev = $sp.Value.PSObject.Properties['value__']
                if ($null -ne $ev) { return [int]$ev.Value }
            }
        }
        $ex = $ex.InnerException
    }
    return $null
}
function Invoke-Blob {
    param([ValidateSet('GET', 'HEAD', 'PUT')][string]$Method, [string]$Name, [byte[]]$Body = $null, [string]$OutFile = '')
    if (-not [string]::IsNullOrWhiteSpace($TestBlobRoot)) {
        $path = Join-Path $TestBlobRoot (ConvertTo-BlobPath -Name $Name)
        switch ($Method) {
            'HEAD' { if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { throw [System.IO.FileNotFoundException]::new('404 test blob absent', $path) }; return }
            'GET' { if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { throw [System.IO.FileNotFoundException]::new('404 test blob absent', $path) }; if ($OutFile) { [IO.File]::WriteAllBytes($OutFile, [IO.File]::ReadAllBytes($path)) }; return }
            'PUT' { New-Item -ItemType Directory -Force -Path (Split-Path -Parent $path) | Out-Null; [IO.File]::WriteAllBytes($path, $Body); return }
        }
    }
    $headers = @{ Authorization = "Bearer $(Get-StorageToken)"; 'x-ms-version' = '2021-08-06' }
    $uri = $BlobContainerUri + '/' + (ConvertTo-BlobPath -Name $Name)
    $p = @{ Method = $Method; Uri = $uri; Headers = $headers; TimeoutSec = 60; ErrorAction = 'Stop'; UseBasicParsing = $true }
    if ($OutFile) { $p['OutFile'] = $OutFile }
    if ($null -ne $Body) { $p['Body'] = $Body; $p['ContentType'] = 'application/octet-stream'; $headers['x-ms-blob-type'] = 'BlockBlob' }
    return Invoke-WebRequest @p
}
function Test-BlobExists {
    param([string]$Name)
    if (-not [string]::IsNullOrWhiteSpace($TestBlobRoot)) {
        return Test-Path -LiteralPath (Join-Path $TestBlobRoot (ConvertTo-BlobPath -Name $Name)) -PathType Leaf
    }
    try { [void](Invoke-Blob -Method HEAD -Name $Name); return $true }
    catch { if ((Get-HttpStatusCode $_) -eq 404) { return $false }; throw }
}
function Get-BlobBytes {
    param([string]$Name)
    $tmp = [IO.Path]::GetTempFileName()
    try { [void](Invoke-Blob -Method GET -Name $Name -OutFile $tmp); return [IO.File]::ReadAllBytes($tmp) }
    finally { Remove-Item -LiteralPath $tmp -Force -ErrorAction SilentlyContinue }
}
function Put-BlobBytes { param([string]$Name, [byte[]]$Bytes) [void](Invoke-Blob -Method PUT -Name $Name -Body $Bytes) }

# --- Profile-root resolution, deliberately mirroring osl-p2p-pair.ps1's Resolve-Root byte-for-byte
# (not factored into a shared module: that script is the tested, in-production same-host path, and
# editing it to extract a shared function was judged riskier than a small, clearly-flagged
# duplication -- keep both in sync by hand if either changes). ---
function Resolve-Root {
    param([string]$Bundle)
    $base = Join-Path $env:APPDATA $Bundle
    $cands = @((Join-Path $base 'discord-qa-shell-v1\osl-core'), (Join-Path $base 'osl-core'))
    $live = @($cands | Where-Object { Test-Path -LiteralPath (Join-Path $_ 'discord-qa-offer.v1.json') })
    if ($live.Count -eq 1) { return $live[0] }
    if ($live.Count -gt 1) {
        Add-S 'resolve/local' 'failed' ('AMBIGUOUS: both candidate roots hold an offer ({0}).' -f ($live -join ' AND '))
        Finish 'blocked' 'This instance''s profile root is ambiguous.' 'Remove the stale profile so exactly one root holds discord-qa-offer.v1.json.'
    }
    Add-S 'resolve/local' 'failed' ('No discord-qa-offer.v1.json under {0}.' -f ($cands -join ' | '))
    Finish 'blocked' `
        'This instance has never published an offer. Every discord-qa-shell build writes discord-qa-offer.v1.json on startup (discord_qa_identity.rs:261).' `
        'Start the instance once, let it finish setup, close it, and re-run.'
}

function Confirm-LocalClosed {
    if (-not [string]::IsNullOrWhiteSpace($TestBlobRoot)) {
        Add-S 'gate/local-closed' 'ok' 'Test fixture has no live product process.'
        return
    }
    $map = Get-P2PBundleMap
    $running = @($map.Keys | Where-Object { $map[$_] -eq $Bundle })
    if ($running.Count -gt 0) {
        Add-S 'gate/local-closed' 'failed' ('Still running: {0} (pid {1}).' -f $Bundle, ($running -join ','))
        Finish 'blocked' `
            'This side is still running. The peer offer is consumed at startup, before the window exists, so writing it now takes effect only at some unpredictable later start.' `
            'Close this instance, re-run, then start it again.'
    }
    Add-S 'gate/local-closed' 'ok' ('No -sic marker for {0}; safe to proceed.' -f $Bundle)
}

function Assert-SelfTest {
    param([bool]$Condition, [string]$Message)
    if (-not $Condition) { throw $Message }
}

function Invoke-PairBlobSelfTestChild {
    param([string]$AppData, [string]$SideForTest, [string]$Mode, [string]$PeerForTest, [string]$RunForTest, [string]$BlobRoot, [string]$Receipt)
    $childArgs = @('-NoProfile', '-File', $script:PairBlobScriptPath, '-Side', $SideForTest, "-$Mode", '-RunId', $RunForTest, '-TestBlobRoot', $BlobRoot, '-JsonOut', $Receipt, '-Quiet')
    if ($PeerForTest) { $childArgs += @('-PeerSide', $PeerForTest) }
    $oldAppData = $env:APPDATA
    try { $env:APPDATA = $AppData; & (Get-Process -Id $PID).Path @childArgs | Out-Null; return $LASTEXITCODE }
    finally { $env:APPDATA = $oldAppData }
}

function Invoke-PairBlobSelfTests {
    $fixture = Join-Path ([IO.Path]::GetTempPath()) ('osl-p2p-pair-blob-selftest-' + [Guid]::NewGuid().ToString('N'))
    $blobRoot = Join-Path $fixture 'blob'
    $appA = Join-Path $fixture 'app-a'; $appB = Join-Path $fixture 'app-b'
    $rootA = Join-Path $appA 'org.oslprivacy.hub\osl-core'; $rootB = Join-Path $appB 'org.oslprivacy.hub\osl-core'
    try {
        New-Item -ItemType Directory -Force -Path $rootA, $rootB | Out-Null
        $offerA = [Text.Encoding]::UTF8.GetBytes('{"schemaVersion":1,"osl_user_id":"opaque-a","friend_code":"a"}')
        $offerB = [Text.Encoding]::UTF8.GetBytes('{"schemaVersion":1,"osl_user_id":"opaque-b","friend_code":"b"}')
        [IO.File]::WriteAllBytes((Join-Path $rootA 'discord-qa-offer.v1.json'), $offerA)
        [IO.File]::WriteAllBytes((Join-Path $rootB 'discord-qa-offer.v1.json'), $offerB)
        foreach ($call in @(
            @($appA, 'A', 'Push', '', 'exchange'), @($appB, 'B', 'Push', '', 'exchange'),
            @($appA, 'A', 'Pull', 'B', 'exchange'), @($appB, 'B', 'Pull', 'A', 'exchange')
        )) {
            $rc = Invoke-PairBlobSelfTestChild -AppData $call[0] -SideForTest $call[1] -Mode $call[2] -PeerForTest $call[3] -RunForTest $call[4] -BlobRoot $blobRoot -Receipt (Join-Path $fixture ([Guid]::NewGuid().ToString('N') + '.json'))
            Assert-SelfTest ($rc -eq 0) ('two-way exchange child failed: {0} {1}' -f $call[1], $call[2])
        }
        Assert-SelfTest ([Linq.Enumerable]::SequenceEqual([IO.File]::ReadAllBytes((Join-Path $rootA 'discord-qa-peer-offer.v1.json')), $offerB)) 'A did not receive B offer byte-for-byte'
        Assert-SelfTest ([Linq.Enumerable]::SequenceEqual([IO.File]::ReadAllBytes((Join-Path $rootB 'discord-qa-peer-offer.v1.json')), $offerA)) 'B did not receive A offer byte-for-byte'

        [IO.File]::WriteAllBytes((Join-Path $rootB 'discord-qa-offer.v1.json'), $offerA)
        $sameRc = Invoke-PairBlobSelfTestChild -AppData $appB -SideForTest B -Mode Push -RunForTest same -BlobRoot $blobRoot -Receipt (Join-Path $fixture 'same-push.json')
        Assert-SelfTest ($sameRc -eq 0) 'same-identity fixture could not push'
        $sameReceipt = Join-Path $fixture 'same-pull.json'
        $sameRc = Invoke-PairBlobSelfTestChild -AppData $appA -SideForTest A -Mode Pull -PeerForTest B -RunForTest same -BlobRoot $blobRoot -Receipt $sameReceipt
        Assert-SelfTest ($sameRc -eq 1) 'same osl_user_id was not refused'
        Assert-SelfTest (((Get-Content -LiteralPath $sameReceipt -Raw | ConvertFrom-Json).overall.verdict) -eq 'failed') 'same osl_user_id did not write a failed receipt'

        [IO.File]::WriteAllBytes((Join-Path $rootB 'discord-qa-offer.v1.json'), $offerB)
        $tornRc = Invoke-PairBlobSelfTestChild -AppData $appB -SideForTest B -Mode Push -RunForTest torn -BlobRoot $blobRoot -Receipt (Join-Path $fixture 'torn-push.json')
        Assert-SelfTest ($tornRc -eq 0) 'torn-upload fixture could not push'
        [IO.File]::WriteAllBytes((Join-Path $blobRoot 'pairing/torn/B-offer.json'), [byte[]](1,2,3))
        $tornReceipt = Join-Path $fixture 'torn-pull.json'
        $tornRc = Invoke-PairBlobSelfTestChild -AppData $appA -SideForTest A -Mode Pull -PeerForTest B -RunForTest torn -BlobRoot $blobRoot -Receipt $tornReceipt
        Assert-SelfTest ($tornRc -eq 1) 'truncated blob upload was accepted'
        Assert-SelfTest (((Get-Content -LiteralPath $tornReceipt -Raw | ConvertFrom-Json).overall.verdict) -eq 'failed') 'truncated blob upload did not write a failed receipt'
        Write-Host 'T19-T34 self-test passed: two-way bytes match; same identity and torn upload refused.' -ForegroundColor Green
    } finally { Remove-Item -LiteralPath $fixture -Recurse -Force -ErrorAction SilentlyContinue }
}

$operation = if ($Push) { 'push' } elseif ($Pull) { 'pull' } else { '<none>' }
Say ('=== OSL QA cross-VM pairing: {0} ===' -f $operation) 'Cyan'

if ($RunScriptSelfTests) { Invoke-PairBlobSelfTests; exit 0 }

if (-not $Side) { Add-S 'gate/side' 'failed' '-Side was not given.'; Finish 'blocked' 'A local identity side is required.' 'Pass -Side A or -Side B.' }
if (-not $Push -and -not $Pull) { Add-S 'gate/mode' 'failed' 'Neither -Push nor -Pull was given.'; Finish 'blocked' 'Nothing to do.' 'Pass -Push or -Pull.' }
if ($Push -and $Pull) { Add-S 'gate/mode' 'failed' '-Push and -Pull are mutually exclusive.'; Finish 'blocked' 'One operation per invocation.' 'Run twice.' }
if ($Pull -and -not $PeerSide) { Add-S 'gate/mode' 'failed' '-Pull requires -PeerSide.'; Finish 'blocked' 'PeerSide is required for -Pull.' 'Pass -PeerSide A or B.' }
if ($Pull -and $PeerSide -eq $Side) { Add-S 'gate/distinct-sides' 'failed' 'PeerSide equals Side.'; Finish 'blocked' 'Pulling your own published side as "the peer" would consume your own offer as if it were the other identity''s.' 'Pass the OTHER side.' }

Confirm-LocalClosed
$root = Resolve-Root -Bundle $Bundle
Add-S 'resolve/local' 'ok' $root
$localOffer = Get-Content -LiteralPath (Join-Path $root 'discord-qa-offer.v1.json') -Raw | ConvertFrom-Json
if (-not $localOffer.osl_user_id) {
    Add-S 'gate/local-offer-readable' 'failed' 'Local offer has no osl_user_id.'
    Finish 'blocked' 'The local offer file is present but not valid.' 'Delete it and restart this instance so the bootstrap rewrites it.'
}
Add-S 'gate/local-offer-readable' 'ok' 'Local offer carries an osl_user_id.'

if ($Push) {
    $localPath = Join-Path $root 'discord-qa-offer.v1.json'
    $bytes = [IO.File]::ReadAllBytes($localPath)
    $sha = ([BitConverter]::ToString([Security.Cryptography.SHA256]::Create().ComputeHash($bytes)) -replace '-', '').ToLowerInvariant()
    $remote = "pairing/$RunId/$Side-offer.json"
    Put-BlobBytes -Name $remote -Bytes $bytes
    Put-BlobBytes -Name "$remote.ready" -Bytes ([Text.Encoding]::ASCII.GetBytes($sha))
    Add-S 'push' 'ok' ('Uploaded {0} bytes to {1} (sha256 {2}), then wrote the .ready sentinel.' -f $bytes.Length, $remote, $sha)
    Finish 'ok' `
        ('Side {0}''s offer is on the blob at {1}, integrity-sealed by its own sha256 in {1}.ready.' -f $Side, $remote) `
        ('On the peer VM, run: osl-p2p-pair-blob.ps1 -Side {0} -Pull -PeerSide {1} -RunId {2}' -f $PeerSide, $Side, $RunId) `
        @{ remote = $remote; sha256 = $sha; localOslUserId = $localOffer.osl_user_id }
}

if ($Pull) {
    $remote = "pairing/$RunId/$PeerSide-offer.json"
    $start = Get-Date
    while (-not (Test-BlobExists "$remote.ready")) {
        if (((Get-Date) - $start).TotalSeconds -ge $TimeoutSec) {
            Add-S 'wait/peer-ready' 'failed' ('{0}.ready absent after {1}s.' -f $remote, $TimeoutSec)
            Finish 'blocked' 'The peer has not pushed yet (or pushed under a different -RunId).' 'Confirm the peer ran -Push with the same -RunId, then retry.'
        }
        Start-Sleep -Seconds 3
    }
    Add-S 'wait/peer-ready' 'ok' ('{0}.ready is present.' -f $remote)

    $expectedSha = ([Text.Encoding]::ASCII.GetString((Get-BlobBytes "$remote.ready"))).Trim()
    $payload = Get-BlobBytes $remote
    $actualSha = ([BitConverter]::ToString([Security.Cryptography.SHA256]::Create().ComputeHash($payload)) -replace '-', '').ToLowerInvariant()
    if ($actualSha -ne $expectedSha) {
        Add-S 'verify/transport-integrity' 'failed' ('sentinel={0} actual={1}' -f $expectedSha, $actualSha)
        Finish 'failed' 'The downloaded bytes do not match the sentinel sha256 -- a torn or tampered transfer.' 'Re-run -Push on the peer, then retry -Pull.'
    }
    Add-S 'verify/transport-integrity' 'ok' ('Downloaded bytes match the sentinel sha256 {0}. This proves the blob copy is intact -- it proves NOTHING about the offer''s authenticity; that check is next.' -f $actualSha)

    $peerOffer = $null
    try { $peerOffer = [Text.Encoding]::UTF8.GetString($payload) | ConvertFrom-Json } catch { $peerOffer = $null }
    if (-not $peerOffer -or -not $peerOffer.osl_user_id) {
        Add-S 'gate/peer-offer-readable' 'failed' 'Downloaded offer has no osl_user_id.'
        Finish 'blocked' 'The downloaded bytes do not parse as a valid offer.' 'Confirm the peer''s local osl-p2p-pair.ps1-style offer file is intact and re-push.'
    }
    Add-S 'gate/peer-offer-readable' 'ok' 'Downloaded offer carries an osl_user_id.'
    if ($peerOffer.osl_user_id -eq $localOffer.osl_user_id) {
        Add-S 'gate/two-identities' 'failed' 'Local and downloaded offers carry the same osl_user_id.'
        Finish 'failed' `
            'BOTH SIDES PUBLISHED THE SAME osl_user_id. This is one identity on two machines -- almost certainly the same profile cloned onto both VMs, or the wrong -Side/-PeerSide was used.' `
            'Confirm each VM created its own identity independently, then re-run.'
    }
    Add-S 'gate/two-identities' 'ok' 'Local and downloaded offers carry different osl_user_ids. (Only the opaque IDs were compared; no friend code or safety number was read.)'

    $statusPath = Join-Path $root 'discord-qa-pairing-status.v1.json'
    if (Test-Path -LiteralPath $statusPath) {
        $status = Get-Content -LiteralPath $statusPath -Raw | ConvertFrom-Json
        if ($status.verified -eq $true -and $status.peer_offer_sha256 -eq $actualSha) {
            Add-S 'gate/not-already-consumed' 'failed' `
                ('discord-qa-pairing-status.v1.json already records a VERIFIED pairing consuming an offer with this exact sha256 ({0}).' -f $actualSha)
            Finish 'blocked' `
                'This exact peer offer was already consumed by the product and the resulting pairing was already verified (peer_offer_sha256 matches). Refusing to reuse it.' `
                'If a fresh pairing is really intended, the peer must publish a NEW offer (reset that identity per docs/qa/two-identity-p2p-verification.md §7), then re-push under a new -RunId.'
        }
    }
    $peerOfferDst = Join-Path $root 'discord-qa-peer-offer.v1.json'
    if ((Test-Path -LiteralPath $peerOfferDst)) {
        $existingSha = (Get-FileHash -LiteralPath $peerOfferDst -Algorithm SHA256).Hash.ToLowerInvariant()
        if ($existingSha -eq $actualSha) {
            Add-S 'gate/not-already-consumed' 'ok' 'Identical bytes are already installed locally and not yet consumed; nothing to do.'
            Finish 'ok' 'This peer offer is already installed (idempotent no-op).' 'Restart this instance to consume it, if you have not already.' `
                @{ remote = $remote; sha256 = $actualSha; peerOslUserId = $peerOffer.osl_user_id; alreadyInstalled = $true }
        }
    }
    Add-S 'gate/not-already-consumed' 'ok' 'No verified pairing and no identical already-installed file for this sha256; safe to install.'

    $tmp = $peerOfferDst + '.tmp'
    [IO.File]::WriteAllBytes($tmp, $payload)
    if (Test-Path -LiteralPath $peerOfferDst) { [IO.File]::Replace($tmp, $peerOfferDst, $null) } else { [IO.File]::Move($tmp, $peerOfferDst) }
    Add-S 'install' 'ok' ('Wrote {0} verbatim ({1} bytes, sha256 {2}).' -f $peerOfferDst, $payload.Length, $actualSha)

    Finish 'ok' `
        'The peer offer is installed. This instance is not running, so it will consume the peer offer at its next start, re-derive and check the osl_user_id/safety number against the signed code itself, and write discord-qa-pairing-status.v1.json only if that holds. THIS SCRIPT DID NOT VERIFY THE SIGNATURE -- the product does, on consumption, exactly as it would for a same-host copy.' `
        'Start this instance, then run scripts\qa\osl-p2p-loop.ps1 (or the same-VM equivalent). Its mutual-pairing gate confirms the result.' `
        @{ remote = $remote; sha256 = $actualSha; peerOslUserId = $peerOffer.osl_user_id; alreadyInstalled = $false }
}
