<#
.SYNOPSIS
Runs the two-copy active OSL long-run journey and records process resources.

.DESCRIPTION
Each copy repeats six already-proved product journeys: OSL Chats send and read,
friend verification, attachment transfer, Settings, and Scrub controls.  The
controller takes sample zero before any action and one sample after each
completed minute.  The resulting JSONL therefore has exactly Minutes + 1
samples per copy.  Resource fields are read from the operating system's
Process object (PrivateMemorySize64, HandleCount, and Threads.Count), rather
than inferred from command output.

The runner deliberately owns the child process lifetime.  It never leaves a
copy or an action command running after either a success or a failure.

.EXAMPLE
pwsh -NoProfile -NonInteractive -File scripts/qa/osl-active-long-run.ps1 -Minutes 20
#>

[CmdletBinding()]
param(
    [ValidateRange(1, 1440)]
    [int]$Minutes = 20,

    [ValidateRange(1, 3600)]
    [int]$SampleIntervalSeconds = 60,

    [ValidateRange(1, 300)]
    [int]$ActionTimeoutSeconds = 55,

    [ValidateSet('Live', 'Fixture')]
    [string]$ActionPlanMode = 'Live',

    [string]$OutputDirectory,

    [switch]$Worker,

    [ValidateSet('A', 'B')]
    [string]$Copy,

    [string]$StatePath,

    [string]$ActionLogPath,

    [string]$RepositoryRoot
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

function Fail([string]$Message) {
    throw "TASK3676: $Message"
}

function Write-JsonLine([string]$Path, [object]$Value) {
    $line = $Value | ConvertTo-Json -Compress -Depth 8
    [System.IO.File]::AppendAllText($Path, "$line`n", [System.Text.UTF8Encoding]::new($false))
}

function Write-JsonAtomic([string]$Path, [object]$Value) {
    $temporary = "$Path.$PID.tmp"
    $json = $Value | ConvertTo-Json -Compress -Depth 8
    [System.IO.File]::WriteAllText($temporary, $json, [System.Text.UTF8Encoding]::new($false))
    Move-Item -LiteralPath $temporary -Destination $Path -Force
}

function Read-JsonFile([string]$Path) {
    Get-Content -LiteralPath $Path -Raw -Encoding utf8 | ConvertFrom-Json
}

function Invoke-ActionCommand {
    param(
        [Parameter(Mandatory)][string]$Name,
        [Parameter(Mandatory)][string[]]$Command,
        [Parameter(Mandatory)][string]$WorkingDirectory,
        [Parameter(Mandatory)][int]$TimeoutSeconds,
        [Parameter(Mandatory)][string]$LogPath,
        [Parameter(Mandatory)][string]$CopyName,
        [Parameter(Mandatory)][int]$Sequence
    )

    $stdout = "$LogPath.$Sequence.stdout"
    $stderr = "$LogPath.$Sequence.stderr"
    $started = [DateTime]::UtcNow
    $environment = @{}
    if ($Name -eq 'friend') { $environment['OSL_ADD_FRIEND_PROOF'] = '1' }
    $process = Start-Process -FilePath $Command[0] -ArgumentList $Command[1..($Command.Count - 1)] `
        -WorkingDirectory $WorkingDirectory -Environment $environment -RedirectStandardOutput $stdout -RedirectStandardError $stderr -PassThru
    $finished = $process.WaitForExit($TimeoutSeconds * 1000)
    if (-not $finished) {
        Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue
        Write-JsonLine $LogPath ([ordered]@{
            copy = $CopyName; sequence = $Sequence; action = $Name; ok = $false; process_id = $process.Id
            error = "action exceeded ${TimeoutSeconds}s"; started_at_utc = $started.ToString('o'); finished_at_utc = [DateTime]::UtcNow.ToString('o')
        })
        Fail "copy $CopyName action $Name exceeded ${TimeoutSeconds}s"
    }
    if ($process.ExitCode -ne 0) {
        Write-JsonLine $LogPath ([ordered]@{
            copy = $CopyName; sequence = $Sequence; action = $Name; ok = $false; process_id = $process.Id
            exit_code = $process.ExitCode; stdout = $stdout; stderr = $stderr
            started_at_utc = $started.ToString('o'); finished_at_utc = [DateTime]::UtcNow.ToString('o')
        })
        Fail "copy $CopyName action $Name exited $($process.ExitCode); see $stderr"
    }
    Write-JsonLine $LogPath ([ordered]@{
        copy = $CopyName; sequence = $Sequence; action = $Name; ok = $true; process_id = $process.Id
        exit_code = 0; stdout = $stdout; stderr = $stderr
        started_at_utc = $started.ToString('o'); finished_at_utc = [DateTime]::UtcNow.ToString('o')
    })
}

function Get-ActionPlan([string]$Root, [string]$Mode) {
    if ($Mode -eq 'Fixture') {
        return @(
            'osl-chats-send', 'osl-chats-read', 'friend', 'file', 'settings', 'scrub' | ForEach-Object {
                [pscustomobject]@{ name = $_; cwd = $Root; command = @('pwsh', '-NoProfile', '-NonInteractive', '-Command', 'exit 0') }
            }
        )
    }
    $manifest = Join-Path $Root 'apps/osl-hub/Cargo.toml'
    $uiRoot = Join-Path $Root 'apps/osl-hub-ui'
    @(
        [pscustomobject]@{ name = 'osl-chats-send'; cwd = $Root; command = @('cargo', 'test', '--manifest-path', $manifest, '--no-default-features', '--features', 'core', '--test', 'sealed_relay_e2e', 'task_1369_live_two_way_direct_messages_read_exactly_once', '--', '--test-threads=1') },
        [pscustomobject]@{ name = 'osl-chats-read'; cwd = $Root; command = @('cargo', 'test', '--manifest-path', $manifest, '--no-default-features', '--features', 'core', '--test', 'task_3671_osl_chat_history_search', '--', '--test-threads=1') },
        [pscustomobject]@{ name = 'friend'; cwd = $Root; command = @('cargo', 'test', '--manifest-path', $manifest, '--no-default-features', '--features', 'core', '--test', 'add_friend_handshake_e2e', 'two_fresh_installs_reach_encrypted_messaging_only_after_a_symmetric_handshake', '--', '--ignored', '--test-threads=1') },
        [pscustomobject]@{ name = 'file'; cwd = $Root; command = @('cargo', 'test', '--manifest-path', $manifest, '--no-default-features', '--features', 'core', '--test', 'osl_chat_attachment_e2e', 'direct_upload_round_trip_recovers_byte_identical_plaintext_and_leaves_no_plaintext_at_rest', '--', '--test-threads=1') },
        [pscustomobject]@{ name = 'settings'; cwd = $uiRoot; command = @('npm', 'exec', 'vitest', 'run', 'src/ia-settings-placement.test.ts', '--maxWorkers=1', '--minWorkers=1') },
        [pscustomobject]@{ name = 'scrub'; cwd = $uiRoot; command = @('npm', 'exec', 'vitest', 'run', 'src/live-scan-controls.test.ts', 'src/scrub-attended-imap-run.test.ts', '--maxWorkers=1', '--minWorkers=1') }
    )
}

function Invoke-Worker {
    if (-not $Copy -or -not $StatePath -or -not $ActionLogPath -or -not $RepositoryRoot) {
        Fail 'worker needs Copy, StatePath, ActionLogPath, and RepositoryRoot'
    }
    $plan = Get-ActionPlan $RepositoryRoot $ActionPlanMode
    if ($plan.Count -ne 6) { Fail "worker action plan must have exactly 6 journeys, got $($plan.Count)" }
    Write-JsonAtomic $StatePath ([ordered]@{ ready = $true; copy = $Copy; sequence = 0; completed_action_count = 0; action = $null })
    $started = [DateTime]::UtcNow
    for ($sequence = 1; $sequence -le $Minutes; $sequence++) {
        $action = $plan[($sequence - 1) % $plan.Count]
        Invoke-ActionCommand -Name $action.name -Command $action.command -WorkingDirectory $action.cwd `
            -TimeoutSeconds $ActionTimeoutSeconds -LogPath $ActionLogPath -CopyName $Copy -Sequence $sequence
        $due = $started.AddSeconds($sequence * $SampleIntervalSeconds)
        $remaining = [int][Math]::Ceiling(($due - [DateTime]::UtcNow).TotalSeconds)
        if ($remaining -lt 0) { Fail "copy $Copy action $($action.name) missed sample $sequence by $(-$remaining)s" }
        if ($remaining -gt 0) { Start-Sleep -Seconds $remaining }
        Write-JsonAtomic $StatePath ([ordered]@{ ready = $true; copy = $Copy; sequence = $sequence; completed_action_count = $sequence; action = $action.name })
    }
    $releasePath = "$StatePath.release"
    $releaseDeadline = [DateTime]::UtcNow.AddSeconds(30)
    while (-not (Test-Path -LiteralPath $releasePath)) {
        if ([DateTime]::UtcNow -gt $releaseDeadline) { Fail "copy $Copy did not receive controller release" }
        Start-Sleep -Milliseconds 50
    }
}

function Get-ProcessSample {
    param([string]$CopyName, [int]$SampleIndex, [int]$CopyPid, [object]$State)
    $process = Get-Process -Id $CopyPid -ErrorAction Stop
    $memory = [int64]$process.PrivateMemorySize64
    $handles = [int64]$process.HandleCount
    $threads = [int]$process.Threads.Count
    if ($memory -lt 1 -or $handles -lt 0 -or $threads -lt 1) { Fail "copy $CopyName returned incomplete process metrics" }
    [ordered]@{
        schema = 'osl-active-long-run-sample.v1'
        copy = $CopyName
        sample_index = $SampleIndex
        sampled_at_utc = [DateTime]::UtcNow.ToString('o')
        process_id = $CopyPid
        memory_bytes = $memory
        handle_count = $handles
        thread_count = $threads
        completed_action_count = [int]$State.completed_action_count
        completed_action = $State.action
    }
}

if ($Worker) {
    Invoke-Worker
    exit 0
}

if (-not $RepositoryRoot) {
    $RepositoryRoot = (Resolve-Path (Join-Path $PSScriptRoot '../..')).Path
}
if (-not (Test-Path -LiteralPath (Join-Path $RepositoryRoot 'apps/osl-hub/Cargo.toml'))) {
    Fail "repository root does not contain apps/osl-hub/Cargo.toml: $RepositoryRoot"
}
if (-not $OutputDirectory) {
    $stamp = [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssZ')
    $OutputDirectory = Join-Path $RepositoryRoot "docs/reports/active-long-run/task-3676-$stamp"
}
$OutputDirectory = [System.IO.Path]::GetFullPath($OutputDirectory)
[void](New-Item -ItemType Directory -Path $OutputDirectory -Force)

$workerScript = $PSCommandPath
$workers = @()
$states = @{}
$samplesPath = Join-Path $OutputDirectory 'samples.jsonl'
$controllerLog = Join-Path $OutputDirectory 'controller.jsonl'
try {
    # Compile and execute every concrete journey before the clock starts.  The
    # one-minute slots must measure active use, never a first-build delay.
    if ($ActionPlanMode -eq 'Live') {
        $preflightLog = Join-Path $OutputDirectory 'preflight.actions.jsonl'
        $preflightPlan = Get-ActionPlan $RepositoryRoot $ActionPlanMode
        $preflightSequence = 0
        foreach ($preflightAction in $preflightPlan) {
            $preflightSequence++
            Invoke-ActionCommand -Name $preflightAction.name -Command $preflightAction.command -WorkingDirectory $preflightAction.cwd `
                -TimeoutSeconds 300 -LogPath $preflightLog -CopyName 'preflight' -Sequence $preflightSequence
        }
    }
    foreach ($copyName in @('A', 'B')) {
        $state = Join-Path $OutputDirectory "copy-$copyName.state.json"
        $actionLog = Join-Path $OutputDirectory "copy-$copyName.actions.jsonl"
        $stdout = Join-Path $OutputDirectory "copy-$copyName.worker.stdout.log"
        $stderr = Join-Path $OutputDirectory "copy-$copyName.worker.stderr.log"
        $arguments = @('-NoProfile', '-NonInteractive', '-File', $workerScript, '-Worker', '-Copy', $copyName,
            '-StatePath', $state, '-ActionLogPath', $actionLog, '-RepositoryRoot', $RepositoryRoot,
            '-Minutes', $Minutes, '-SampleIntervalSeconds', $SampleIntervalSeconds, '-ActionTimeoutSeconds', $ActionTimeoutSeconds,
            '-ActionPlanMode', $ActionPlanMode)
        $workerProcess = Start-Process -FilePath (Get-Process -Id $PID).Path -ArgumentList $arguments -PassThru `
            -RedirectStandardOutput $stdout -RedirectStandardError $stderr
        $workers += [pscustomobject]@{ copy = $copyName; process = $workerProcess; state = $state; stdout = $stdout; stderr = $stderr }
    }
    $readyDeadline = [DateTime]::UtcNow.AddSeconds(30)
    while ($true) {
        $allReady = $true
        foreach ($workerInfo in $workers) {
            if (-not (Test-Path -LiteralPath $workerInfo.state)) { $allReady = $false; break }
            $state = Read-JsonFile $workerInfo.state
            if (-not $state.ready) { $allReady = $false; break }
        }
        if ($allReady) { break }
        if ([DateTime]::UtcNow -gt $readyDeadline) { Fail 'workers did not become ready within 30 seconds' }
        Start-Sleep -Milliseconds 100
    }
    for ($sampleIndex = 0; $sampleIndex -le $Minutes; $sampleIndex++) {
        foreach ($workerInfo in $workers) {
            $state = Read-JsonFile $workerInfo.state
            if ([int]$state.sequence -ne $sampleIndex) {
                Fail "copy $($workerInfo.copy) state sequence $($state.sequence) does not match sample $sampleIndex"
            }
            $sample = Get-ProcessSample -CopyName $workerInfo.copy -SampleIndex $sampleIndex -CopyPid $workerInfo.process.Id -State $state
            Write-JsonLine $samplesPath $sample
            Write-Host ("TASK3676 sample copy={0} index={1} memory_bytes={2} handle_count={3} thread_count={4} completed_action_count={5}" -f $sample.copy, $sample.sample_index, $sample.memory_bytes, $sample.handle_count, $sample.thread_count, $sample.completed_action_count)
        }
        if ($sampleIndex -eq $Minutes) { break }
        $next = $sampleIndex + 1
        while ($true) {
            $allAtSample = $true
            foreach ($workerInfo in $workers) {
                if ($workerInfo.process.HasExited) { Fail "copy $($workerInfo.copy) exited before sample $next; see $($workerInfo.stderr)" }
                $state = Read-JsonFile $workerInfo.state
                if ([int]$state.sequence -lt $next) { $allAtSample = $false; break }
                if ([int]$state.sequence -gt $next) { Fail "copy $($workerInfo.copy) skipped sample $next" }
            }
            if ($allAtSample) { break }
            Start-Sleep -Milliseconds 100
        }
    }
    foreach ($workerInfo in $workers) {
        Set-Content -LiteralPath "$($workerInfo.state).release" -Value 'release' -NoNewline -Encoding ascii
    }
    foreach ($workerInfo in $workers) {
        $workerInfo.process.WaitForExit()
        if ($workerInfo.process.ExitCode -ne 0) { Fail "copy $($workerInfo.copy) exited $($workerInfo.process.ExitCode); see $($workerInfo.stderr)" }
    }
    $parsedSamples = @(Get-Content -LiteralPath $samplesPath -Encoding utf8 | ForEach-Object { $_ | ConvertFrom-Json })
    foreach ($copyName in @('A', 'B')) {
        $copySamples = @($parsedSamples | Where-Object { $_.copy -eq $copyName })
        if ($copySamples.Count -ne ($Minutes + 1)) { Fail "copy $copyName recorded $($copySamples.Count) samples, expected $($Minutes + 1)" }
        foreach ($sample in $copySamples) {
            if ($null -eq $sample.memory_bytes -or $null -eq $sample.handle_count -or $null -eq $sample.thread_count -or $null -eq $sample.completed_action_count) {
                Fail "copy $copyName sample $($sample.sample_index) is missing a required metric"
            }
        }
    }
    $summary = [ordered]@{
        schema = 'osl-active-long-run-summary.v1'; minutes = $Minutes; sample_interval_seconds = $SampleIntervalSeconds
        expected_samples_per_copy = $Minutes + 1; samples_per_copy = @{ A = $Minutes + 1; B = $Minutes + 1 }
        journeys = @('osl-chats-send', 'osl-chats-read', 'friend', 'file', 'settings', 'scrub')
        samples_jsonl = $samplesPath
    }
    $summaryPath = Join-Path $OutputDirectory 'summary.json'
    $summary | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath $summaryPath -Encoding utf8
    Write-JsonLine $controllerLog ([ordered]@{ result = 'pass'; summary = $summaryPath })
    Write-Host "TASK3676 finish samples_per_copy_A=$($Minutes + 1) samples_per_copy_B=$($Minutes + 1) samples=$samplesPath"
}
finally {
    foreach ($workerInfo in $workers) {
        if (-not $workerInfo.process.HasExited) {
            Stop-Process -Id $workerInfo.process.Id -Force -ErrorAction SilentlyContinue
            $workerInfo.process.WaitForExit()
        }
    }
}
