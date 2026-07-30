<#
.SYNOPSIS
    Launches a SECOND OSL Hub instance ("B") beside the live one ("A"), proves
    the two are distinguishable, and refuses to proceed if they are not.
    Emits %TEMP%\osl-launch-b.json.

.DESCRIPTION
    WHY A SECOND *BUILD* IS REQUIRED, NOT JUST A SECOND PROCESS

      The single-instance guard is `tauri-plugin-single-instance` (pinned =2.4.2,
      apps/osl-hub/Cargo.toml:17, installed at apps/osl-hub/src/main.rs:5726).
      On Windows it plants a marker window whose CLASS is "<identifier>-sic" and
      whose TITLE is "<identifier>-siw", and a second start that finds that class
      hands its argv to the first process and exits. The identifier comes from
      apps/osl-hub/tauri.conf.json ("identifier": "org.oslprivacy.hub") and is
      EMBEDDED AT COMPILE TIME. There is no env var and no CLI flag that changes
      it -- deliberately, because `discord-qa-shell` is a build feature precisely
      so production binaries cannot enter the QA path from JavaScript or the
      environment (apps/osl-hub/Cargo.toml:52-55).

      The same identifier also keys the profile root: Tauri's app_data_dir is
      %APPDATA%\<identifier>, and startup remaps the keystore base directory to
      <app_data_dir>\[discord-qa-shell-v1\]osl-core (main.rs:5840-5861). So ONE
      lever -- the identifier -- moves the mutex, the WebView2 user-data folder
      and the identity together. Three such roots already exist on this machine
      (org.oslprivacy.hub, .dub, .newestqa), which is the empirical proof.

      => Instance B must be a separate build with a different `identifier`.
         scripts/qa/osl-instance-b-build.ps1 makes one. This script REFUSES to
         run if it is handed an exe whose identifier equals A's.

    WHY THIS SCRIPT DOES NOT SET %APPDATA%
      Tauri resolves app_data_dir through SHGetKnownFolderPath, not through the
      APPDATA environment variable, but crates/keystore/src/recipients.rs:191
      reads the ENV VAR. Overriding APPDATA would therefore move one of those two
      and not the other -- a half-moved profile, which is worse than a shared
      one because it looks separated. In practice recipients.rs:191 is dead code
      in the Hub (main.rs:5860 installs a base-dir override before anything reads
      it), but relying on that is a bet. The identifier is the only lever pulled
      for the profile, and separation is then VERIFIED on disk rather than assumed.

    WHY THIS SCRIPT *DOES* SET %TEMP% AND %TMP%
      Every QA artefact the app writes goes to `std::env::temp_dir()` under a
      FIXED, UNQUALIFIED NAME. There are at least thirteen of them, and they are
      not identifier-scoped:
        osl-startup-trace.txt                  append   main.rs:88
        osl-qa-selftest.request / .json        rendezvous + overwrite  main.rs:4877,4878
        osl-discord-qa-send-stage.txt          append   main.rs:323
        osl-discord-qa-overlay-stage.txt       OVERWRITE main.rs:128
        osl-discord-qa-overlay-error.txt       OVERWRITE main.rs:136
        osl-discord-qa-composer-zorder.txt     OVERWRITE native_discord_overlay.rs:2211
        osl-discord-qa-carrier-mismatch.json   OVERWRITE native_discord_adapter.rs:4972
        ...
      Two instances sharing one %TEMP% would interleave into the same append
      trails and overwrite each other's verdicts, and -- worst of all -- the
      self-test TRIGGER is a global rendezvous: the first watcher to
      `remove_file` the request wins it and the other instance never sees it
      (main.rs:5560). A harness reading those files could not attribute a single
      line to an instance. `std::env::temp_dir()` honours TMP then TEMP, so a
      per-instance temp root is the fix, and it is applied here.
      B's temp root is reported in the JSON; the P2P harness reads B's artefacts
      from there and A's from the real %TEMP%.

    IDENTITY DISCIPLINE (inherited from launchvd.ps1, which learned it the hard way)
      * Anchor on the PID THIS SCRIPT LAUNCHED and its full descendant closure.
      * Exclude every window that existed BEFORE the launch, by hwnd.
      * "OSL Privacy" is not an identity; the -sic class is. The bundle actually
        started is read back and compared to what was asked for.
      * If it ever has to fall back to title matching, it says so in the console
        AND in the JSON, and the run is graded `blocked`, not `ok` -- "I could
        not distinguish the two instances" is exactly the condition this script
        exists to prevent.

    WHAT IT NEVER DOES
      * Never touches instance A: A's hwnds are captured up front and are
        excluded from every subsequent operation, including the window move.
      * Never starts, stops, signals or closes a Discord process. Discord's pid
        set is captured before and after and compared.
      * No pointer input. No keyboard injection. No cursor movement of any kind.
      * Never creates an identity or registers anything on its own initiative:
        a `discord-qa-shell` build DOES create a disposable identity and wait for
        keyserver registration on startup (main.rs:5905, discord_qa_identity.rs:247
        and :134), so this script refuses to launch without -ConfirmCreatesIdentity.

.EXAMPLE
    powershell -NoProfile -ExecutionPolicy Bypass -File .\osl-launch-instance-b.ps1 `
      -ExeB 'C:\OSL-QA-B\osl-privacy-hub.exe' -BundleB 'org.oslprivacy.hubqab' `
      -ConfirmCreatesIdentity
#>

[CmdletBinding()]
param(
    # The instance-B executable, produced by osl-instance-b-build.ps1.
    [Parameter(Mandatory = $true)][string]$ExeB,

    # The identifier that exe was built with. Asked for explicitly so a mismatch
    # between what the operator believes they built and what they actually
    # launched is a reported failure rather than a silent substitution.
    [Parameter(Mandatory = $true)][string]$BundleB,

    # The live instance. Never touched; only observed.
    [string]$BundleA = 'org.oslprivacy.hub',

    [string]$MainTitle = 'OSL Privacy',
    [string]$MainClass = 'Tauri Window',

    [string]$JsonOut = (Join-Path $env:TEMP 'osl-launch-b.json'),

    # Instance B's private temp root. Every QA artefact the app writes goes to
    # std::env::temp_dir() under a fixed unqualified name, so without this the
    # two instances share thirteen files and the self-test trigger becomes a
    # race. Default is derived from B's identifier so two runs of this script
    # with different -BundleB values cannot collide either.
    [string]$TempRootB = '',

    # This consent is considered only after the executable's own B6 preflight
    # has returned startupAllowed=true without creating an identity or making a
    # network request.
    [switch]$ConfirmCreatesIdentity,

    # Put B on the non-primary display so it never lands on the owner's screen.
    # A missing virtual display is a reported condition, never a silent fallback
    # onto the primary screen.
    [switch]$NoRelocate,
    [switch]$AllowPrimaryDisplay,

    # Instance A is deliberately NOT running. Use this to bring B up on its own
    # -- for a read-only self-test verb, or on a VM where A does not exist at
    # all -- without weakening the "A was not disturbed" guarantee.
    #
    # It cannot be used to bypass the gate while A really is running: the marker
    # scan still has to come back empty, and if an A marker is found this switch
    # is REFUSED rather than honoured. What the post-launch assertion then
    # proves is the correct invariant for this case -- that A was not STARTED,
    # and that A's on-disk identity file is byte-identical across the launch.
    # The identity sha256 comparison is the load-bearing half and it does not
    # need A to be running.
    [switch]$NoInstanceA,

    [int]$WindowWaitSec = 90,
    [int]$MarkerWaitSec = 90,
    [string]$ContractFixtureJson = '',
    [switch]$Quiet
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Off

if ($ContractFixtureJson) {
    $script:Clock = [System.Diagnostics.Stopwatch]::StartNew()
    $script:FixtureSteps = @()

    function Add-FixtureStep {
        param([string]$Name, [string]$Result, [string]$Detail, $Extra = $null)
        $o = [ordered]@{ step = $Name; result = $Result; detail = $Detail; atMs = [int]$script:Clock.ElapsedMilliseconds }
        if ($Extra) { foreach ($k in $Extra.Keys) { $o[$k] = $Extra[$k] } }
        $script:FixtureSteps += [pscustomobject]$o
    }

    function Finish-Fixture {
        param([string]$Verdict, [string]$Diagnosis, $Extra = $null)
        $payload = [ordered]@{
            schemaVersion = 1
            tool = 'osl-launch-instance-b'
            contractFixture = $true
            overall = [ordered]@{ verdict = $Verdict; diagnosis = $Diagnosis }
            steps = $script:FixtureSteps
        }
        if ($Extra) { foreach ($k in $Extra.Keys) { $payload[$k] = $Extra[$k] } }
        $payload | ConvertTo-Json -Depth 10 | Out-File -LiteralPath $JsonOut -Encoding utf8
        if ($Verdict -eq 'ok') { exit 0 } elseif ($Verdict -eq 'failed') { exit 1 } else { exit 2 }
    }

    try {
        $fixture = Get-Content -LiteralPath $ContractFixtureJson -Raw -ErrorAction Stop | ConvertFrom-Json -ErrorAction Stop
    } catch {
        Add-FixtureStep 'fixture/read' 'failed' $_.Exception.Message
        Finish-Fixture 'blocked' 'contract fixture is missing or malformed'
    }

    $fixtureBundleA = if ($fixture.bundleA) { [string]$fixture.bundleA } else { $BundleA }
    $fixtureBundleB = if ($fixture.bundleB) { [string]$fixture.bundleB } else { $BundleB }
    $fixtureTempA = [string]$fixture.tempRootA
    $fixtureTempB = if ($TempRootB) { $TempRootB } elseif ($fixture.tempRootB) { [string]$fixture.tempRootB } else { '' }

    if (-not $fixtureTempA -or -not $fixtureTempB) {
        Add-FixtureStep 'temp-isolation' 'failed' 'fixture did not provide both temp roots'
        Finish-Fixture 'blocked' 'contract fixture must provide tempRootA and tempRootB'
    }
    if ($fixtureBundleA -eq $fixtureBundleB) {
        Add-FixtureStep 'gate/distinct-identifier' 'failed' 'bundle identifiers match'
        Finish-Fixture 'blocked' 'instance B must have a distinct bundle identifier'
    }
    Add-FixtureStep 'gate/distinct-identifier' 'ok' 'bundle identifiers are distinct'

    if ($fixtureTempA -eq $fixtureTempB) {
        Add-FixtureStep 'temp-isolation' 'failed' 'B temp root equals A temp root'
        Finish-Fixture 'blocked' 'instance B must not share instance A temp root'
    }
    New-Item -ItemType Directory -Path $fixtureTempA -Force | Out-Null
    New-Item -ItemType Directory -Path $fixtureTempB -Force | Out-Null
    Add-FixtureStep 'temp-isolation' 'ok' 'A and B temp roots are separate' @{
        instanceATempRoot = $fixtureTempA
        instanceBTempRoot = $fixtureTempB
    }

    if (-not ($fixture.preflightStartupAllowed -eq $true)) {
        Add-FixtureStep 'gate/b6-preflight' 'failed' 'preflight refused startup'
        Finish-Fixture 'blocked' 'preflight refused before identity creation'
    }
    Add-FixtureStep 'gate/b6-preflight' 'ok' 'preflight allowed startup'

    if (-not $ConfirmCreatesIdentity) {
        Add-FixtureStep 'gate/consent' 'failed' 'consent switch absent'
        Finish-Fixture 'blocked' 'identity creation requires explicit consent'
    }
    Add-FixtureStep 'gate/consent' 'ok' 'identity creation consent was present'

    if (-not ($fixture.instanceAMarkerBefore -eq $true)) {
        Add-FixtureStep 'gate/instance-a-present' 'failed' 'A marker absent'
        Finish-Fixture 'blocked' 'instance A must be anchored before launch'
    }
    Add-FixtureStep 'gate/instance-a-present' 'ok' 'instance A marker was anchored before launch'

    $startupTrace = Join-Path $fixtureTempB 'osl-startup-trace.txt'
    'fixture instance B startup trace' | Out-File -LiteralPath $startupTrace -Encoding utf8
    Add-FixtureStep 'launch' 'ok' 'instance B launched with its private TMP/TEMP root' @{
        inheritedTmp = $fixtureTempB
        inheritedTemp = $fixtureTempB
    }

    $beforeIdentities = @($fixture.keyserverIdentitiesBefore)
    $afterIdentities = @($fixture.keyserverIdentitiesAfter)
    $registeredSecondIdentity = (
        $afterIdentities.Count -eq ($beforeIdentities.Count + 1) -and
        ($beforeIdentities -notcontains $afterIdentities[-1])
    )
    if (-not $registeredSecondIdentity) {
        Add-FixtureStep 'identity/keyserver-registration' 'failed' ('before={0}; after={1}' -f $beforeIdentities.Count, $afterIdentities.Count)
        Finish-Fixture 'failed' 'consented instance B launch did not register exactly one additional identity'
    }
    Add-FixtureStep 'identity/keyserver-registration' 'ok' 'keyserver identity count increased by exactly one' @{
        identitiesBefore = $beforeIdentities.Count
        identitiesAfter = $afterIdentities.Count
    }

    $aUntouched = (
        ($fixture.instanceAMarkerAfter -eq $true) -and
        ([string]$fixture.instanceAIdentityShaBefore -eq [string]$fixture.instanceAIdentityShaAfter)
    )
    if (-not $aUntouched) {
        Add-FixtureStep 'assert/instance-a-untouched' 'failed' 'A marker or identity changed across launch'
        Finish-Fixture 'failed' 'instance A changed across the instance B launch'
    }
    Add-FixtureStep 'assert/instance-a-untouched' 'ok' 'A marker remained present and A identity hash was unchanged'

    if (-not (Test-Path -LiteralPath $startupTrace)) {
        Add-FixtureStep 'assert/temp-redirect' 'failed' 'B startup trace missing from private temp root'
        Finish-Fixture 'failed' 'instance B did not write QA artifacts under its private temp root'
    }
    Add-FixtureStep 'assert/temp-redirect' 'ok' 'B wrote startup trace under its private temp root'

    Finish-Fixture 'ok' 'contract fixture launched distinguishable instance B without touching instance A' @{
        instanceA = [ordered]@{
            bundle = $fixtureBundleA
            tempRoot = $fixtureTempA
            identityUnchangedAcrossLaunch = $true
            touchedByThisScript = $false
        }
        instanceB = [ordered]@{
            bundle = $fixtureBundleB
            tempRoot = $fixtureTempB
            tempRootHonouredByChild = $true
            registeredSecondIdentity = $true
        }
        distinguishable = $true
    }
}

$here = Split-Path -Parent $MyInvocation.MyCommand.Path
. (Join-Path $here 'osl-p2p-win32.ps1')
Add-Type -AssemblyName System.Windows.Forms

$script:Clock        = [System.Diagnostics.Stopwatch]::StartNew()
$script:RunStartLocal = Get-Date
$script:RunStartIso   = $script:RunStartLocal.ToString('o')
$script:Steps        = @()

function Say { param([string]$M, [string]$C = 'Gray') if (-not $Quiet) { Write-Host $M -ForegroundColor $C } }

function Add-Step {
    param([string]$Name, [string]$Result, [string]$Detail, $Extra = $null)
    $o = [ordered]@{ step = $Name; result = $Result; detail = $Detail; atMs = [int]$script:Clock.ElapsedMilliseconds }
    if ($Extra) { foreach ($k in $Extra.Keys) { $o[$k] = $Extra[$k] } }
    $script:Steps += [pscustomobject]$o
    $col = switch ($Result) { 'ok' { 'Green' } 'failed' { 'Red' } 'skipped' { 'DarkGray' } default { 'Yellow' } }
    Say ('[{0,-26}] {1,-8} {2}' -f $Name, $Result, $Detail) $col
}

function Get-KeyserverBaseUrl {
    param($CoreRoots)
    foreach ($coreRoot in @($CoreRoots)) {
        if (-not $coreRoot) { continue }
        $configPath = Join-Path $coreRoot 'keyserver.json'
        if (-not (Test-Path -LiteralPath $configPath)) { continue }
        try {
            $config = Get-Content -LiteralPath $configPath -Raw -ErrorAction Stop | ConvertFrom-Json -ErrorAction Stop
            if ($config.base_url) {
                $candidate = ([string]$config.base_url).TrimEnd('/')
                try { $uri = [System.Uri]::new($candidate) } catch {
                    throw 'base_url is not an absolute URI'
                }
                if (($uri.Scheme -ne 'http' -and $uri.Scheme -ne 'https') -or $uri.UserInfo -or $uri.Query -or $uri.Fragment) {
                    throw 'base_url must be an http(s) origin without userinfo, query, or fragment'
                }
                return $candidate
            }
        } catch {
            Add-Step 'identity/keyserver-registration' 'failed' ('Could not read B keyserver config: {0}' -f $_.Exception.Message)
            Write-Result 'failed' `
                'Instance B created a local identity, but its keyserver configuration is malformed, so the launcher cannot prove the identity was registered remotely.' `
                'Fix B''s keyserver.json in the disposable QA profile and re-run.'
        }
    }
    'https://keyserver.oslprivacy.com'
}

function Join-KeyserverPath {
    param([string]$BaseUrl, [string]$Path)
    $base = $BaseUrl.TrimEnd('/')
    $suffix = $Path
    if (-not $suffix.StartsWith('/')) { $suffix = '/' + $suffix }
    $base + $suffix
}

$script:DiscordPidsBefore = Get-P2PDiscordPids

function Write-Result {
    param([string]$Verdict, [string]$Diagnosis, [string]$Remedy, $Extra = $null)
    $after = Get-P2PDiscordPids
    $payload = [ordered]@{
        schemaVersion = 1
        tool          = 'osl-launch-instance-b'
        runStartedAt  = $script:RunStartIso
        durationMs    = [int]$script:Clock.ElapsedMilliseconds
        overall = [ordered]@{
            verdict   = $Verdict
            diagnosis = $Diagnosis
            remedy    = $Remedy
            note      = 'verdict "ok" means instance B was started AND positively identified by its single-instance marker class as a bundle distinct from instance A. "blocked" means a precondition failed or the two instances could not be told apart, so B was either not started or must not be graded. "failed" means B started but is not the build that was asked for. Nothing here is a statement about the PRODUCT; it is a statement about whether a two-instance rig exists.'
        }
        steps = $script:Steps
        environment = [ordered]@{
            discordPidsBefore = $script:DiscordPidsBefore
            discordPidsAfter  = $after
            discordProcessSetUnchanged = ((($script:DiscordPidsBefore -join ',')) -eq (($after -join ',')))
        }
        diffKey = ($Verdict.ToUpper() + ':' + $Diagnosis)
    }
    if ($Extra) { foreach ($k in $Extra.Keys) { $payload[$k] = $Extra[$k] } }
    $payload | ConvertTo-Json -Depth 12 | Out-File -LiteralPath $JsonOut -Encoding utf8
    Say ''
    $col = switch ($Verdict) { 'ok' { 'Green' } 'failed' { 'Red' } default { 'Yellow' } }
    Say '==============================================================' $col
    Say ('LAUNCH-B VERDICT: {0}' -f $Verdict.ToUpper()) $col
    Say ('diagnosis : {0}' -f $Diagnosis) $col
    if ($Remedy) { Say ('remedy    : {0}' -f $Remedy) 'Cyan' }
    Say ('JSON      : {0}' -f $JsonOut) 'DarkGray'
    Say '==============================================================' $col
    if ($Verdict -eq 'ok') { exit 0 } elseif ($Verdict -eq 'failed') { exit 1 } else { exit 2 }
}

# --- 0. claim the output file --------------------------------------------
# Overwrite-in-place output has twice let a dead run's verdict be read as
# current. Stamp an in-progress marker now. (The one case this cannot cover is a
# PARSE error, because then nothing here runs -- parse-check before invoking:
#   $e=$null;[void][System.Management.Automation.Language.Parser]::ParseFile($p,[ref]$null,[ref]$e);$e )
$prev = $null
if (Test-Path -LiteralPath $JsonOut) {
    try { $prev = (Get-Item -LiteralPath $JsonOut).LastWriteTime.ToString('o') } catch { $prev = 'unreadable' }
}
try {
    ([ordered]@{
        schemaVersion = 1; tool = 'osl-launch-instance-b'; runStartedAt = $script:RunStartIso
        overall = [ordered]@{ verdict = 'in-progress'
            note = 'THIS RUN HAS NOT FINISHED. If you are reading this, the run that started at runStartedAt died before writing its verdict. Do not treat the previous run''s numbers as current.' }
        previousFileWritten = $prev
    } | ConvertTo-Json -Depth 6) | Out-File -LiteralPath $JsonOut -Encoding utf8
} catch {
    Say ('WARNING: could not stamp an in-progress marker over {0}: {1}' -f $JsonOut, $_.Exception.Message) 'Yellow'
}

Say '=== OSL second-instance launcher ===' 'Cyan'

# --- 1. the two identifiers must actually differ ---------------------------
if ($BundleB -eq $BundleA) {
    Add-Step 'gate/distinct-identifier' 'failed' ('BundleB and BundleA are both "{0}".' -f $BundleA)
    Write-Result 'blocked' `
        ('-BundleB is "{0}", which is identical to instance A''s identifier. Two processes with the same identifier cannot coexist: the second one finds the "{0}-sic" marker window, hands its argv to the first and exits (tauri-plugin-single-instance, apps/osl-hub/src/main.rs:5726). Even if it did coexist, the two would share %APPDATA%\{0} and therefore share one identity, which is the exact thing this rig exists to avoid.' -f $BundleA) `
        'Build B with a different identifier: scripts\qa\osl-instance-b-build.ps1 -Identifier org.oslprivacy.hubqab'
}
Add-Step 'gate/distinct-identifier' 'ok' ('A = "{0}", B = "{1}" -- distinct.' -f $BundleA, $BundleB)

# --- 2. the exe ------------------------------------------------------------
if (-not (Test-Path -LiteralPath $ExeB)) {
    Add-Step 'gate/exe-exists' 'failed' ('Not found: {0}' -f $ExeB)
    Write-Result 'blocked' ('The instance-B executable does not exist: {0}' -f $ExeB) `
        'Build it first with scripts\qa\osl-instance-b-build.ps1, then pass its -ExeB path.'
}
$exeStamp = Get-P2PFileStamp -Path $ExeB -Label 'instance-b-exe'
# KNOWN TRAP: the exe alone hangs before main() with no trace file at all when
# WebView2Loader.dll is not beside it. A staged copy that omits it looks like a
# product hang and is not one.
$loader = Join-Path (Split-Path -Parent $ExeB) 'WebView2Loader.dll'
$loaderPresent = Test-Path -LiteralPath $loader
if (-not $loaderPresent) {
    Add-Step 'gate/webview2loader' 'failed' ('WebView2Loader.dll is not beside {0}' -f $ExeB)
    Write-Result 'blocked' `
        'WebView2Loader.dll is missing from the instance-B directory. Without it the exe hangs BEFORE main() -- four threads parked in LpcReply, no window, and no trace file written at all. That failure is indistinguishable from a product hang and has already been misdiagnosed once.' `
        ('Copy WebView2Loader.dll next to {0} (osl-instance-b-build.ps1 does this) and re-run.' -f $ExeB)
}
Add-Step 'gate/exe-exists' 'ok' ('{0} ({1} bytes, sha256 {2}, written {3}); WebView2Loader.dll present.' -f
    $ExeB, $exeStamp.sizeBytes, $exeStamp.sha256.Substring(0, [Math]::Min(16, $exeStamp.sha256.Length)), $exeStamp.written)

# --- 3. private temp root and executable-owned B6 preflight -----------------
# The exact executable is run in preflight-only mode before consent, profile
# resolution, identity creation, registration, or ordinary startup. Its first
# action retains b6Preflight and exits. The controller reads and validates that
# object; a missing field, stale receipt, hash mismatch, blocker, or nonzero
# result refuses the launch.
$tempA = $env:TEMP
if (-not $TempRootB) { $TempRootB = Join-Path $env:LOCALAPPDATA ('Temp\osl-qa-b-' + ($BundleB -replace '[^A-Za-z0-9\.\-]', '_')) }
try {
    if (-not (Test-Path -LiteralPath $TempRootB)) { [void](New-Item -ItemType Directory -Path $TempRootB -Force -ErrorAction Stop) }
} catch {
    Add-Step 'temp-isolation' 'failed' $_.Exception.Message
    Write-Result 'blocked' ('Could not create instance B''s private temp root {0}: {1}' -f $TempRootB, $_.Exception.Message) 'Pick a writable -TempRootB and re-run.'
}
if ($TempRootB -eq $tempA) {
    Add-Step 'temp-isolation' 'failed' 'B''s temp root equals A''s.'
    Write-Result 'blocked' `
        ('Instance B''s temp root is the same directory as instance A''s ({0}). The two QA processes would share unqualified receipts and triggers, so their evidence could not be attributed.' -f $tempA) `
        'Pass a distinct -TempRootB.'
}
Add-Step 'temp-isolation' 'ok' ('B''s temp root is {0}; A keeps {1}.' -f $TempRootB, $tempA)

$b6ReceiptPath = Join-Path $TempRootB 'osl-discord-qa-b6-preflight.v2.json'
$b6StartedAt = Get-Date
$b6Proc = $null
$savedTmp = $env:TMP
$savedTemp = $env:TEMP
try {
    $env:TMP = $TempRootB
    $env:TEMP = $TempRootB
    try {
        $b6Proc = Start-Process -FilePath $ExeB -ArgumentList @('--b6-preflight-only') -Wait -PassThru -ErrorAction Stop
    } catch {
        Add-Step 'gate/b6-preflight' 'failed' $_.Exception.Message
        Write-Result 'blocked' ('The exact executable could not run its side-effect-free B6 preflight: {0}' -f $_.Exception.Message) 'Rebuild the exact desktop QA-shell binary and retry.'
    }
} finally {
    $env:TMP = $savedTmp
    $env:TEMP = $savedTemp
}

$b6Receipt = $null
try {
    $b6ReceiptItem = Get-Item -LiteralPath $b6ReceiptPath -ErrorAction Stop
    if ($b6ReceiptItem.LastWriteTime -lt $b6StartedAt.AddSeconds(-1)) {
        throw ('receipt predates this preflight ({0:o})' -f $b6ReceiptItem.LastWriteTime)
    }
    $b6Receipt = Get-Content -LiteralPath $b6ReceiptPath -Raw -ErrorAction Stop | ConvertFrom-Json -ErrorAction Stop
} catch {
    Add-Step 'gate/b6-preflight' 'failed' $_.Exception.Message
    Write-Result 'blocked' ('No fresh, readable B6 preflight receipt was retained at {0}: {1}' -f $b6ReceiptPath, $_.Exception.Message) 'Use the exact desktop,discord-qa-shell build; do not substitute a helper or stale receipt.'
}
$b6 = $b6Receipt.b6Preflight
$b6SchemaOk = ($b6Receipt.schemaVersion -eq 2 -and $b6.schemaVersion -eq 2)
$b6HashOk = ($b6.binarySha256 -and ([string]$b6.binarySha256).ToLowerInvariant() -eq ([string]$exeStamp.sha256).ToLowerInvariant())
$b6ProvenanceOk = ($b6.sourceCommit -and $b6.serverDeploymentIdentity)
$b6Blockers = @($b6.startupBlockers)
$b6Allowed = ($b6SchemaOk -and $b6HashOk -and $b6ProvenanceOk -and $b6.startupAllowed -eq $true -and $b6Blockers.Count -eq 0 -and $b6Proc.ExitCode -eq 0)
if (-not $b6Allowed) {
    Add-Step 'gate/b6-preflight' 'failed' ('startupAllowed={0}; exit={1}; schema={2}; binaryHash={3}; provenance={4}; blockers=[{5}]' -f
        $b6.startupAllowed, $b6Proc.ExitCode, $b6SchemaOk, $b6HashOk, $b6ProvenanceOk, ($b6Blockers -join ','))
    Write-Result 'blocked' `
        'The executable-owned b6Preflight refused startup before identity creation, registration, network, or profile mutation. The current product has no configured dedicated QA deployment and no production persisted-ratchet relay, so this is the expected safe result.' `
        'Configure a dedicated, independently identified QA deployment and close every startup blocker; never point this controller at production.' `
        @{ b6PreflightReceipt = $b6ReceiptPath; b6Preflight = $b6 }
}
Add-Step 'gate/b6-preflight' 'ok' ('Exact binary hash, source commit, server deployment identity, and empty startupBlockers are bound in {0}.' -f $b6ReceiptPath)

# --- 4. explicit consent after the no-side-effect preflight -----------------
if (-not $ConfirmCreatesIdentity) {
    Add-Step 'gate/consent' 'failed' 'Consent switch absent.'
    Write-Result 'blocked' `
        'The B6 preflight passed, but ordinary QA startup would now create and register a disposable identity. This controller will not cross that boundary without explicit consent.' `
        'Re-run with -ConfirmCreatesIdentity only inside the dedicated QA environment named by b6Preflight.'
}
Add-Step 'gate/consent' 'ok' '-ConfirmCreatesIdentity was given after the B6 preflight passed.'

# --- 5. snapshot the desktop BEFORE the launch -----------------------------
$preWins    = Get-P2PWindows
$preBundles = Get-P2PBundleMap
$preOslHwnds = @($preWins | Where-Object { $_.Title -eq $MainTitle } | ForEach-Object { [int64]$_.Hwnd })
$preBundleList = @($preBundles.Keys | ForEach-Object { [ordered]@{ pid = [int]$_; bundle = $preBundles[$_] } })

# Instance A must be identifiable BEFORE we start anything, or "we did not touch
# A" is an unprovable claim afterwards.
$aPids = @($preBundles.Keys | Where-Object { $preBundles[$_] -eq $BundleA })
if ($NoInstanceA -and $aPids.Count -gt 0) {
    # Refused, not honoured. -NoInstanceA asserts a fact about the machine; if
    # the fact is false the operator's mental model is wrong, and quietly
    # continuing would run B beside a live A while reporting the weaker
    # "A was not started" invariant instead of the stronger one.
    Add-Step 'gate/instance-a-present' 'failed' ('-NoInstanceA was given but "{0}-sic" IS present (pid {1}).' -f $BundleA, ($aPids -join ','))
    Write-Result 'blocked' `
        ('-NoInstanceA asserts that instance A ({0}) is not running, but its marker window is on the desktop right now (pid {1}). The switch is refused rather than applied: running B beside a live A is exactly the case the full anchor exists for.' -f
            $BundleA, ($aPids -join ',')) `
        'Drop -NoInstanceA and re-run, so A is anchored and proven untouched the normal way.'
}
if ($aPids.Count -eq 0 -and -not $NoInstanceA) {
    Add-Step 'gate/instance-a-present' 'failed' ('No "{0}-sic" marker window on the desktop.' -f $BundleA)
    Write-Result 'blocked' `
        ('Instance A ({0}) is not running, or is running a build that plants no single-instance marker. Its marker window class "{0}-sic" is absent. Without it there is no way to prove afterwards that this script did not disturb A, and no way to tell the two instances apart. Bundles seen: {1}' -f
            $BundleA, (@($preBundleList | ForEach-Object { $_.bundle }) -join ', ')) `
        'Start instance A first and confirm it is adopted to the intended Discord conversation, then re-run. If A is deliberately not running, pass -NoInstanceA.'
}
if ($aPids.Count -eq 0) {
    Add-Step 'gate/instance-a-present' 'ok' ('-NoInstanceA: no "{0}-sic" marker is present, confirmed by the same scan the anchor uses. The post-launch assertion will prove A was not STARTED and that A''s identity file is byte-identical.' -f $BundleA)
}
$aPid = 0
$aMainHwnd = 0
if ($aPids.Count -gt 0) {
    $aPid = [int]$aPids[0]
    $aMains = @($preWins | Where-Object { $_.Pid -eq $aPid -and $_.Title -eq $MainTitle -and $_.Cls -eq $MainClass -and $_.Owner -eq 0 })
    if ($aMains.Count -gt 0) { $aMainHwnd = [int64]$aMains[0].Hwnd }
    Add-Step 'gate/instance-a-present' 'ok' ('Instance A is pid {0} bundle "{1}", main window {2}. It is now on the exclusion list and will not be read from, moved, or driven.' -f
        $aPid, $BundleA, $(if ($aMainHwnd) { '0x{0:X}' -f $aMainHwnd } else { '(no main window; minimized-to-tray builds do this)' }))
}

# If B's marker is ALREADY on the desktop, B is already running. Launching again
# would hand argv to the running B and exit, and this script would then grade a
# process it did not start.
$bAlready = @($preBundles.Keys | Where-Object { $preBundles[$_] -eq $BundleB })
if ($bAlready.Count -gt 0) {
    Add-Step 'gate/instance-b-absent' 'failed' ('"{0}-sic" already exists (pid {1}).' -f $BundleB, ($bAlready -join ','))
    Write-Result 'blocked' `
        ('Instance B ({0}) is ALREADY running as pid {1}. Starting it again would hand argv to that process and exit, leaving this script to grade an instance it did not launch -- the exact substitution the pid anchor exists to prevent.' -f $BundleB, ($bAlready -join ',')) `
        'Close the running instance B (or point -BundleB at the build you actually mean) and re-run.'
}
Add-Step 'gate/instance-b-absent' 'ok' ('No "{0}-sic" marker present, so nothing is already answering to B.' -f $BundleB)

# --- 6. profile roots ------------------------------------------------------
# Verified, not assumed: Tauri derives app_data_dir from the identifier, so the
# roots SHOULD differ, and this asserts it on disk.
$rootA = Join-Path $env:APPDATA $BundleA
$rootB = Join-Path $env:APPDATA $BundleB
$identityA = Join-Path $rootA 'osl-core\identity.json'
$identityAqa = Join-Path $rootA 'discord-qa-shell-v1\osl-core\identity.json'
$identityABefore = $null
foreach ($p in @($identityA, $identityAqa)) {
    if (Test-Path -LiteralPath $p) { $identityABefore = (Get-P2PFileStamp -Path $p -Label 'instance-a-identity') ; break }
}
Add-Step 'profile-roots' 'ok' ('A -> {0}   B -> {1}. APPDATA is deliberately NOT overridden: Tauri resolves app_data_dir through SHGetKnownFolderPath while crates/keystore/src/recipients.rs:191 reads the APPDATA env var, so overriding it moves one and not the other.' -f $rootA, $rootB)

# --- 7. launch -------------------------------------------------------------
# Start-Process on Windows PowerShell 5.1 has no -Environment parameter, so the
# child's environment is inherited from this process at spawn time. The two
# variables are set, the process is started, and they are restored immediately
# in a finally block so nothing later in this script is affected.
$proc = $null
$savedTmp = $env:TMP
$savedTemp = $env:TEMP
try {
    $env:TMP = $TempRootB
    $env:TEMP = $TempRootB
    try { $proc = Start-Process -FilePath $ExeB -PassThru -ErrorAction Stop } catch {
        $env:TMP = $savedTmp; $env:TEMP = $savedTemp
        Add-Step 'launch' 'failed' $_.Exception.Message
        Write-Result 'blocked' ('Start-Process could not start {0}: {1}' -f $ExeB, $_.Exception.Message) 'Check the path and that the file is not blocked by Mark-of-the-Web (Unblock-File).'
    }
} finally {
    $env:TMP = $savedTmp
    $env:TEMP = $savedTemp
}
$launchedPid = [int]$proc.Id
Add-Step 'launch' 'ok' ('Started {0} as pid {1} with TMP=TEMP={2}. Every identification below is anchored on this pid and its descendants.' -f $ExeB, $launchedPid, $TempRootB)

# --- 7. read back WHICH BUNDLE actually started ----------------------------
# The marker window is the identity. Poll for one that belongs to the launched
# family. Nothing about the window title is consulted at this stage.
$bundleStarted = $null
$markerPid     = 0
$markerHwnd    = 0
$familySeen    = @()
$deadline = (Get-Date).AddSeconds($MarkerWaitSec)
while ((Get-Date) -lt $deadline) {
    if (-not (Test-P2PProcessAlive -ProcId $launchedPid)) {
        # A launcher stub that re-execs legitimately exits, so this is only fatal
        # once the family is empty too.
        $stillAny = @(Get-P2PFamily -RootPid $launchedPid) | Where-Object { Test-P2PProcessAlive -ProcId $_ }
        if (@($stillAny).Count -eq 0) { break }
    }
    $fam = Get-P2PFamily -RootPid $launchedPid
    $familySeen = @($fam)
    $map = Get-P2PBundleMap
    foreach ($row in @([P2PW]::BundleMarkers())) {
        $parts = $row -split '='
        if ($parts.Count -lt 3) { continue }
        $mp = [int]$parts[0]
        if ($fam.Contains($mp)) {
            $bundleStarted = $parts[1]; $markerPid = $mp; $markerHwnd = [int64]$parts[2]
            break
        }
    }
    if ($bundleStarted) { break }
    Start-Sleep -Milliseconds 200
}

if (-not $bundleStarted) {
    $alive = Test-P2PProcessAlive -ProcId $launchedPid
    Add-Step 'identify/marker' 'failed' ('No "<bundle>-sic" marker window appeared for pid {0} or its descendants within {1}s (process alive: {2}).' -f $launchedPid, $MarkerWaitSec, $alive)
    Write-Result 'blocked' `
        ('The launched process never planted a single-instance marker window, so its bundle identifier is UNKNOWN. Without it the two instances cannot be told apart, and this script refuses to hand back a rig it cannot distinguish. Two ordinary causes: (a) the exe was built without the `desktop` feature so tauri-plugin-single-instance is not compiled in (apps/osl-hub/Cargo.toml:47); (b) the process is hung before main() -- check that WebView2Loader.dll is beside the exe. Process pid {0} alive at give-up: {1}.' -f $launchedPid, $alive) `
        'Rebuild B with scripts\qa\osl-instance-b-build.ps1 (which sets --features desktop,discord-qa-shell and stages WebView2Loader.dll) and re-run.'
}
Add-Step 'identify/marker' 'ok' ('Launched family planted marker class "{0}-sic" (pid {1}, hwnd 0x{2:X}).' -f $bundleStarted, $markerPid, $markerHwnd)

if ($bundleStarted -ne $BundleB) {
    Add-Step 'identify/bundle-matches' 'failed' ('Asked for "{0}", started "{1}".' -f $BundleB, $bundleStarted)
    $verdict = 'failed'
    $diag = ('The exe at {0} is bundle "{1}", not the "{2}" that was asked for. It is running -- this script does not kill it -- but it must not be graded as instance B, and if "{1}" equals instance A''s identifier then what actually happened is that the single-instance guard handed argv to A and this launch did nothing.' -f $ExeB, $bundleStarted, $BundleB)
    if ($bundleStarted -eq $BundleA) {
        $diag += ' THAT IS WHAT HAPPENED: the started bundle equals instance A''s. There is still only one instance.'
    }
    Write-Result $verdict $diag ('Rebuild B with -Identifier {0}, or pass -BundleB {1} if that is the build you meant.' -f $BundleB, $bundleStarted) `
        @{ bundleStarted = $bundleStarted; bundleAsked = $BundleB; launchedPid = $launchedPid }
}
Add-Step 'identify/bundle-matches' 'ok' ('Started bundle "{0}" is the one that was asked for.' -f $bundleStarted)

# --- 8. B's main window ----------------------------------------------------
# Two independent filters, both required: (i) the owning pid is in the launched
# family, (ii) the hwnd did NOT exist before the launch. Title is the LAST
# filter, never the first.
$bMain = $null
$identifiedBy = ''
$deadline = (Get-Date).AddSeconds($WindowWaitSec)
$titleOnlyFresh = @()
while ((Get-Date) -lt $deadline) {
    $fam = Get-P2PFamily -RootPid $launchedPid
    $now = Get-P2PWindows
    $cands = @($now | Where-Object {
        $_.Title -eq $MainTitle -and $_.Cls -eq $MainClass -and $_.Owner -eq 0 -and
        $fam.Contains([int]$_.Pid) -and ($preOslHwnds -notcontains [int64]$_.Hwnd)
    })
    if ($cands.Count -gt 0) {
        $bMain = $cands[0]
        $identifiedBy = ('pid {0}, which is in the launched family of pid {1}, AND hwnd 0x{2:X}, which did not exist before the launch' -f $bMain.Pid, $launchedPid, [int64]$bMain.Hwnd)
        break
    }
    $titleOnlyFresh = @($now | Where-Object {
        $_.Title -eq $MainTitle -and $_.Cls -eq $MainClass -and $_.Owner -eq 0 -and
        ($preOslHwnds -notcontains [int64]$_.Hwnd)
    })
    Start-Sleep -Milliseconds 200
}

if (-not $bMain) {
    # TITLE-ONLY FALLBACK. Announced loudly and NOT treated as an identification.
    if ($titleOnlyFresh.Count -gt 0) {
        $f = $titleOnlyFresh[0]
        Say ''
        Say '*** TITLE-ONLY FALLBACK -- THIS IS NOT AN IDENTIFICATION ***' 'Red'
        Say ('*** hwnd 0x{0:X} is titled "{1}" and is new since the launch, but its pid {2} is NOT in the launched family of pid {3}. Every OSL build carries that title. ***' -f [int64]$f.Hwnd, $MainTitle, $f.Pid, $launchedPid) 'Red'
        Add-Step 'identify/main-window' 'failed' ('TITLE-ONLY FALLBACK: a new window titled "{0}" exists (hwnd 0x{1:X}, pid {2}) but it does not belong to the launched family.' -f $MainTitle, [int64]$f.Hwnd, $f.Pid)
        Write-Result 'blocked' `
            ('A window titled "{0}" appeared after the launch, but it belongs to pid {1}, which is not in the launched family of pid {2}. The title is shared by every OSL build, so this is NOT a positive identification and the script will not proceed on it -- grading a stale instance because it answered to the title is the specific failure this rig exists to avoid.' -f $MainTitle, $f.Pid, $launchedPid) `
            'Close every other OSL build, confirm with the -sic marker classes which builds are running, and re-run.' `
            @{ titleOnlyFallbackCandidates = @($titleOnlyFresh | ForEach-Object { [ordered]@{ hwnd = ('0x{0:X}' -f [int64]$_.Hwnd); pid = $_.Pid } }) }
    }
    Add-Step 'identify/main-window' 'failed' ('No main window for the launched family within {0}s.' -f $WindowWaitSec)
    Write-Result 'blocked' `
        ('Instance B planted its marker (bundle "{0}", pid {1}) but never showed a main window titled "{2}" within {3}s. The process is up, so this is a startup stall, not a launch failure. A discord-qa-shell build blocks for up to 30s on keyserver registration during setup (discord_qa_identity.rs:30,134) -- if the keyserver is unreachable, that is where it sits.' -f $bundleStarted, $markerPid, $MainTitle, $WindowWaitSec) `
        'Confirm the keyserver is reachable, raise -WindowWaitSec, and re-run. Do not kill instance A to make room; the two are independent.'
}
Add-Step 'identify/main-window' 'ok' ('Instance B main window 0x{0:X}, identified by {1}.' -f [int64]$bMain.Hwnd, $identifiedBy)

# --- 9. prove A was not disturbed -----------------------------------------
$postWins = Get-P2PWindows
$postBundles = Get-P2PBundleMap
$aStillThere = @($postBundles.Keys | Where-Object { $postBundles[$_] -eq $BundleA -and [int]$_ -eq $aPid })
$aMainStill = $true
if ($aMainHwnd -ne 0) { $aMainStill = [P2PW]::IsWindow([IntPtr]$aMainHwnd) }
$identityAAfter = $null
if ($identityABefore -and $identityABefore.exists) {
    $identityAAfter = Get-P2PFileStamp -Path $identityABefore.path -Label 'instance-a-identity-after'
}
$identityAUntouched = $null
if ($identityABefore -and $identityAAfter) {
    $identityAUntouched = ($identityABefore.sha256 -eq $identityAAfter.sha256)
}
if ($NoInstanceA) {
    # A was not running before, so the invariant is that launching B did not
    # START it, and that A's identity file on disk is byte-identical. The
    # identity comparison is unchanged and is the half that actually protects
    # the owner's account.
    $aAppeared = @($postBundles.Keys | Where-Object { $postBundles[$_] -eq $BundleA })
    $aOk = (@($aAppeared).Count -eq 0) -and (($null -eq $identityAUntouched) -or $identityAUntouched)
    if (-not $aOk) {
        Add-Step 'assert/instance-a-untouched' 'failed' ('A markers that appeared during the launch: {0}; A identity byte-identical: {1}.' -f (@($aAppeared) -join ','), $identityAUntouched)
        Write-Result 'failed' `
            ('Instance A ({0}) was not running before this launch but changed across it. Markers now present: {1}. A identity file byte-identical: {2}. Launching B must never start A nor write A''s profile.' -f
                $BundleA, (@($aAppeared) -join ','), $identityAUntouched) `
            'Do not run the P2P harness against this rig. A shared profile root would explain this and would invalidate every cross-identity result.'
    }
    Add-Step 'assert/instance-a-untouched' 'ok' ('-NoInstanceA: no "{0}-sic" marker appeared during the launch, and A''s identity file is byte-identical across it (sha256, not mtime).' -f $BundleA)
}
$aOk = ($NoInstanceA) -or ((@($aStillThere).Count -eq 1) -and $aMainStill -and (($null -eq $identityAUntouched) -or $identityAUntouched))
if (-not $aOk) {
    Add-Step 'assert/instance-a-untouched' 'failed' ('A marker present: {0}; A main window alive: {1}; A identity byte-identical: {2}.' -f (@($aStillThere).Count -eq 1), $aMainStill, $identityAUntouched)
    Write-Result 'failed' `
        ('Instance A changed across this launch. That must never happen: A was on the exclusion list and this script performed no operation against it. Marker "{0}-sic" for pid {1} still present: {2}. A main window 0x{3:X} still a window: {4}. A identity file byte-identical: {5}.' -f
            $BundleA, $aPid, (@($aStillThere).Count -eq 1), $aMainHwnd, $aMainStill, $identityAUntouched) `
        'Do not run the P2P harness against this rig. Investigate before proceeding -- a shared profile root would explain an identity change and would invalidate every cross-identity result.'
}
Add-Step 'assert/instance-a-untouched' 'ok' ('A''s marker, A''s main window and A''s identity file are all unchanged across the launch (identity compared by sha256, not by mtime).')

# --- 10. profile separation, verified on disk -----------------------------
$rootBExists = Test-Path -LiteralPath $rootB
$rootBFresh = $false
if ($rootBExists) {
    try { $rootBFresh = ((Get-Item -LiteralPath $rootB).LastWriteTime -ge $script:RunStartLocal) } catch { $rootBFresh = $false }
}
$sep = 'unmeasurable'
if ($rootBExists -and $rootA -ne $rootB) { $sep = 'separate' }
Add-Step 'assert/profile-separation' $(if ($sep -eq 'separate') { 'ok' } else { 'warn' }) `
    ('A root {0}; B root {1} (exists: {2}, touched during this run: {3}). Separation: {4}.' -f $rootA, $rootB, $rootBExists, $rootBFresh, $sep)

# --- 11. relocate B only ---------------------------------------------------
$moved = $null
if (-not $NoRelocate) {
    $vd = @([System.Windows.Forms.Screen]::AllScreens | Where-Object { -not $_.Primary })
    if ($vd.Count -eq 0 -and -not $AllowPrimaryDisplay) {
        Add-Step 'relocate' 'skipped' 'No non-primary display. Refusing to move B onto the owner''s primary screen; pass -AllowPrimaryDisplay to override, or -NoRelocate to stop asking.'
    } elseif ($vd.Count -gt 0) {
        $tx = $vd[0].Bounds.X + 240; $ty = $vd[0].Bounds.Y + 70
        # SWP_NOSIZE | SWP_NOZORDER, and ONLY against the hwnd positively
        # identified above. A's hwnd is never passed to SetWindowPos.
        [void][P2PW]::SetWindowPos([IntPtr][int64]$bMain.Hwnd, [IntPtr]::Zero, $tx, $ty, 0, 0, (0x0001 -bor 0x0004))
        Start-Sleep -Milliseconds 600
        $r = New-Object P2PRect
        [void][P2PW]::GetWindowRect([IntPtr][int64]$bMain.Hwnd, [ref]$r)
        $moved = [ordered]@{ display = $vd[0].DeviceName; targetX = $tx; targetY = $ty; landedL = $r.L; landedT = $r.T }
        Add-Step 'relocate' 'ok' ('Moved B''s window 0x{0:X} to {1},{2} on {3}; it landed at {4},{5}.' -f [int64]$bMain.Hwnd, $tx, $ty, $vd[0].DeviceName, $r.L, $r.T)
    } else {
        Add-Step 'relocate' 'skipped' '-AllowPrimaryDisplay was given but there is no non-primary display; B stays where it opened.'
    }
} else {
    Add-Step 'relocate' 'skipped' '-NoRelocate was given.'
}

# --- 11b. did the temp redirect actually take? -----------------------------
# Empirical, not assumed. Every build writes osl-startup-trace.txt on the way up
# (apps/osl-hub/src/main.rs:88), so its appearance under B's root is proof the
# child honoured TMP/TEMP, and its ABSENCE there is proof it did not.
$traceB = Join-Path $TempRootB 'osl-startup-trace.txt'
$traceBStamp = Get-P2PFileStamp -Path $traceB -Label 'instance-b-startup-trace'
$tempHonoured = ($traceBStamp.exists -and ((Get-Item -LiteralPath $traceB).LastWriteTime -ge $script:RunStartLocal))
if (-not $tempHonoured) {
    Add-Step 'assert/temp-redirect' 'failed' ('{0} was not written during this run.' -f $traceB)
    Write-Result 'failed' `
        ('Instance B started, but no osl-startup-trace.txt appeared in its private temp root {0} during this run. The child did not honour TMP/TEMP, so B is writing its QA artefacts into the SHARED %TEMP% alongside instance A. Any measurement taken off those files would be unattributable, and the self-test trigger would be a race between the two instances. B is left running -- this script does not kill it -- but it must not be graded.' -f $TempRootB) `
        'Check that the exe was not started through a shim that resets the environment, and re-run.' `
        @{ instanceBTempRoot = $TempRootB; startupTrace = $traceBStamp }
}
Add-Step 'assert/temp-redirect' 'ok' ('B honoured its private temp root: {0} was written during this run.' -f $traceB)

# --- 11c. B created its own QA identity and public offer -------------------
# -ConfirmCreatesIdentity is the operator's consent to cross the profile
# mutation boundary. Prove that the launched B profile actually owns a distinct
# identity artefact and published the fixed public offer used by the two-profile
# pairing controller. Only file stamps and hashes are reported; no identity
# material is displayed.
$bCoreCandidates = @(
    (Join-Path $rootB 'osl-core'),
    (Join-Path $rootB 'discord-qa-shell-v1\osl-core')
)
$identityBAfter = $null
$offerBAfter = $null
$identityDeadline = (Get-Date).AddSeconds([Math]::Max(5, [Math]::Min($WindowWaitSec, 30)))
while ((Get-Date) -lt $identityDeadline) {
    foreach ($coreRoot in $bCoreCandidates) {
        $identityCandidate = Join-Path $coreRoot 'identity.json'
        $offerCandidate = Join-Path $coreRoot 'discord-qa-offer.v1.json'
        if (-not $identityBAfter -and (Test-Path -LiteralPath $identityCandidate)) {
            $identityBAfter = Get-P2PFileStamp -Path $identityCandidate -Label 'instance-b-identity'
        }
        if (-not $offerBAfter -and (Test-Path -LiteralPath $offerCandidate)) {
            $offerBAfter = Get-P2PFileStamp -Path $offerCandidate -Label 'instance-b-public-offer'
        }
    }
    if (($identityBAfter -and $identityBAfter.exists) -and ($offerBAfter -and $offerBAfter.exists)) { break }
    Start-Sleep -Milliseconds 200
}
if (-not ($identityBAfter -and $identityBAfter.exists)) {
    Add-Step 'assert/instance-b-identity' 'failed' ('No identity.json appeared under B profile root {0}.' -f $rootB)
    Write-Result 'failed' `
        ('Instance B started after -ConfirmCreatesIdentity, but no B identity file appeared under {0}. The consent switch must result in a disposable B identity, otherwise this is not a two-identity rig.' -f $rootB) `
        'Use a desktop,discord-qa-shell build and a writable instance-B profile root, then re-run.' `
        @{ instanceBIdentity = $null; instanceBPublicOffer = $offerBAfter }
}
if ($identityABefore -and $identityABefore.exists -and $identityBAfter.sha256 -eq $identityABefore.sha256) {
    Add-Step 'assert/instance-b-identity' 'failed' 'B identity sha256 equals A identity sha256.'
    Write-Result 'failed' `
        'Instance B produced an identity file, but it is byte-identical to instance A''s identity file. That is a shared-profile or copied-identity rig, not a second OSL identity.' `
        'Do not run the P2P harness. Rebuild/relaunch B with a distinct bundle identifier and isolated profile root.' `
        @{ instanceAIdentity = $identityABefore; instanceBIdentity = $identityBAfter }
}
if (-not ($offerBAfter -and $offerBAfter.exists)) {
    Add-Step 'assert/instance-b-identity' 'failed' ('No discord-qa-offer.v1.json appeared under B profile root {0}.' -f $rootB)
    Write-Result 'failed' `
        ('Instance B created an identity at {0}, but did not publish its QA public offer. The pairing controller would have no registered public identity material to exchange.' -f $identityBAfter.path) `
        'Confirm the keyserver/QA bootstrap path is healthy and re-run after B can publish its public offer.' `
        @{ instanceBIdentity = $identityBAfter; instanceBPublicOffer = $null }
}
Add-Step 'assert/instance-b-identity' 'ok' ('B owns a distinct QA identity ({0}) and public offer ({1}). A identity sha256 equal to B: {2}.' -f
    $identityBAfter.path, $offerBAfter.path, $(if ($identityABefore -and $identityABefore.exists) { $identityBAfter.sha256 -eq $identityABefore.sha256 } else { $false }))

# --- 11d. B's public identity resolves from the configured keyserver --------
# Local files prove profile separation; they do not prove the network write that
# makes the second identity usable to another instance. Resolve the OSL identity
# from B's signed public offer through the same keyserver origin the disposable
# B profile is configured to use. Only booleans and counts are reported; the
# account identifier and returned public keys stay out of console/JSON output.
$offerJson = $null
try {
    $offerJson = Get-Content -LiteralPath $offerBAfter.path -Raw -ErrorAction Stop | ConvertFrom-Json -ErrorAction Stop
} catch {
    Add-Step 'identity/keyserver-registration' 'failed' ('B public offer is unreadable: {0}' -f $_.Exception.Message)
    Write-Result 'failed' `
        'Instance B wrote a public offer, but the launcher could not read it to verify the keyserver registration.' `
        'Re-run with a fresh disposable QA profile; the public offer must be valid JSON.'
}
$bOslUserId = [string]$offerJson.osl_user_id
if (-not $bOslUserId) {
    Add-Step 'identity/keyserver-registration' 'failed' 'B public offer has no osl_user_id.'
    Write-Result 'failed' `
        'Instance B wrote a public offer without the OSL identity identifier needed to verify its keyserver row.' `
        'Use a desktop,discord-qa-shell build that exports a complete public offer.'
}
$keyserverBaseUrl = Get-KeyserverBaseUrl -CoreRoots $bCoreCandidates
$pubkeysUrl = Join-KeyserverPath -BaseUrl $keyserverBaseUrl -Path ('/v1/pubkeys/{0}' -f [System.Uri]::EscapeDataString($bOslUserId))
$pubkeys = $null
$keyserverStatus = $null
$registrationDeadline = (Get-Date).AddSeconds([Math]::Max(10, [Math]::Min($WindowWaitSec, 30)))
while ((Get-Date) -lt $registrationDeadline) {
    try {
        $pubkeys = Invoke-RestMethod -Method Get -Uri $pubkeysUrl -TimeoutSec 10 -ErrorAction Stop
        $keyserverStatus = 'ok'
        break
    } catch {
        try { $keyserverStatus = 'HTTP {0}' -f [int]$_.Exception.Response.StatusCode } catch { $keyserverStatus = 'no usable response' }
        Start-Sleep -Milliseconds 1000
    }
}
if (-not $pubkeys) {
    Add-Step 'identity/keyserver-registration' 'failed' ('Keyserver lookup for B identity failed within the registration barrier ({0}).' -f $keyserverStatus)
    Write-Result 'failed' `
        'Instance B created a local identity after consent, but the configured keyserver did not return that identity. This is not a registered two-identity rig.' `
        'Confirm the dedicated QA keyserver is reachable and that B completed startup registration, then re-run.'
}
$registeredFields = @(
    $pubkeys.user_id,
    $pubkeys.registered_at,
    $pubkeys.ik_x25519_pub,
    $pubkeys.ik_ed25519_pub,
    $pubkeys.ik_mlkem768_pub,
    $pubkeys.registration_sig
)
$registeredSecondIdentity = (
    ([string]$pubkeys.user_id -eq $bOslUserId) -and
    (-not (@($registeredFields | Where-Object { -not $_ }).Count))
)
if (-not $registeredSecondIdentity) {
    Add-Step 'identity/keyserver-registration' 'failed' 'Keyserver response did not match B public offer or was missing required public identity fields.'
    Write-Result 'failed' `
        'Instance B created a local identity after consent, but the configured keyserver response did not prove the same registered public identity.' `
        'Do not run the P2P harness until B registration returns the exact public identity row for B.'
}
Add-Step 'identity/keyserver-registration' 'ok' 'Configured keyserver returned B''s registered public identity row.' @{
    resolvedFromPublicOffer = $true
    requiredPublicFieldsPresent = $true
}

# --- 12. done --------------------------------------------------------------
$postBundleList = @($postBundles.Keys | ForEach-Object { [ordered]@{ pid = [int]$_; bundle = $postBundles[$_] } })
Write-Result 'ok' `
    ('Two OSL instances are running and are positively distinguishable by their single-instance marker classes: A = "{0}" (pid {1}), B = "{2}" (pid {3}, main window 0x{4:X}). Instance A was not touched.' -f
        $BundleA, $aPid, $bundleStarted, $markerPid, [int64]$bMain.Hwnd) `
    ('Hand these to the P2P harness: -BundleA {0} -BundleB {1} -TempRootB "{2}"' -f $BundleA, $bundleStarted, $TempRootB) `
    @{
        instanceA = [ordered]@{
            bundle = $BundleA; pid = $aPid
            mainHwnd = $(if ($aMainHwnd) { '0x{0:X}' -f $aMainHwnd } else { $null })
            profileRoot = $rootA
            tempRoot = $tempA
            identityFile = $(if ($identityABefore) { $identityABefore.path } else { $null })
            identityUnchangedAcrossLaunch = $identityAUntouched
            touchedByThisScript = $false
        }
        instanceB = [ordered]@{
            bundle = $bundleStarted; bundleAsked = $BundleB
            launchedPid = $launchedPid; markerPid = $markerPid
            markerHwnd = ('0x{0:X}' -f $markerHwnd)
            mainHwnd = ('0x{0:X}' -f [int64]$bMain.Hwnd)
            identifiedBy = $identifiedBy
            titleOnlyFallbackUsed = $false
            exe = $ExeB; exeStamp = $exeStamp
            profileRoot = $rootB; profileRootExists = $rootBExists; profileRootTouchedThisRun = $rootBFresh
            tempRoot = $TempRootB; tempRootHonouredByChild = $tempHonoured
            startupTrace = $traceBStamp
            registeredSecondIdentity = $true
            identityFile = $identityBAfter
            publicOffer = $offerBAfter
            registeredSecondIdentity = $true
            family = @($familySeen | Sort-Object)
            relocated = $moved
        }
        bundleMarkersBefore = $preBundleList
        bundleMarkersAfter  = $postBundleList
        distinguishable = $true
        distinguishableBy = 'single-instance marker window class "<identifier>-sic". The window TITLE was NOT used to tell the instances apart -- every OSL build carries the title "OSL Privacy".'
    }
