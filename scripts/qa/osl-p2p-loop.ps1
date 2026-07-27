<#
.SYNOPSIS
    Two-identity P2P verification runner. Drives a two-way exchange between two
    OSL Hub instances and emits one machine-diffable verdict at
    %TEMP%\osl-p2p.json.

.DESCRIPTION
    STATUS VOCABULARY (per step -- inherited from accept.ps1/loop.ps1, unchanged)
        pass          measured, and the measurement satisfies the step
        fail          measured, and the measurement violates the step
        unmeasurable  could NOT be measured. NEVER counts as green.
        blocked       the step CANNOT be measured by any run of this harness
                      against this build, because the product does not expose
                      the fact. Distinct from `unmeasurable`, which means "the
                      measurement was possible but did not happen this time".
                      Both are non-green; only `blocked` carries a file:line
                      naming the product change that would make it measurable.

    VERDICT VOCABULARY (overall.verdict)
        pass          every step is `pass`
        fail          at least one step was MEASURED and violated
        not-passing   a mix of pass and unmeasurable/blocked, no measured violation
        unmeasurable  nothing could be measured, but the rig was sane
        blocked       A PRECONDITION GATE FAILED. Nothing was measured, so no
                      statement is made about the product. "We could not test"
                      and "the product is broken" are different facts.

    THE SIX STEPS
        P1  A sends an encrypted message to the peer
        P2  B drains, decrypts and renders it
        P3  Receipts flow back to A (Received, then Opened)
        P4  A view-once message reveals exactly once on B, and is refused twice
        P5  A burn issued by A is applied on B, and B acknowledges
        P6  The eye on B paints a row B DID NOT SEND

    HOW IT DRIVES ANYTHING AT ALL
      There is no CLI entry to any Tauri command and no IPC socket. The design
      note is explicit (apps/osl-hub/src/main.rs:4851-4857): a start-up CLI flag
      was rejected because it cannot be re-run without relaunching, and because
      tauri-plugin-single-instance would swallow a second process's argv.
      The ONE non-GUI entry point is a file rendezvous:
        write  <temp>\osl-qa-selftest.request     (main.rs:4877, watcher :5555)
        read   <temp>\osl-qa-selftest.json        (main.rs:4878, partial+rename :4881)
      It drives exactly one verb -- `send_native_discord_qa_atomic_text` with the
      fixed plaintext at main.rs:4876 (drive_probe_send, main.rs:5443). There is
      no file trigger for DRAIN, for REVEAL, for BURN or for REHYDRATE. That is
      why P2..P6 are observation-only here and why several of them come back
      `blocked` with a named product change rather than a red or a green.

    WHY THE TWO TEMP ROOTS MATTER
      Every QA artefact is written to std::env::temp_dir() under a fixed,
      unqualified name, and the self-test request is a global rendezvous whose
      first consumer wins it (main.rs:5560). Two instances sharing one %TEMP%
      would interleave their append trails, clobber each other's overwrite files
      and race for the trigger. osl-launch-instance-b.ps1 gives B a private temp
      root; this harness must be told where it is. A shared root is a `blocked`
      precondition, not a warning.

    HARNESS DISCIPLINE (every rule below was paid for)
      H1  RUN START IS CAPTURED ONCE and every file-backed artefact is graded
          against it. There is no fixed staleness window -- a fixed window is
          what once let a 4158-second-old receipt be reported as a live failure.
          An artefact that predates run start is `unmeasurable` with its age
          stated: never pass, never fail.
      H2  APPEND-ONLY TRAILS ARE SLICED BY BYTE OFFSET. The trail length is
          captured at run start and only the bytes appended after that point are
          ever parsed. mtime alone is not enough for an append trail: the file is
          fresh the moment ANY instance writes to it.
      H3  OVERWRITE-IN-PLACE OUTPUT IS CLAIMED FIRST. The verdict file is stamped
          `in-progress` before anything else runs, so a run that dies cannot leave
          the previous run's verdict looking current. The one case this cannot
          cover is a PARSE error, because then nothing runs at all -- so
          parse-check before invoking:
            $e=$null;[void][System.Management.Automation.Language.Parser]::ParseFile($p,[ref]$null,[ref]$e);$e
          A dangling `else` has silently killed two runs.
      H4  PRECONDITIONS FAIL FAST with `blocked`, and the steps are not run at
          all. A run that reports six rows of unmeasurable when the rig was never
          assembled is worse than no run.
      H5  IDENTITY IS THE -sic MARKER CLASS, NEVER THE WINDOW TITLE. Every OSL
          build is titled "OSL Privacy".
      H6  BOTH INSTANCES ARE ASSERTED ALIVE at every step boundary, and the
          Discord process set is compared before and after.

    SAFETY
      * NO POINTER INPUT AND NO KEYBOARD INJECTION. This script synthesises no
        input of any kind: there is no SendInput, no keybd_event, no mouse_event
        and no SetCursorPos anywhere in it or in osl-p2p-win32.ps1. It writes
        files and reads files. The only actuator the win32 layer exposes is
        PostMessage, and this harness does not call it.
      * Never starts, stops, signals or closes a Discord process. Discord's pid
        set is captured before and after and compared; a change is reported as a
        defect OF THIS RUN.
      * Never logs plaintext, cover text, draft text, key material or a
        conversation name. Only counts, labels, hashes, lengths and booleans are
        read out of artefacts, and only fixed strings are written.
      * Refuses to run without -ConfirmDriveLiveConversation, because P1 posts a
        real message into whatever conversation instance A is adopted to.
#>

[CmdletBinding()]
param(
    [string]$BundleA = 'org.oslprivacy.hub',
    [Parameter(Mandatory = $true)][string]$BundleB,

    # The exact executable this lane staged and launched, as reported by
    # osl-launch-instance-b.ps1. MANDATORY: every verb this harness drives is
    # refused unless the target process is running precisely this binary, so
    # "whichever OSL happens to be up" is not expressible. See the target
    # preflight in Invoke-SelftestVerb.
    [Parameter(Mandatory = $true)][string]$ExeB,

    [string]$TempRootA = $env:TEMP,
    [Parameter(Mandatory = $true)][string]$TempRootB,

    [string]$JsonOut = (Join-Path $env:TEMP 'osl-p2p.json'),

    [string]$MainTitle = 'OSL Privacy',
    [string]$MainClass = 'Tauri Window',
    [string]$DiscordProcessPattern = '^Discord$|^DiscordPTB$|^DiscordCanary$|^DiscordDevelopment$',

    # P1 posts a real, human-paced message into the conversation instance A is
    # adopted to. That is the owner's decision, never this script's.
    [switch]$ConfirmDriveLiveConversation,

    # P2..P6 have no file trigger. With this switch the harness waits and prints
    # what the operator must click; without it, it observes for -ObserveSec and
    # reports what it saw.
    [switch]$OperatorDrivesReceiveSide,

    [int]$SendTriggerWaitSec = 20,
    [int]$SendVerdictWaitSec = 180,
    [int]$ObserveSec         = 90,
    [int]$OperatorObserveSec = 300,
    [int]$TimeoutSec         = 900,
    [switch]$Quiet
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Off

$here = Split-Path -Parent $MyInvocation.MyCommand.Path
. (Join-Path $here 'osl-p2p-win32.ps1')

# H1. Run start, captured once. LastWriteTime and Get-Date are both local time,
# so they compare directly. No slack is allowed: slack is what hides staleness.
$script:Clock         = [System.Diagnostics.Stopwatch]::StartNew()
$script:CapMs         = $TimeoutSec * 1000
$script:RunStartLocal = Get-Date
$script:RunStartIso   = $script:RunStartLocal.ToString('o')

$script:Steps          = @()
$script:Gates          = @()
$script:Criteria       = @()
$script:ArtefactAudit  = @()
$script:TrailBaselines = @{}
$script:ProductGaps    = @()

function TimeLeftMs { return ($script:CapMs - $script:Clock.ElapsedMilliseconds) }
function Say { param([string]$M, [string]$C = 'Gray') if (-not $Quiet) { Write-Host $M -ForegroundColor $C } }

function Add-Step {
    param([string]$Name, [string]$Result, [string]$Detail, $Extra = $null)
    $o = [ordered]@{ step = $Name; result = $Result; detail = $Detail; atMs = [int]$script:Clock.ElapsedMilliseconds }
    if ($Extra) { foreach ($k in $Extra.Keys) { $o[$k] = $Extra[$k] } }
    $script:Steps += [pscustomobject]$o
    $col = switch ($Result) { 'ok' { 'Green' } 'failed' { 'Red' } 'skipped' { 'DarkGray' } default { 'Yellow' } }
    Say ('[{0,-28}] {1,-8} {2}' -f $Name, $Result, $Detail) $col
}

function Add-Gate {
    param([string]$Id, [string]$Name, [bool]$Ok, [string]$Detail, $Extra = $null)
    $o = [ordered]@{ gate = $Id; name = $Name; ok = $Ok; detail = $Detail; atMs = [int]$script:Clock.ElapsedMilliseconds }
    if ($Extra) { foreach ($k in $Extra.Keys) { $o[$k] = $Extra[$k] } }
    $script:Gates += [pscustomobject]$o
    Add-Step ('gate/' + $Id) $(if ($Ok) { 'ok' } else { 'failed' }) $Detail
    return $Ok
}

# ---------------------------------------------------------------------------
# H1 -- artefact freshness. Every criterion that reads a file goes through here.
# FromThisRun is true only if the file was written at or after run start. A
# false FromThisRun makes the reading criterion `unmeasurable` with the age
# stated -- never pass, never fail. A violation recorded an hour ago is not this
# run's violation.
# ---------------------------------------------------------------------------
function Get-Artefact {
    param([string]$Path, [string]$Label, [switch]$NoText)
    $o = [pscustomobject]@{
        Label = $Label; Path = $Path; Exists = $false; AgeSec = $null
        WrittenIso = $null; FromThisRun = $false; PredatesRunBySec = $null
        Text = $null; Json = $null; Sha256 = $null; Detail = ''
    }
    if (-not $Path) { $o.Detail = ('No path configured for the {0} artefact.' -f $Label); return $o }
    if (-not (Test-Path -LiteralPath $Path)) {
        $o.Detail = ('The {0} artefact does not exist: {1}' -f $Label, $Path); return $o
    }
    # No -ErrorAction SilentlyContinue: it has swallowed a real error here twice.
    try { $fi = Get-Item -LiteralPath $Path -ErrorAction Stop } catch {
        $o.Detail = ('The {0} artefact at {1} could not be stat-ed: {2}' -f $Label, $Path, $_.Exception.Message); return $o
    }
    $o.Exists      = $true
    $o.AgeSec      = [int]((Get-Date) - $fi.LastWriteTime).TotalSeconds
    $o.WrittenIso  = $fi.LastWriteTime.ToString('o')
    $o.FromThisRun = ($fi.LastWriteTime -ge $script:RunStartLocal)
    if (-not $o.FromThisRun) { $o.PredatesRunBySec = [int]($script:RunStartLocal - $fi.LastWriteTime).TotalSeconds }
    try { $o.Sha256 = (Get-FileHash -LiteralPath $Path -Algorithm SHA256 -ErrorAction Stop).Hash } catch { }
    if (-not $NoText) {
        # Stale CONTENT is deliberately never returned for grading. It is still
        # audited, so "there is an old one" remains visible.
        if ($o.FromThisRun) {
            try { $o.Text = (Get-Content -LiteralPath $Path -Raw -ErrorAction Stop) } catch { $o.Text = $null }
            if ($o.Text -and $Path -match '\.json$') {
                try { $o.Json = ($o.Text | ConvertFrom-Json -ErrorAction Stop) } catch { $o.Json = $null }
            }
        }
    }
    if ($o.FromThisRun) {
        $o.Detail = ('The {0} artefact was written {1}s ago ({2}), during this run (started {3}), so it describes this run.' -f
            $Label, $o.AgeSec, $o.WrittenIso, $script:RunStartIso)
    } else {
        $o.Detail = ('STALE: the {0} artefact at {1} was written {2}s ago ({3}), which is {4}s BEFORE this run started ({5}). It records a PREVIOUS run and cannot be graded as this run''s result -- reported unmeasurable, neither pass nor fail.' -f
            $Label, $Path, $o.AgeSec, $o.WrittenIso, $o.PredatesRunBySec, $script:RunStartIso)
    }
    $script:ArtefactAudit += [ordered]@{
        artefact = $Label; path = $Path; exists = $o.Exists; ageSec = $o.AgeSec
        written = $o.WrittenIso; writtenDuringThisRun = $o.FromThisRun
        predatesRunStartBySec = $o.PredatesRunBySec; sha256 = $o.Sha256
        gradeable = ($o.Exists -and $o.FromThisRun); note = $o.Detail
    }
    return $o
}

# ---------------------------------------------------------------------------
# H2 -- append-only trails. mtime is NOT sufficient for these: the file is
# "fresh" the moment anything writes to it, including the other instance or a
# previous stage of this same run. Baseline the LENGTH at run start and only
# ever parse the bytes appended after that offset.
# ---------------------------------------------------------------------------
function Set-TrailBaseline {
    param([string]$Path, [string]$Label)
    $len = 0
    if (Test-Path -LiteralPath $Path) {
        try { $len = [int64](Get-Item -LiteralPath $Path -ErrorAction Stop).Length } catch { $len = 0 }
    }
    $script:TrailBaselines[$Label] = [pscustomobject]@{ Path = $Path; Baseline = $len }
    return $len
}

function Get-TrailSince {
    param([string]$Label)
    $b = $script:TrailBaselines[$Label]
    $o = [pscustomobject]@{ Label = $Label; Path = $null; Baseline = $null; NowLength = $null; Lines = @(); Grew = $false; Detail = '' }
    if (-not $b) { $o.Detail = ('No baseline was taken for the {0} trail, so nothing appended to it can be attributed to this run.' -f $Label); return $o }
    $o.Path = $b.Path; $o.Baseline = $b.Baseline
    if (-not (Test-Path -LiteralPath $b.Path)) {
        $o.Detail = ('The {0} trail does not exist: {1}' -f $Label, $b.Path); return $o
    }
    try {
        $fs = [System.IO.File]::Open($b.Path, 'Open', 'Read', 'ReadWrite')
        try {
            $o.NowLength = $fs.Length
            if ($fs.Length -gt $b.Baseline) {
                [void]$fs.Seek($b.Baseline, 'Begin')
                $buf = New-Object byte[] ($fs.Length - $b.Baseline)
                [void]$fs.Read($buf, 0, $buf.Length)
                $txt = [System.Text.Encoding]::UTF8.GetString($buf)
                $o.Lines = @($txt -split "`r?`n" | Where-Object { $_ -ne '' })
                $o.Grew = $true
            }
        } finally { $fs.Close() }
    } catch {
        $o.Detail = ('The {0} trail at {1} could not be read: {2}' -f $Label, $b.Path, $_.Exception.Message); return $o
    }
    if ($o.Grew) {
        $o.Detail = ('{0} line(s) were appended to the {1} trail after this run started (baseline offset {2} bytes, now {3}).' -f
            $o.Lines.Count, $Label, $o.Baseline, $o.NowLength)
    } else {
        $o.Detail = ('Nothing was appended to the {0} trail during this run (it is still {1} bytes). Any content it holds predates run start and is not gradeable.' -f
            $Label, $o.Baseline)
    }
    $script:ArtefactAudit += [ordered]@{
        artefact = ($Label + ' (append trail)'); path = $b.Path; exists = $true
        baselineBytesAtRunStart = $b.Baseline; bytesNow = $o.NowLength
        linesAppendedDuringThisRun = $o.Lines.Count
        gradeable = $o.Grew; note = $o.Detail
    }
    return $o
}

function New-Criterion {
    param(
        [string]$Id, [string]$Name, [string]$Status, $Measured,
        [string]$Detail, [string]$ProductChange = '', $Extra = $null
    )
    $o = [ordered]@{
        id = $Id; name = $Name; status = $Status
        pass = ($Status -eq 'pass'); green = ($Status -eq 'pass')
        measured = $Measured; detail = $Detail
    }
    if ($ProductChange) {
        $o['productChangeRequired'] = $ProductChange
        $script:ProductGaps += [ordered]@{ step = $Id; change = $ProductChange }
    }
    if ($Extra) { foreach ($k in $Extra.Keys) { $o[$k] = $Extra[$k] } }
    $script:Criteria += [pscustomobject]$o
    $col = switch ($Status) { 'pass' { 'Green' } 'fail' { 'Red' } 'blocked' { 'Magenta' } default { 'Yellow' } }
    Say ('  {0,-4} {1,-14} {2}' -f $Id, $Status.ToUpper(), $Name) $col
    Say ('       {0}' -f $Detail) 'DarkGray'
    return $o
}

# ---------------------------------------------------------------------------
# Instance discovery. H5: the -sic marker class, never the title.
# ---------------------------------------------------------------------------
function Resolve-Instance {
    param([string]$Bundle, [string]$Side)
    $map  = Get-P2PBundleMap
    $wins = Get-P2PWindows
    $pids = @($map.Keys | Where-Object { $map[$_] -eq $Bundle })
    $o = [pscustomobject]@{
        Side = $Side; Bundle = $Bundle; Found = $false; Pid = 0; MainHwnd = 0
        Detail = ''; AllBundles = @($map.Keys | ForEach-Object { [ordered]@{ pid = [int]$_; bundle = $map[$_] } })
    }
    if ($pids.Count -eq 0) {
        $o.Detail = ('No single-instance marker window of class "{0}-sic" exists, so instance {1} is not running (or is a build that does not plant one). Marker classes actually present: {2}' -f
            $Bundle, $Side, $(if ($o.AllBundles.Count) { (@($o.AllBundles | ForEach-Object { $_.bundle }) -join ', ') } else { '(none)' }))
        return $o
    }
    if ($pids.Count -gt 1) {
        $o.Detail = ('{0} processes claim the marker class "{1}-sic" (pids {2}). That should be impossible -- the guard is a named mutex plus that window -- and it means instance {3} cannot be identified.' -f
            $pids.Count, $Bundle, ($pids -join ','), $Side)
        return $o
    }
    $o.Pid = [int]$pids[0]
    $mains = @($wins | Where-Object { $_.Pid -eq $o.Pid -and $_.Title -eq $MainTitle -and $_.Cls -eq $MainClass -and $_.Owner -eq 0 })
    if ($mains.Count -gt 0) { $o.MainHwnd = [int64]$mains[0].Hwnd }
    $o.Found = $true
    $o.Detail = ('Instance {0} is pid {1}, identified by its marker class "{2}-sic". Main window: {3}. The window TITLE "{4}" was not used -- every OSL build carries it.' -f
        $Side, $o.Pid, $Bundle, $(if ($o.MainHwnd) { '0x{0:X}' -f $o.MainHwnd } else { 'none visible (minimized to tray)' }), $MainTitle)
    return $o
}

# H6. Both instances asserted alive at every step boundary.
$script:VanishDetail = ''
function Assert-BothAlive {
    param([string]$Stage, $A, $B)
    $aOk = Test-P2PProcessAlive -ProcId $A.Pid
    $bOk = Test-P2PProcessAlive -ProcId $B.Pid
    if ($aOk -and $bOk) { return $true }
    $what = @()
    if (-not $aOk) { $what += ('instance A (pid {0}, bundle {1}) has exited' -f $A.Pid, $A.Bundle) }
    if (-not $bOk) { $what += ('instance B (pid {0}, bundle {1}) has exited' -f $B.Pid, $B.Bundle) }
    $script:VanishDetail = ('AN INSTANCE UNDER TEST DISAPPEARED at stage "{0}", {1} ms into the run: {2}. Nothing measured after this point is about the rig that was assembled.' -f
        $Stage, [int]$script:Clock.ElapsedMilliseconds, ($what -join ' and '))
    Add-Step 'instance-vanished' 'failed' $script:VanishDetail
    return $false
}

# The receipt root. main.rs:5859 sets the keystore base dir to
# <app_config_dir>/osl-core, and a discord-qa-shell build remaps config_dir to
# <app_config_dir>/discord-qa-shell-v1 when the ordinary profile carries a
# password marker (main.rs:5844-5851). So there are exactly two candidates, and
# which one is live is a fact to be READ, not assumed.
function Resolve-ReceiptRoot {
    param([string]$Bundle, [string]$Side)
    $base = Join-Path $env:APPDATA $Bundle
    $cands = @(
        (Join-Path $base 'discord-qa-shell-v1\osl-core'),
        (Join-Path $base 'osl-core')
    )
    $live = @($cands | Where-Object { Test-Path -LiteralPath (Join-Path $_ 'discord-qa-offer.v1.json') })
    $o = [pscustomobject]@{ Side = $Side; Bundle = $Bundle; Root = $null; Candidates = $cands; Detail = '' }
    if ($live.Count -eq 1) {
        $o.Root = $live[0]
        $o.Detail = ('Instance {0}''s QA profile root is {1} (it is the only candidate holding discord-qa-offer.v1.json).' -f $Side, $o.Root)
        return $o
    }
    if ($live.Count -gt 1) {
        $o.Detail = ('AMBIGUOUS: both candidate roots for instance {0} hold discord-qa-offer.v1.json ({1}). Grading either would be a guess about which profile the running process actually opened.' -f
            $Side, ($live -join ' AND '))
        return $o
    }
    $existing = @($cands | Where-Object { Test-Path -LiteralPath $_ })
    $o.Detail = ('No QA profile root found for instance {0}. Neither candidate holds discord-qa-offer.v1.json, which every discord-qa-shell build writes on startup (apps/osl-hub/src/main.rs:5913 -> discord_qa_identity.rs:261). Candidates checked: {1}. Directories that exist at all: {2}. The usual cause is that this instance was NOT built with --features discord-qa-shell, in which case it emits no QA artefacts whatsoever and nothing here can be measured.' -f
        $Side, ($cands -join ' | '), $(if ($existing.Count) { ($existing -join ' | ') } else { '(none)' }))
    return $o
}

# ---------------------------------------------------------------------------
# Report writers
# ---------------------------------------------------------------------------
$script:DiscordPidsBefore = Get-P2PDiscordPids

function Get-EnvironmentLeg {
    $after = Get-P2PDiscordPids
    return [ordered]@{
        discordPidsBefore = $script:DiscordPidsBefore
        discordPidsAfter  = $after
        discordProcessSetUnchanged = ((($script:DiscordPidsBefore -join ',')) -eq (($after -join ',')))
        note = 'Discord''s process set is compared as a SORTED PID LIST, not as a count, so a restart that happens to keep the count the same is still caught. A change here is a defect OF THIS RUN: nothing in this harness starts, stops, signals or closes a Discord process.'
        inputSynthesised = 'none'
        inputNote = 'This harness performs NO pointer input and NO keyboard injection. It writes files and reads files. There is no SendInput, keybd_event, mouse_event or SetCursorPos in this script or in osl-p2p-win32.ps1.'
    }
}

function Write-Blocked {
    param([string]$BlockedBy, [string]$Diagnosis, [string]$Remedy, $Extra = $null)
    $payload = [ordered]@{
        schemaVersion = 1
        tool          = 'osl-p2p-loop'
        runStartedAt  = $script:RunStartIso
        durationMs    = [int]$script:Clock.ElapsedMilliseconds
        overall = [ordered]@{
            verdict = 'blocked'; blockedBy = $BlockedBy; diagnosis = $Diagnosis; remedy = $Remedy
            passed = 0; failed = 0; unmeasurable = 0; blocked = 0; total = 0
            stepsRun = $false
            note = 'verdict "blocked" means a PRECONDITION failed: the two-instance rig could not be put into a state where the product is observable, so NOTHING was measured and NO statement is made about the product. This is deliberately not "fail". The six steps were not run, so there are no rows to read.'
        }
        preconditionGate = $script:Gates
        preconditions    = $script:Steps
        artefactAudit    = $script:ArtefactAudit
        environment      = (Get-EnvironmentLeg)
        steps            = @()
        diffKey          = ('BLOCKED:' + $BlockedBy)
    }
    if ($Extra) { foreach ($k in $Extra.Keys) { $payload[$k] = $Extra[$k] } }
    $payload | ConvertTo-Json -Depth 14 | Out-File -LiteralPath $JsonOut -Encoding utf8
    Say ''
    Say '==============================================================' 'Red'
    Say 'RUN BLOCKED -- NOTHING WAS MEASURED, SO NOTHING IS CLAIMED' 'Red'
    Say '==============================================================' 'Red'
    Say ('blockedBy : {0}' -f $BlockedBy) 'Yellow'
    Say ('diagnosis : {0}' -f $Diagnosis) 'Yellow'
    Say ('remedy    : {0}' -f $Remedy) 'Cyan'
    Say 'The six P2P steps were NOT run. This is "blocked", not "fail".' 'Red'
    Say ('diffKey: BLOCKED:{0}' -f $BlockedBy)
    Say ('JSON written to {0}' -f $JsonOut)
    exit 2
}

function Write-Final {
    $pass = @($script:Criteria | Where-Object { $_.status -eq 'pass' }).Count
    $fail = @($script:Criteria | Where-Object { $_.status -eq 'fail' }).Count
    $unm  = @($script:Criteria | Where-Object { $_.status -eq 'unmeasurable' }).Count
    $blk  = @($script:Criteria | Where-Object { $_.status -eq 'blocked' }).Count
    $tot  = $script:Criteria.Count

    $verdict = 'unmeasurable'
    if ($fail -gt 0) { $verdict = 'fail' }
    elseif ($pass -eq $tot -and $tot -gt 0) { $verdict = 'pass' }
    elseif ($pass -gt 0) { $verdict = 'not-passing' }

    $diffKey = (@($script:Criteria | ForEach-Object { $_.id + '=' + $_.status }) -join ';')

    $payload = [ordered]@{
        schemaVersion = 1
        tool          = 'osl-p2p-loop'
        runStartedAt  = $script:RunStartIso
        durationMs    = [int]$script:Clock.ElapsedMilliseconds
        overall = [ordered]@{
            verdict = $verdict
            passed = $pass; failed = $fail; unmeasurable = $unm; blocked = $blk; total = $tot
            stepsRun = $true
            note = 'A step is green ONLY when status == "pass". "unmeasurable" (the measurement was possible but did not happen this run) and "blocked" (the product does not expose the fact, so no run of this harness can measure it) are both non-green and neither is ever counted toward passed. Every "blocked" step carries productChangeRequired with a file:line.'
        }
        preconditionGate = $script:Gates
        preconditions    = $script:Steps
        steps            = $script:Criteria
        productChangesRequired = $script:ProductGaps
        artefactAudit    = $script:ArtefactAudit
        instanceLifecycle = [ordered]@{
            disappearedMidRun = ($script:VanishDetail -ne '')
            disappearedDetail = $(if ($script:VanishDetail) { $script:VanishDetail } else { $null })
        }
        environment = (Get-EnvironmentLeg)
        diffKey = $diffKey
    }
    $payload | ConvertTo-Json -Depth 14 | Out-File -LiteralPath $JsonOut -Encoding utf8

    $col = switch ($verdict) { 'pass' { 'Green' } 'fail' { 'Red' } default { 'Yellow' } }
    Say ''
    Say '==============================================================' $col
    Say ('P2P VERDICT: {0}   pass={1} fail={2} unmeasurable={3} blocked={4} of {5}' -f $verdict.ToUpper(), $pass, $fail, $unm, $blk, $tot) $col
    Say '==============================================================' $col
    if ($script:ProductGaps.Count -gt 0) {
        Say 'PRODUCT CHANGES REQUIRED before these steps can ever be measured:' 'Magenta'
        foreach ($g in $script:ProductGaps) { Say ('  {0}: {1}' -f $g.step, $g.change) 'Magenta' }
    }
    $env2 = Get-EnvironmentLeg
    if (-not $env2.discordProcessSetUnchanged) {
        Say 'DEFECT OF THIS RUN: Discord''s process set CHANGED across the run.' 'Red'
    }
    Say ('diffKey: {0}' -f $diffKey)
    Say ('JSON written to {0}' -f $JsonOut)
    if ($verdict -eq 'pass') { exit 0 } elseif ($verdict -eq 'fail') { exit 1 } else { exit 3 }
}

# ===========================================================================
# H3 -- claim the output file BEFORE anything else
# ===========================================================================
$prevWritten = $null
if (Test-Path -LiteralPath $JsonOut) { try { $prevWritten = (Get-Item -LiteralPath $JsonOut).LastWriteTime.ToString('o') } catch { $prevWritten = 'unreadable' } }
try {
    ([ordered]@{
        schemaVersion = 1; tool = 'osl-p2p-loop'; runStartedAt = $script:RunStartIso
        overall = [ordered]@{ verdict = 'in-progress'
            note = 'THIS RUN HAS NOT FINISHED. If you are reading this, the run that started at runStartedAt died before writing its verdict. Do NOT read any steps from it and do not treat the previous run''s numbers as current.' }
        previousFileWritten = $prevWritten
        steps = @()
    } | ConvertTo-Json -Depth 6) | Out-File -LiteralPath $JsonOut -Encoding utf8
} catch {
    Say ('WARNING: could not stamp an in-progress marker over {0}: {1}' -f $JsonOut, $_.Exception.Message) 'Yellow'
}

Say '=== OSL two-identity P2P verification ===' 'Cyan'
Say ('run started {0}' -f $script:RunStartIso) 'DarkGray'

# ===========================================================================
# H4 -- THE PRECONDITION GATE. Runs BEFORE any step. Each check is bounded.
# ===========================================================================

# G0 consent -------------------------------------------------------------
if (-not (Add-Gate 'consent' 'The operator has authorised a real send' $ConfirmDriveLiveConversation.IsPresent `
    $(if ($ConfirmDriveLiveConversation) { 'Authorised.' } else { 'Not authorised.' }))) {
    Write-Blocked 'consent' `
        'Step P1 posts a real, human-paced message into whatever Discord conversation instance A is adopted to. That is a live conversation with a real peer, and it is the owner''s decision, not this script''s.' `
        'Re-run with -ConfirmDriveLiveConversation once you are content for A to post into its adopted conversation.'
}

# G1 the two instances, distinguishable ------------------------------------
if ($BundleA -eq $BundleB) {
    [void](Add-Gate 'distinct-bundles' 'A and B have different identifiers' $false ('Both are "{0}".' -f $BundleA))
    Write-Blocked 'distinct-bundles' `
        ('-BundleA and -BundleB are both "{0}". Two instances with one identifier cannot coexist: the second finds the "{0}-sic" marker, hands its argv to the first over WM_COPYDATA and exits, and they would share one %APPDATA%\{0} identity anyway. There is no second identity to verify against.' -f $BundleA) `
        'Build B with a different identifier (scripts\qa\osl-instance-b-build.ps1) and launch it with scripts\qa\osl-launch-instance-b.ps1.'
}
$A = Resolve-Instance -Bundle $BundleA -Side 'A'
$B = Resolve-Instance -Bundle $BundleB -Side 'B'
if (-not (Add-Gate 'instance-a' 'Instance A is running and identified' $A.Found $A.Detail)) {
    Write-Blocked 'instance-a' $A.Detail 'Start instance A, confirm it is adopted to the intended Discord conversation, and re-run.'
}
if (-not (Add-Gate 'instance-b' 'Instance B is running and identified' $B.Found $B.Detail)) {
    Write-Blocked 'instance-b' $B.Detail 'Run scripts\qa\osl-launch-instance-b.ps1 first; it reports the bundle it actually started.'
}
if ($A.Pid -eq $B.Pid) {
    [void](Add-Gate 'two-processes' 'A and B are different processes' $false ('Both resolved to pid {0}.' -f $A.Pid))
    Write-Blocked 'two-processes' ('Instances A and B both resolved to pid {0}. There is only one process, so there is only one identity and nothing cross-identity can be proven.' -f $A.Pid) `
        'Confirm with the -sic marker classes which builds are actually running.'
}
[void](Add-Gate 'two-processes' 'A and B are different processes' $true ('A = pid {0} ("{1}"), B = pid {2} ("{3}"). Distinguished by marker class, not by window title.' -f $A.Pid, $BundleA, $B.Pid, $BundleB))

# G2 separate temp roots ---------------------------------------------------
$tempSame = ([System.IO.Path]::GetFullPath($TempRootA).TrimEnd('\') -eq [System.IO.Path]::GetFullPath($TempRootB).TrimEnd('\'))
if (-not (Add-Gate 'separate-temp-roots' 'The two instances do not share a temp root' (-not $tempSame) `
    $(if ($tempSame) { ('Both are {0}.' -f $TempRootA) } else { ('A -> {0}; B -> {1}.' -f $TempRootA, $TempRootB) }))) {
    Write-Blocked 'separate-temp-roots' `
        ('Both instances are reported to use the temp root {0}. Every QA artefact is written to std::env::temp_dir() under a fixed unqualified name: osl-startup-trace.txt and osl-discord-qa-send-stage.txt are APPEND trails that would interleave with no way to attribute a line to an instance; osl-discord-qa-overlay-stage.txt, osl-discord-qa-composer-zorder.txt and osl-qa-selftest.json are OVERWRITE files that would clobber each other; and osl-qa-selftest.request is a global rendezvous whose first consumer wins it (apps/osl-hub/src/main.rs:5560), so a trigger meant for A could be eaten by B. Nothing measured off those files would mean anything.' -f $TempRootA) `
        'Relaunch B through scripts\qa\osl-launch-instance-b.ps1, which gives it a private TMP/TEMP, and pass that root as -TempRootB.'
}

# G3 both are discord-qa-shell builds, with a resolvable profile root -------
$rA = Resolve-ReceiptRoot -Bundle $BundleA -Side 'A'
$rB = Resolve-ReceiptRoot -Bundle $BundleB -Side 'B'
if (-not (Add-Gate 'qa-profile-a' 'Instance A''s QA profile root resolves' ($null -ne $rA.Root) $rA.Detail)) {
    Write-Blocked 'qa-profile-a' $rA.Detail 'Rebuild A with --features discord-qa-shell, or point -BundleA at the build that has it. A production binary emits no QA artefacts at all, so none of the six steps can be observed against one.'
}
if (-not (Add-Gate 'qa-profile-b' 'Instance B''s QA profile root resolves' ($null -ne $rB.Root) $rB.Detail)) {
    Write-Blocked 'qa-profile-b' $rB.Detail 'Rebuild B with --features discord-qa-shell (scripts\qa\osl-instance-b-build.ps1 does this).'
}
if ($rA.Root -eq $rB.Root) {
    [void](Add-Gate 'separate-profiles' 'A and B have separate identity roots' $false ('Both resolved to {0}.' -f $rA.Root))
    Write-Blocked 'separate-profiles' `
        ('Instances A and B resolved to the SAME identity root: {0}. They are therefore one identity wearing two processes, and every cross-identity result would be a self-test. This is the single most dangerous false green available in this rig.' -f $rA.Root) `
        'Confirm the two identifiers differ and that %APPDATA%\<identifier>\ really is distinct for each.'
}
[void](Add-Gate 'separate-profiles' 'A and B have separate identity roots' $true ('A -> {0}; B -> {1}.' -f $rA.Root, $rB.Root))

# G4 pairing: each instance has verified the OTHER, and it is mutual --------
# This is a fully measurable gate and it costs nothing: the files are written by
# the QA bootstrap (discord_qa_identity.rs:261). It is checked BEFORE any send,
# because "the message never arrived" and "the two were never paired" are
# different facts and only one of them is a product defect.
$offerA  = Get-Artefact -Path (Join-Path $rA.Root 'discord-qa-offer.v1.json')          -Label 'A-offer'
$offerB  = Get-Artefact -Path (Join-Path $rB.Root 'discord-qa-offer.v1.json')          -Label 'B-offer'
$statusA = Get-Artefact -Path (Join-Path $rA.Root 'discord-qa-pairing-status.v1.json') -Label 'A-pairing-status'
$statusB = Get-Artefact -Path (Join-Path $rB.Root 'discord-qa-pairing-status.v1.json') -Label 'B-pairing-status'

# Pairing status legitimately predates the run (it is written once, at the
# bootstrap that paired them), so freshness is NOT required here -- existence
# and mutual consistency are. Read as raw JSON, and only IDs are ever compared;
# no friend code, safety number or name is written to the report.
function Read-JsonAnyAge { param([string]$Path) if (-not (Test-Path -LiteralPath $Path)) { return $null } try { return (Get-Content -LiteralPath $Path -Raw -ErrorAction Stop | ConvertFrom-Json -ErrorAction Stop) } catch { return $null } }
$oA = Read-JsonAnyAge (Join-Path $rA.Root 'discord-qa-offer.v1.json')
$oB = Read-JsonAnyAge (Join-Path $rB.Root 'discord-qa-offer.v1.json')
$sA = Read-JsonAnyAge (Join-Path $rA.Root 'discord-qa-pairing-status.v1.json')
$sB = Read-JsonAnyAge (Join-Path $rB.Root 'discord-qa-pairing-status.v1.json')

$pairOk = $false
$pairDetail = ''
if (-not $oA -or -not $oB) {
    $pairDetail = 'One or both instances have no readable discord-qa-offer.v1.json, so neither identity has published itself and there is nothing to pair.'
} elseif ($oA.osl_user_id -eq $oB.osl_user_id) {
    $pairDetail = 'BOTH INSTANCES PUBLISHED THE SAME osl_user_id. They are one identity, not two. Every cross-identity result from this rig would be a self-test. Do not proceed.'
} elseif (-not $sA -or -not $sB) {
    $pairDetail = ('Pairing is incomplete: A''s pairing status is {0} and B''s is {1}. The bootstrap writes discord-qa-pairing-status.v1.json only after it has consumed a peer offer, added the friend code and VERIFIED the safety number (apps/osl-hub/src/discord_qa_identity.rs:261-326).' -f
        $(if ($sA) { 'present' } else { 'ABSENT' }), $(if ($sB) { 'present' } else { 'ABSENT' }))
} elseif ($sA.peer_osl_user_id -ne $oB.osl_user_id -or $sB.peer_osl_user_id -ne $oA.osl_user_id) {
    $pairDetail = 'The pairing is not MUTUAL: at least one instance''s recorded peer is not the other instance. Each side has verified somebody, but not each other.'
} elseif (-not $sA.verified -or -not $sB.verified) {
    $pairDetail = ('At least one side''s pairing is not marked verified (A: {0}, B: {1}). An unverified pairing means the safety number was not proven stable, and the send path will refuse.' -f $sA.verified, $sB.verified)
} else {
    $pairOk = $true
    $pairDetail = 'Pairing is mutual and verified: each instance''s recorded peer_osl_user_id is the other instance''s published osl_user_id, and both carry verified=true. Only opaque IDs were compared; no friend code or safety number is recorded in this report.'
}
if (-not (Add-Gate 'mutual-pairing' 'A and B have mutually verified each other' $pairOk $pairDetail)) {
    Write-Blocked 'mutual-pairing' $pairDetail `
        'Exchange the offers: copy A''s discord-qa-offer.v1.json into B''s profile as discord-qa-peer-offer.v1.json and vice versa, then restart both instances so the bootstrap consumes them (scripts\qa\osl-p2p-pair.ps1 does exactly this and nothing else).'
}

# G5 both alive, and Discord present ---------------------------------------
if (-not (Assert-BothAlive -Stage 'gate' -A $A -B $B)) {
    Write-Blocked 'instance-vanished' $script:VanishDetail 'Restart the missing instance and re-run.'
}
$discordNow = Get-P2PDiscordPids
if (-not (Add-Gate 'discord-present' 'Discord is running' ($discordNow.Count -gt 0) `
    ('{0} Discord-family process(es): {1}. This harness never starts, stops, signals or closes any of them; the set is compared again at the end.' -f $discordNow.Count, ($discordNow -join ',')))) {
    Write-Blocked 'discord-present' 'No Discord process is running, so instance A has nothing to post into and no step can be measured.' `
        'Start Discord, open the QA conversation, confirm instance A has adopted it, and re-run.'
}

Say ''
Say 'Preconditions passed. Baselining append trails before anything is driven.' 'Cyan'

# H2 -- baseline EVERY append trail before a single byte is driven.
[void](Set-TrailBaseline -Path (Join-Path $TempRootA 'osl-discord-qa-send-stage.txt') -Label 'A-send-stage')
[void](Set-TrailBaseline -Path (Join-Path $TempRootA 'osl-startup-trace.txt')          -Label 'A-startup-trace')
[void](Set-TrailBaseline -Path (Join-Path $TempRootB 'osl-discord-qa-rehydrate.txt')   -Label 'B-rehydrate')
[void](Set-TrailBaseline -Path (Join-Path $TempRootB 'osl-discord-qa-send-stage.txt')  -Label 'B-send-stage')
[void](Set-TrailBaseline -Path (Join-Path $TempRootB 'osl-startup-trace.txt')          -Label 'B-startup-trace')

# B must not send during this run. Its outbound receipt is fingerprinted now so
# P6's "a row B did not send" is a proven claim rather than an assumption.
$bOutboundBefore = Get-Artefact -Path (Join-Path $rB.Root 'discord-qa-outbound-receipt.json') -Label 'B-outbound-before' -NoText

Say ''
Say '--- steps ---' 'Cyan'

# ===========================================================================
# Addressed self-test rendezvous: drive one verb on ONE named instance
# ===========================================================================
# The unqualified `osl-qa-selftest.request` is a GLOBAL rendezvous whose first
# consumer wins it, so with two instances running a trigger meant for B can be
# eaten by A. Each instance additionally polls a trigger addressed to its own
# bundle identifier (apps/osl-hub/src/main.rs, mod qa_selftest:
# ADDRESSED_TRIGGER_FORMAT / ADDRESSED_VERDICT_FORMAT) and answers it into an
# equally addressed verdict. That is the only safe way to drive B.
#
# Verbs: status | host | send | drain | rehydrate | reveal-view-once. Each one
# is the EXACT entry point the protected renderer already calls -- not a QA-only
# path into the broker, which is how this surface produced QA/production
# divergences before.
#
# This function synthesises no input of any kind. It writes a file and reads a
# file. No SendInput, no PostMessage, no window is touched.
function Get-InstanceFileToken {
    param([Parameter(Mandatory)][string]$Identifier)
    # Mirrors instance_file_token (apps/osl-hub/src/qa_selftest_request.rs):
    # every character outside [A-Za-z0-9.\-_] becomes '_'. The >96-byte digest
    # branch is not reproduced here because no bundle identifier we use is that
    # long; a longer one would silently disagree, so it is refused instead.
    if ($Identifier.Length -gt 96) {
        throw ("Bundle identifier '{0}' is longer than 96 chars; instance_file_token would append a digest that this harness does not reproduce." -f $Identifier)
    }
    ($Identifier.ToCharArray() | ForEach-Object {
        if ($_ -match '[A-Za-z0-9._-]') { $_ } else { '_' }
    }) -join ''
}

function Invoke-SelftestVerb {
    param(
        [Parameter(Mandatory)][string]$TempRoot,
        [Parameter(Mandatory)][string]$Identifier,
        [Parameter(Mandatory)][ValidateSet('status','host','send','drain','rehydrate','reveal-view-once')][string]$Verb,
        [Parameter(Mandatory)][string]$Label,
        # TARGET SELECTION IS BY PID AND PATH, NEVER BY NAME OR "first window".
        #
        # On 2026-07-26 another lane's helper selected "the first process with a
        # window" and drove SIX UI steps of THIS lane's instance B, silently
        # contaminating a running test. Selecting a target by anything a second
        # OSL build also satisfies -- window title, process name, "the one that
        # is running" -- is the defect, not the accident. Both facts are
        # mandatory here and a mismatch REFUSES rather than falls back.
        [Parameter(Mandatory)][int]$ExpectPid,
        [Parameter(Mandatory)][string]$ExpectExePath,
        [int]$ConsumeSec = 15,
        [int]$VerdictSec = 60
    )

    # --- target preflight, before a single byte is written ------------------
    if ($ExpectPid -le 0) {
        Add-Step ("{0}/target-pid" -f $Label) 'failed' 'No explicit process id was supplied. This harness refuses to drive "whichever OSL is running".'
        return [pscustomobject]@{ Verb = $Verb; Written = $false; Consumed = $false; Verdict = $null; Declined = $null; TargetOk = $false }
    }
    $proc = Get-Process -Id $ExpectPid -ErrorAction SilentlyContinue
    if (-not $proc) {
        Add-Step ("{0}/target-pid" -f $Label) 'failed' ('Process {0} is not running, so there is nothing this verb could legitimately reach.' -f $ExpectPid)
        return [pscustomobject]@{ Verb = $Verb; Written = $false; Consumed = $false; Verdict = $null; Declined = $null; TargetOk = $false }
    }
    $actualPath = $null
    try { $actualPath = $proc.Path } catch { $actualPath = $null }
    $wantPath = $null
    try { $wantPath = (Resolve-Path -LiteralPath $ExpectExePath -ErrorAction Stop).Path } catch { $wantPath = $ExpectExePath }
    if (-not $actualPath -or ($actualPath -ne $wantPath)) {
        Add-Step ("{0}/target-path" -f $Label) 'failed' `
            ('Process {0} runs "{1}", not this lane''s staged build "{2}". Refusing to drive a binary this harness did not stage -- that is how another lane''s instance gets driven by accident.' -f
                $ExpectPid, $(if ($actualPath) { $actualPath } else { '<unreadable>' }), $wantPath)
        return [pscustomobject]@{ Verb = $Verb; Written = $false; Consumed = $false; Verdict = $null; Declined = $null; TargetOk = $false }
    }
    # And the addressed identifier must belong to THAT pid, so the rendezvous
    # and the process anchor cannot disagree about who is being driven.
    $markerPids = @((Get-P2PBundleMap).GetEnumerator() | Where-Object { $_.Value -eq $Identifier } | ForEach-Object { [int]$_.Key })
    if ($markerPids -notcontains $ExpectPid) {
        Add-Step ("{0}/target-marker" -f $Label) 'failed' `
            ('The "{0}-sic" marker belongs to pid(s) {1}, not to the expected pid {2}. Refusing: the file rendezvous would reach a different process than the one anchored here.' -f
                $Identifier, (@($markerPids) -join ','), $ExpectPid)
        return [pscustomobject]@{ Verb = $Verb; Written = $false; Consumed = $false; Verdict = $null; Declined = $null; TargetOk = $false }
    }
    Add-Step ("{0}/target" -f $Label) 'ok' `
        ('Target proven three ways before writing anything: pid {0}, exe "{1}" (this lane''s staged build), and the "{2}-sic" marker belongs to that same pid.' -f $ExpectPid, $wantPath, $Identifier)

    $token   = Get-InstanceFileToken -Identifier $Identifier
    $req     = Join-Path $TempRoot ('osl-qa-selftest.{0}.request' -f $token)
    $ver     = Join-Path $TempRoot ('osl-qa-selftest.{0}.json' -f $token)
    $decl    = Join-Path $TempRoot ('osl-qa-selftest.{0}.declined.json' -f $token)
    $result  = [ordered]@{
        Verb = $Verb; RequestPath = $req; VerdictPath = $ver
        Written = $false; Consumed = $false; Verdict = $null; Declined = $null
    }

    # The verb is named in the body as JSON. A body that is not JSON is treated
    # by the app as the LEGACY SEND (qa_selftest_request.rs:286-290), so a
    # malformed or truncated body must never be written here: failing open into
    # the one verb with an irreversible side effect is the exact mistake that
    # boundary exists to prevent.
    $body = (@{ verb = $Verb; instance = $Identifier } | ConvertTo-Json -Compress)
    try {
        Set-Content -LiteralPath $req -Value $body -Encoding ascii -ErrorAction Stop
        $result.Written = $true
        Add-Step ('{0}/trigger-written' -f $Label) 'ok' ('Wrote verb "{0}" to the addressed trigger {1}.' -f $Verb, $req)
    } catch {
        Add-Step ('{0}/trigger-written' -f $Label) 'failed' $_.Exception.Message
        return [pscustomobject]$result
    }

    $dl = (Get-Date).AddSeconds($ConsumeSec)
    while ((Get-Date) -lt $dl -and (TimeLeftMs) -gt 20000) {
        if (-not (Test-Path -LiteralPath $req)) { $result.Consumed = $true; break }
        Start-Sleep -Milliseconds 250
    }
    Add-Step ('{0}/trigger-consumed' -f $Label) $(if ($result.Consumed) { 'ok' } else { 'failed' }) `
        $(if ($result.Consumed) { ('The addressed instance deleted its own request file, which proves the verb reached {0} and not the other instance.' -f $Identifier) }
          else { ('The addressed request was still present after {0}s. Either that instance is not a discord-qa-shell build, its watcher never started, or the bundle identifier is wrong.' -f $ConsumeSec) })
    if (-not $result.Consumed) { return [pscustomobject]$result }

    $dl = (Get-Date).AddSeconds($VerdictSec)
    while ((Get-Date) -lt $dl -and (TimeLeftMs) -gt 20000) {
        $v = Get-Artefact -Path $ver -Label ('{0}-verdict' -f $Label)
        if ($v.Exists -and $v.FromThisRun -and $v.Json) { $result.Verdict = $v; break }
        $d = Get-Artefact -Path $decl -Label ('{0}-declined' -f $Label)
        if ($d.Exists -and $d.FromThisRun -and $d.Json) { $result.Declined = $d; break }
        Start-Sleep -Milliseconds 500
    }
    if (-not $result.Verdict) { $result.Verdict = Get-Artefact -Path $ver -Label ('{0}-verdict' -f $Label) }
    [pscustomobject]$result
}

# ===========================================================================
# P1  A sends an encrypted message to the peer
# ===========================================================================
$reqA = Join-Path $TempRootA 'osl-qa-selftest.request'
$verA = Join-Path $TempRootA 'osl-qa-selftest.json'

# The watcher deletes a stale verdict when it picks a trigger up (main.rs:5564),
# but a verdict that was never replaced would still be sitting there, so the
# freshness test below is what actually protects this step.
$verBefore = Get-Artefact -Path $verA -Label 'A-selftest-verdict-before' -NoText

$triggerWritten = $false
try {
    # Fixed, contentless trigger. Nothing is typed and no text is chosen here:
    # the plaintext is a fixed &'static str inside the app (main.rs:4876).
    Set-Content -LiteralPath $reqA -Value 'osl-p2p-loop' -Encoding ascii -ErrorAction Stop
    $triggerWritten = $true
    Add-Step 'p1/trigger-written' 'ok' ('Wrote {0}. The watcher polls every 500ms (apps/osl-hub/src/main.rs:4889).' -f $reqA)
} catch {
    Add-Step 'p1/trigger-written' 'failed' $_.Exception.Message
}

$consumed = $false
if ($triggerWritten) {
    $dl = (Get-Date).AddSeconds($SendTriggerWaitSec)
    while ((Get-Date) -lt $dl) {
        if (-not (Test-Path -LiteralPath $reqA)) { $consumed = $true; break }
        Start-Sleep -Milliseconds 250
    }
    Add-Step 'p1/trigger-consumed' $(if ($consumed) { 'ok' } else { 'failed' }) `
        $(if ($consumed) { 'Instance A deleted the request file, which is positive proof its self-test watcher is live and that the trigger was not eaten by the other instance.' }
          else { ('The request file was still there after {0}s. Either A is not a discord-qa-shell build, or its watcher never started (look for setup_step_46_qa_selftest_watcher_spawned in A''s startup trace).' -f $SendTriggerWaitSec) })
}

$p1Verdict = $null
if ($consumed) {
    $dl = (Get-Date).AddSeconds($SendVerdictWaitSec)
    while ((Get-Date) -lt $dl -and (TimeLeftMs) -gt 30000) {
        $v = Get-Artefact -Path $verA -Label 'A-selftest-verdict'
        if ($v.Exists -and $v.FromThisRun -and $v.Json) { $p1Verdict = $v; break }
        Start-Sleep -Milliseconds 500
    }
    if (-not $p1Verdict) { $p1Verdict = Get-Artefact -Path $verA -Label 'A-selftest-verdict' }
}

$sendStage   = Get-TrailSince -Label 'A-send-stage'
$sendReceipt = Get-Artefact -Path (Join-Path $rA.Root 'discord-qa-send-stage-receipt.json') -Label 'A-send-stage-receipt'
$aOutbound   = Get-Artefact -Path (Join-Path $rA.Root 'discord-qa-outbound-receipt.json')   -Label 'A-outbound-receipt'

if (-not $triggerWritten) {
    [void](New-Criterion -Id 'P1' -Name 'A sends an encrypted message to the peer' -Status 'unmeasurable' -Measured $null `
        -Detail ('The self-test trigger could not be written to {0}, so no send was ever driven. Nothing about the send path was measured.' -f $reqA))
} elseif (-not $consumed) {
    [void](New-Criterion -Id 'P1' -Name 'A sends an encrypted message to the peer' -Status 'unmeasurable' -Measured $false `
        -Detail ('Instance A never consumed the trigger within {0}s, so the send was never driven and the send path was not exercised. This is not evidence that sending is broken.' -f $SendTriggerWaitSec) `
        -Extra @{ requestPath = $reqA })
} elseif (-not ($p1Verdict.Exists -and $p1Verdict.FromThisRun)) {
    [void](New-Criterion -Id 'P1' -Name 'A sends an encrypted message to the peer' -Status 'unmeasurable' -Measured $null `
        -Detail ('The trigger WAS consumed, so a run started, but no verdict written during this run appeared at {0} within {1}s. {2}' -f $verA, $SendVerdictWaitSec, $p1Verdict.Detail) `
        -Extra @{ verdictArtefact = [ordered]@{ exists = $p1Verdict.Exists; written = $p1Verdict.WrittenIso; ageSec = $p1Verdict.AgeSec; fromThisRun = $p1Verdict.FromThisRun } })
} else {
    $j = $p1Verdict.Json
    $outcome = $null
    foreach ($f in @('outcome', 'verdict', 'result', 'status')) { if ($null -ne $j.$f) { $outcome = [string]$j.$f; break } }
    $green = ($outcome -match '^(pass|ok|sent|delivered|success)')
    $st = 'fail'
    if ($null -eq $outcome) { $st = 'unmeasurable' } elseif ($green) { $st = 'pass' }
    if ($outcome -match '^(busy|stalled|not-ready)$') { $st = 'unmeasurable' }
    [void](New-Criterion -Id 'P1' -Name 'A sends an encrypted message to the peer' -Status $st -Measured $outcome `
        -Detail ('A''s self-test verdict for THIS run reports outcome "{0}". {1} {2} outcomes "busy", "stalled" and "not-ready" are the app saying it could not run, which is unmeasurable, not a product failure. The send-stage trail grew by {3} line(s) during this run and the send-stage receipt {4}.' -f
            $outcome, $p1Verdict.Detail,
            $(if ($st -eq 'unmeasurable') { 'The' } else { 'For reference, the' }),
            $sendStage.Lines.Count,
            $(if ($sendReceipt.Exists -and $sendReceipt.FromThisRun) { 'was written during this run' } else { 'was NOT written during this run (' + $sendReceipt.Detail + ')' })) `
        -Extra @{
            sendStageLinesThisRun = $sendStage.Lines.Count
            sendStageTrailNote = $sendStage.Detail
            sendStageReceiptFresh = ($sendReceipt.Exists -and $sendReceipt.FromThisRun)
            outboundReceiptFresh = ($aOutbound.Exists -and $aOutbound.FromThisRun)
        })
}

[void](Assert-BothAlive -Stage 'after-P1' -A $A -B $B)

# ===========================================================================
# P2  B drains, decrypts and renders it
# ===========================================================================
# THE DRAIN CAN NOW BE DRIVEN. This step used to be observe-only, because the
# self-test module drove exactly one verb (the probe send). It now exposes a
# `drain` verb that reaches `open_native_discord_overlay_text` -- the same entry
# point the protected renderer calls -- through the instance-addressed
# rendezvous. So B's non-drain is now a real failure rather than `unmeasurable`.
#
# `-OperatorDrivesReceiveSide` is kept as an override for the cases the verb
# still cannot reach (anything needing a real pointer gesture), and because an
# operator watching is sometimes the point of the run.
$obsSec = $(if ($OperatorDrivesReceiveSide) { $OperatorObserveSec } else { $ObserveSec })
$bDrain = $null
if ($OperatorDrivesReceiveSide) {
    Say ''
    Say ('OPERATOR ACTION REQUIRED on instance B (pid {0}) within {1}s:' -f $B.Pid, $obsSec) 'Cyan'
    Say '  1. engage the lock on B so the protected composer opens' 'Cyan'
    Say '  2. open the eye on B (decrypt display) so the transcript is decoded' 'Cyan'
    Say '  3. let B drain -- do not click anything in instance A' 'Cyan'
    Say '  This harness synthesises no input; it is only watching files.' 'DarkGray'
} else {
    # Addressed to B by bundle identifier AND anchored to B's pid and staged
    # exe path, so neither instance A nor another lane's build can be reached.
    $bDrain = Invoke-SelftestVerb -TempRoot $TempRootB -Identifier $BundleB -Verb 'drain' `
        -Label 'p2/drain' -VerdictSec $obsSec `
        -ExpectPid ([int]$B.Pid) -ExpectExePath $ExeB
}

$bPoll = $null
$bInbound = $null
$dl = (Get-Date).AddSeconds($obsSec)
while ((Get-Date) -lt $dl -and (TimeLeftMs) -gt 20000) {
    $p = Get-Artefact -Path (Join-Path $rB.Root 'discord-qa-inbound-poll-receipt.json') -Label 'B-inbound-poll-receipt'
    if ($p.Exists -and $p.FromThisRun -and $p.Json) { $bPoll = $p; break }
    Start-Sleep -Milliseconds 750
}
if (-not $bPoll) { $bPoll = Get-Artefact -Path (Join-Path $rB.Root 'discord-qa-inbound-poll-receipt.json') -Label 'B-inbound-poll-receipt' }
$bInbound = Get-Artefact -Path (Join-Path $rB.Root 'discord-qa-inbound-receipt.json') -Label 'B-inbound-receipt'

$p2Gap = 'The drain verb now exists (apps/osl-hub/src/main.rs, mod qa_selftest) and this harness drives it through the instance-addressed rendezvous, so a missing receipt is no longer explained by "the harness cannot drive a drain". If this step is unmeasurable, the remaining gap is the addressing itself: confirm the -BundleB identifier matches the bundle that actually started (osl-launch-instance-b.ps1 reads it back) and that B is a discord-qa-shell build.'

if ($bPoll.Exists -and $bPoll.FromThisRun -and $bPoll.Json) {
    $j = $bPoll.Json
    $opened  = [int](0 + $j.openedCount)
    $fetched = $j.fetched
    $outcome = [string]$j.outcome
    $st = 'fail'
    if ($opened -ge 1) { $st = 'pass' }
    [void](New-Criterion -Id 'P2' -Name 'B drains, decrypts and renders the message' -Status $st -Measured $opened `
        -Detail ('B''s inbound poll receipt written during this run reports outcome "{0}", fetched={1}, openedCount={2}. openedCount is incremented only after an authenticated open AND a durable replay consumption (apps/osl-hub/src/broker.rs:2905), so a non-zero value is a real cross-identity decrypt, not a delivery notification. No plaintext is read from this file; only counts.' -f
            $outcome, $fetched, $opened) `
        -Extra @{ outcome = $outcome; fetched = $fetched; openedCount = $opened
                  pendingViewOnceCount = [int](0 + $j.pendingViewOnceCount)
                  acknowledgmentCount = [int](0 + $j.acknowledgmentCount)
                  inboundReceiptFresh = ($bInbound.Exists -and $bInbound.FromThisRun) })
} elseif ($null -ne $bDrain -and $bDrain.Consumed) {
    # The drain WAS driven on B and B answered the trigger. A missing or empty
    # inbound poll receipt is now a real negative result about the receive path,
    # not a gap in the rig -- which is the whole point of the verb existing.
    [void](New-Criterion -Id 'P2' -Name 'B drains, decrypts and renders the message' -Status 'fail' -Measured 0 `
        -Detail ('B consumed the addressed "drain" trigger, so open_native_discord_overlay_text was invoked through the same entry point the renderer uses, but no inbound poll receipt written during this run appeared in B''s profile within {0}s. {1} Because the drain was genuinely driven, this is evidence about the receive path rather than an untested step.' -f
            $obsSec, $bPoll.Detail) `
        -Extra @{ drainVerdictPath = $bDrain.VerdictPath
                  drainVerdictFresh = [bool]($bDrain.Verdict -and $bDrain.Verdict.Exists -and $bDrain.Verdict.FromThisRun)
                  drainDeclined = [bool]$bDrain.Declined })
} else {
    [void](New-Criterion -Id 'P2' -Name 'B drains, decrypts and renders the message' -Status 'unmeasurable' -Measured $null `
        -Detail ('No inbound poll receipt written during this run appeared in B''s profile within {0}s, and the drain was not confirmed driven ({1}). {2}' -f
            $obsSec,
            $(if ($OperatorDrivesReceiveSide) { 'the operator was driving the receive side by hand' } else { 'B never consumed the addressed drain trigger' }),
            $bPoll.Detail) `
        -ProductChange $p2Gap)
}

[void](Assert-BothAlive -Stage 'after-P2' -A $A -B $B)

# ===========================================================================
# P3  Receipts flow back to A (Received, then Opened)
# ===========================================================================
# B posts an acknowledgement over the control inbox (broker.rs:3686/3733).
# A learns of it only by draining, and A's own ledger is AEAD-encrypted at rest
# (broker.rs:3869), so its CONTENT cannot be read by any harness.
$aPoll = Get-Artefact -Path (Join-Path $rA.Root 'discord-qa-inbound-poll-receipt.json') -Label 'A-inbound-poll-receipt'
$p3Gap = 'apps/osl-hub/src/broker.rs:3082 puts the acknowledgements in the drain batch, but discord_qa_inbound_receipt.rs records only acknowledgmentCount -- a single number that cannot distinguish Received (broker.rs:2834) from Opened (broker.rs:2905), nor their order. The sender-side ledger hub_native_overlay_receipts.json is encrypted at rest (broker.rs:3869) so its statuses are unreadable from outside. Record the per-status counts (or the status transitions) in the QA poll receipt to make this measurable.'

if ($aPoll.Exists -and $aPoll.FromThisRun -and $aPoll.Json) {
    $acks = [int](0 + $aPoll.Json.acknowledgmentCount)
    $st = $(if ($acks -ge 1) { 'pass' } else { 'fail' })
    [void](New-Criterion -Id 'P3a' -Name 'An acknowledgement flowed back from B to A' -Status $st -Measured $acks `
        -Detail ('A''s inbound poll receipt written during this run reports acknowledgmentCount={0}. A non-zero value means B sealed a MSG_TYPE_NATIVE_OVERLAY_ACK and posted it to the control inbox (broker.rs:3686,3733) and A drained it.' -f $acks) `
        -Extra @{ acknowledgmentCount = $acks })
} else {
    [void](New-Criterion -Id 'P3a' -Name 'An acknowledgement flowed back from B to A' -Status 'unmeasurable' -Measured $null `
        -Detail ('No inbound poll receipt written during this run appeared in A''s profile. {0} A only learns of an acknowledgement by draining, and the drain cannot be driven from outside the process.' -f $aPoll.Detail) `
        -ProductChange $p2Gap)
}
[void](New-Criterion -Id 'P3b' -Name 'The receipts were Received first, then Opened' -Status 'blocked' -Measured $null `
    -Detail 'The ORDER and the KIND of the two receipts cannot be observed from outside the process against this build. Received is emitted only for an unrevealed view-once row (broker.rs:2834, :2977) and Opened after an authenticated open (broker.rs:2905), but the only artefact that crosses the process boundary is a single acknowledgmentCount, and the sender-side status ledger is AEAD-encrypted at rest. That makes this "blocked" rather than "unmeasurable": no run of this harness against this build can measure it.' `
    -ProductChange $p3Gap)

[void](Assert-BothAlive -Stage 'after-P3' -A $A -B $B)

# ===========================================================================
# P4  A view-once message reveals exactly once on B, and is refused twice
# ===========================================================================
# The self-test drives one fixed, NON-view-once probe send (main.rs:4876,5443),
# so a view-once message can only enter this rig by hand.
$p4Gap = 'Two changes. (1) The self-test rendezvous can only send the one fixed probe (apps/osl-hub/src/main.rs:4876, drive_probe_send at :5443) -- it needs a view-once variant so a harness can stage phase 1 at all. (2) The second-attempt refusal is returned only as an Err string to the renderer (apps/osl-hub/src/broker.rs:2536-2542); it leaves no artefact, so "it was refused the second time" is unobservable. Record the refusal in discord-qa-inbound-receipt.json.'
$vo1 = $null
if ($bPoll.Exists -and $bPoll.FromThisRun -and $bPoll.Json) { $vo1 = [int](0 + $bPoll.Json.pendingViewOnceCount) }
$vo2 = $null
if ($bInbound.Exists -and $bInbound.FromThisRun -and $bInbound.Json) {
    $vo2 = @($bInbound.Json | ForEach-Object { $_ } | Where-Object { $_.viewOnceConsumed -eq $true }).Count
}
if ($null -eq $vo1) {
    [void](New-Criterion -Id 'P4a' -Name 'A view-once message is listed but not opened (phase 1)' -Status 'unmeasurable' -Measured $null `
        -Detail 'B produced no gradeable inbound poll receipt this run, so pendingViewOnceCount could not be read. Note that even with one, this run staged no view-once message: the only send this harness can drive is the fixed non-view-once probe.' `
        -ProductChange $p4Gap)
} elseif ($vo1 -ge 1) {
    [void](New-Criterion -Id 'P4a' -Name 'A view-once message is listed but not opened (phase 1)' -Status 'pass' -Measured $vo1 `
        -Detail ('B reported pendingViewOnceCount={0} this run: the row was listed without being opened, which is the two-phase gate at broker.rs:2822 doing its job.' -f $vo1))
} else {
    [void](New-Criterion -Id 'P4a' -Name 'A view-once message is listed but not opened (phase 1)' -Status 'unmeasurable' -Measured 0 `
        -Detail 'B reported pendingViewOnceCount=0. No view-once message was in flight, because this harness cannot send one -- the self-test drives one fixed non-view-once plaintext. Zero here is the absence of a stimulus, not a product failure.' `
        -ProductChange $p4Gap)
}
if ($null -ne $vo2 -and $vo2 -ge 1) {
    [void](New-Criterion -Id 'P4b' -Name 'The view-once message reveals exactly once (phase 2)' -Status 'pass' -Measured $vo2 `
        -Detail ('B''s inbound receipt for this run marks {0} message(s) viewOnceConsumed=true, so the reveal completed and the replay ledger was consumed.' -f $vo2))
} else {
    [void](New-Criterion -Id 'P4b' -Name 'The view-once message reveals exactly once (phase 2)' -Status 'unmeasurable' -Measured $vo2 `
        -Detail 'No message in B''s inbound receipt for this run is marked viewOnceConsumed. Nothing was revealed, because nothing view-once was staged.' `
        -ProductChange $p4Gap)
}
[void](New-Criterion -Id 'P4c' -Name 'A second reveal attempt is refused' -Status 'blocked' -Measured $null `
    -Detail 'The refusal exists and is correct by construction -- the second call finds already_consumed at broker.rs:2872, takes the continue at :2884, and the empty batch trips the Err at broker.rs:2536-2542 -- but it is returned only to the renderer as a string. It writes no file, increments no counter and touches no server. No run of this harness can observe it, which makes this "blocked" rather than "unmeasurable".' `
    -ProductChange $p4Gap)

[void](Assert-BothAlive -Stage 'after-P4' -A $A -B $B)

# ===========================================================================
# P5  A burn issued by A is applied on B, and B acknowledges
# ===========================================================================
# This one is settled without needing a stimulus: the lane is INERT. It is
# checked dynamically anyway, so that the day it IS wired the harness notices
# rather than continuing to report a stale conclusion.
$bRevLedger  = Get-Artefact -Path (Join-Path $rB.Root 'hub_revocation_ledger.json')  -Label 'B-revocation-ledger'  -NoText
$aRevOutbox  = Get-Artefact -Path (Join-Path $rA.Root 'hub_revocation_outbox.json')  -Label 'A-revocation-outbox'  -NoText
$p5Gap = 'apps/osl-hub/src/security.rs:2124 apply_peer_revocation, :2421 record_revocation_ack, :2366 due_revocations and :2396 record_revocation_attempt have NO callers outside security.rs. The drain at apps/osl-hub/src/broker.rs:2603-2690 tests only is_native_overlay_ack_bundle (:2620) and is_native_overlay_relay_bundle (:2676) -- it never calls is_revocation_bundle, so an inbound 0x0A row is silently skipped by the continue. Nothing POSTs the revocation lane, and keyserver-cf/migrations/0027_control_inbox_revocation_lane.sql:4 says NOT DEPLOYED. This is Wave B1 (wire burn 0x0A/0x0B); until it lands, no rig of any kind can measure P5.'
if ($bRevLedger.Exists -and $bRevLedger.FromThisRun) {
    [void](New-Criterion -Id 'P5' -Name 'A burn issued by A is applied on B, and B acknowledges' -Status 'unmeasurable' -Measured $true `
        -Detail ('B''s hub_revocation_ledger.json WAS written during this run, which contradicts the static reading that the apply-on-peer lane is inert. The file is encrypted at rest so its contents cannot be graded, but its appearance means the lane may now be wired and this step must be re-derived from source before it is trusted either way.' ) `
        -ProductChange $p5Gap)
} else {
    [void](New-Criterion -Id 'P5' -Name 'A burn issued by A is applied on B, and B acknowledges' -Status 'blocked' -Measured $false `
        -Detail ('The bilateral burn lane is INERT in this build, so there is nothing to measure. The wire types exist (MSG_TYPE_REVOCATION 0x0A at crates/ipc/src/wire_v2.rs:204, the ack 0x0B at :214) and the apply/ack state machine is complete, but nothing calls it: apply_peer_revocation (security.rs:2124) and record_revocation_ack (security.rs:2421) have zero callers outside their own file, and the drain never tests is_revocation_bundle, so an inbound 0x0A is skipped. Burn commands only QUEUE (security.rs:1719,1855) and nothing ever posts the queue. Observed here: B''s revocation ledger {0}; A''s revocation outbox {1}.' -f
            $(if ($bRevLedger.Exists) { 'exists but predates this run' } else { 'does not exist' }),
            $(if ($aRevOutbox.Exists) { 'exists' } else { 'does not exist' })) `
        -ProductChange $p5Gap)
}

[void](Assert-BothAlive -Stage 'after-P5' -A $A -B $B)

# ===========================================================================
# P6  The eye on B paints a row B DID NOT SEND
# ===========================================================================
# This is the step two identities exist to settle, and it is the best
# instrumented of the six. The decoder proves BOTH orientations
# (broker.rs:1788-1791: PeerToSelf and SelfToPeer), so on a single instance a
# decoded row could always have been one's own. On B, which has sent nothing,
# every decoded row is BY CONSTRUCTION a row B did not send -- provided "B sent
# nothing" is proven rather than assumed, which is what the outbound-receipt
# fingerprint below does.
$bOutboundAfter = Get-Artefact -Path (Join-Path $rB.Root 'discord-qa-outbound-receipt.json') -Label 'B-outbound-after' -NoText
$bSentSomething = $false
$bSentDetail = ''
if ($bOutboundAfter.Exists -and $bOutboundAfter.FromThisRun) {
    $bSentSomething = $true
    $bSentDetail = 'B''s outbound receipt WAS written during this run, so B sent something and "a row B did not send" cannot be concluded from a decode count alone.'
} elseif ($bOutboundBefore.Exists -and $bOutboundAfter.Exists -and $bOutboundBefore.Sha256 -ne $bOutboundAfter.Sha256) {
    $bSentSomething = $true
    $bSentDetail = 'B''s outbound receipt changed byte-for-byte across this run (compared by sha256, not by mtime), so B sent something.'
} else {
    $bSentDetail = 'B''s outbound receipt is byte-identical across this run (sha256 compared, not mtime), so B sent nothing while it was observed.'
}
$bSendStage = Get-TrailSince -Label 'B-send-stage'
if ($bSendStage.Grew) {
    $bSentSomething = $true
    $bSentDetail += (' B''s send-stage trail also grew by {0} line(s) during this run.' -f $bSendStage.Lines.Count)
}

$rehy = Get-TrailSince -Label 'B-rehydrate'
# The trail's format is a fixed label plus a count: "{stage} count={n}"
# (apps/osl-hub/src/main.rs:2845). No content is ever in it, and none is read.
$decoded = $null
$undecodable = $null
$reasons = @()
foreach ($ln in @($rehy.Lines)) {
    if ($ln -match '^(?<stage>[a-z_]+)\s+count=(?<n>\d+)') {
        $stage = $Matches['stage']; $n = [int]$Matches['n']
        switch -Regex ($stage) {
            '^rehydrate_rows_decoded$'      { if ($null -eq $decoded -or $n -gt $decoded) { $decoded = $n } }
            '^rehydrate_rows_undecodable$'  { if ($null -eq $undecodable -or $n -gt $undecodable) { $undecodable = $n } }
            default { if ($n -gt 0) { $reasons += ('{0}={1}' -f $stage, $n) } }
        }
    }
}
$reasons = @($reasons | Select-Object -Unique)

if (-not $rehy.Grew) {
    [void](New-Criterion -Id 'P6' -Name 'The eye on B paints a row B did not send' -Status 'unmeasurable' -Measured $null `
        -Detail ('Nothing was appended to B''s rehydrate trail during this run, so the eye never ran on B. {0} rehydrate_native_discord_overlay_history (apps/osl-hub/src/main.rs:2956) is callable only from the overlay window (main.rs:2963) and is gated on the eye being open (apps/osl-hub-ui/src/overlay.ts:604), and this harness cannot open it -- no pointer input, no keyboard injection, and no file trigger for the eye. Re-run with -OperatorDrivesReceiveSide and open the eye on B by hand.' -f $rehy.Detail) `
        -ProductChange 'Add an eye/rehydrate verb to the qa_selftest file rendezvous (apps/osl-hub/src/main.rs:4869-4910) so the decode can be driven without the GUI.' `
        -Extra @{ trailNote = $rehy.Detail })
} elseif ($bSentSomething) {
    [void](New-Criterion -Id 'P6' -Name 'The eye on B paints a row B did not send' -Status 'unmeasurable' -Measured $decoded `
        -Detail ('The eye ran on B and decoded {0} row(s), but B ALSO SENT during this run, so a decoded row could be B''s own. The decoder deliberately proves both orientations -- PeerToSelf and SelfToPeer at broker.rs:1788-1791 -- and the renderer labels every decoded row as incoming regardless (apps/osl-hub-ui/src/overlay.ts:688-689), so orientation cannot be recovered from outside. {1} Re-run without letting B send.' -f $decoded, $bSentDetail) `
        -ProductChange 'apps/osl-hub-ui/src/overlay.ts:688-689 stamps direction:"incoming" and author:verifiedFriendIdentity on EVERY decoded row, discarding the orientation that broker.rs:1788-1791 already proved. Carry the proven orientation through to the row binding so own-sent and peer-sent rows are distinguishable.' `
        -Extra @{ rowsDecoded = $decoded; bSentDuringRun = $true })
} elseif ($null -ne $decoded -and $decoded -ge 1) {
    [void](New-Criterion -Id 'P6' -Name 'The eye on B paints a row B did not send' -Status 'pass' -Measured $decoded `
        -Detail ('B''s eye decoded {0} row(s) during this run, and B sent nothing while it was observed. {1} Every decoded row is therefore, by construction, a row B did not send -- which is the fact a single identity can never establish, because the decoder proves both orientations (broker.rs:1788-1791) and a lone instance cannot rule out its own. Counts only were read; no row text, no plaintext and no conversation name.' -f $decoded, $bSentDetail) `
        -Extra @{ rowsDecoded = $decoded; rowsUndecodable = $undecodable; bSentDuringRun = $false })
} else {
    [void](New-Criterion -Id 'P6' -Name 'The eye on B paints a row B did not send' -Status 'fail' -Measured 0 `
        -Detail ('B''s eye RAN during this run ({0} trail line(s)) but decoded zero rows. This is a measured violation, not an absence of stimulus: the decode was attempted and produced nothing. The trail''s own refusal counters say why -- {1}. Each of those is a distinct bug: pointer_absent means no stego pointer was found in the row text, pointer_blob_gone means the pointer authenticated but the cipher-store blob is gone, store_unreachable means the network leg failed, display_off means the eye was closed, budget_exhausted means the probe budget ran out before the container was reached.' -f
            $rehy.Lines.Count, $(if ($reasons.Count) { ($reasons -join ', ') } else { 'no non-zero counter was emitted, which is itself a diagnostic gap' })) `
        -Extra @{ rowsDecoded = 0; rowsUndecodable = $undecodable; refusalCounters = $reasons })
}

[void](Assert-BothAlive -Stage 'end' -A $A -B $B)
Write-Final
