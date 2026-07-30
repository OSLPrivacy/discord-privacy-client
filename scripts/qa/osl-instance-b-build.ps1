<#
.SYNOPSIS
    Builds and stages a second OSL Hub executable with a distinct Tauri
    identifier. Emits %TEMP%\osl-instance-b-build.json.

.DESCRIPTION
    Instance B has to be a separate build, not just a second process. The
    Windows single-instance marker, the WebView2 profile, and Tauri's
    application data root are all keyed by the compile-time identifier from
    apps/osl-hub/tauri.conf.json. If B uses the same identifier as A, Windows
    single-instance activation hands argv to A and exits, and both processes
    would share one identity root anyway.

    This script changes only the Tauri identifier through a temporary
    `tauri build --config` overlay. It does not edit tauri.conf.json, run the
    result, create an identity, contact any service, or drive the UI. Launching
    and explicit consent for disposable identity creation remain owned by
    scripts/qa/osl-launch-instance-b.ps1.

.EXAMPLE
    powershell -NoProfile -ExecutionPolicy Bypass -File .\osl-instance-b-build.ps1 `
      -Identifier org.oslprivacy.hubqab
#>

[CmdletBinding()]
param(
    [string]$Identifier = 'org.oslprivacy.hubqab',
    [string]$BundleA = 'org.oslprivacy.hub',
    [string]$RepoRoot = '',
    [string]$StageDir = 'C:\OSL-QA-B',
    [string]$JsonOut = (Join-Path $env:TEMP 'osl-instance-b-build.json'),
    [string]$Log = (Join-Path $env:TEMP 'osl-instance-b-build.log'),
    [string]$LoaderDll = '',
    [string]$InstanceAExe = ''
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Off

$script:RunStartLocal = Get-Date
$script:RunStartIso = $script:RunStartLocal.ToString('o')
$script:Steps = @()

function Say {
    param([string]$Message, [string]$Color = 'Gray')
    Write-Host $Message -ForegroundColor $Color
}

function Add-Step {
    param([string]$Name, [string]$Result, [string]$Detail)
    $script:Steps += [pscustomobject][ordered]@{
        step = $Name
        result = $Result
        detail = $Detail
    }
    $color = switch ($Result) { 'ok' { 'Green' } 'failed' { 'Red' } default { 'Yellow' } }
    Say ('[{0,-24}] {1,-8} {2}' -f $Name, $Result, $Detail) $color
}

function Get-Sha256 {
    param([Parameter(Mandatory = $true)][string]$Path)
    (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
}

function Test-BinaryContainsAscii {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string]$Needle
    )
    $haystack = [IO.File]::ReadAllBytes($Path)
    $needleBytes = [Text.Encoding]::ASCII.GetBytes($Needle)
    if ($needleBytes.Length -eq 0 -or $haystack.Length -lt $needleBytes.Length) { return $false }
    for ($i = 0; $i -le ($haystack.Length - $needleBytes.Length); $i++) {
        $matched = $true
        for ($j = 0; $j -lt $needleBytes.Length; $j++) {
            if ($haystack[$i + $j] -ne $needleBytes[$j]) {
                $matched = $false
                break
            }
        }
        if ($matched) { return $true }
    }
    return $false
}

function Write-Result {
    param([string]$Verdict, [string]$Diagnosis, [string]$Remedy, $Extra = $null)
    $payload = [ordered]@{
        schemaVersion = 1
        tool = 'osl-instance-b-build'
        runStartedAt = $script:RunStartIso
        identifier = $Identifier
        bundleA = $BundleA
        overall = [ordered]@{
            verdict = $Verdict
            diagnosis = $Diagnosis
            remedy = $Remedy
            note = 'This verdict describes only whether a separately identified instance-B build was staged. It does not launch the app, create an identity, register anything, or verify product behavior.'
        }
        steps = $script:Steps
        diffKey = ($Verdict.ToUpperInvariant() + ':' + $Diagnosis)
    }
    if ($Extra) {
        foreach ($key in $Extra.Keys) { $payload[$key] = $Extra[$key] }
    }
    $jsonParent = Split-Path -Parent $JsonOut
    if ($jsonParent -and -not (Test-Path -LiteralPath $jsonParent)) {
        [void](New-Item -ItemType Directory -Path $jsonParent -Force)
    }
    $payload | ConvertTo-Json -Depth 12 | Out-File -LiteralPath $JsonOut -Encoding utf8
    $color = switch ($Verdict) { 'ok' { 'Green' } 'failed' { 'Red' } default { 'Yellow' } }
    Say ''
    Say ('BUILD-B VERDICT: {0} -- {1}' -f $Verdict.ToUpperInvariant(), $Diagnosis) $color
    if ($Remedy) { Say ('next: {0}' -f $Remedy) 'Cyan' }
    Say ('JSON: {0}' -f $JsonOut) 'DarkGray'
    if ($Verdict -eq 'ok') { exit 0 }
    if ($Verdict -eq 'failed') { exit 1 }
    exit 2
}

try {
    ([ordered]@{
        schemaVersion = 1
        tool = 'osl-instance-b-build'
        runStartedAt = $script:RunStartIso
        identifier = $Identifier
        overall = [ordered]@{
            verdict = 'in-progress'
            note = 'This run has not finished. Do not treat an older verdict in this file as current.'
        }
    } | ConvertTo-Json -Depth 6) | Out-File -LiteralPath $JsonOut -Encoding utf8
} catch {
    Say ('WARNING: could not stamp in-progress JSON at {0}: {1}' -f $JsonOut, $_.Exception.Message) 'Yellow'
}

if ($Identifier -eq $BundleA) {
    Add-Step 'gate/distinct-id' 'failed' ('Identifier equals BundleA: {0}' -f $BundleA)
    Write-Result 'blocked' `
        ('-Identifier equals instance A''s identifier ({0}); that build can never run beside A.' -f $BundleA) `
        'Pick a different identifier, for example org.oslprivacy.hubqab.'
}
if ($Identifier -cnotmatch '^[A-Za-z0-9][A-Za-z0-9.-]{2,95}$') {
    Add-Step 'gate/identifier-shape' 'failed' $Identifier
    Write-Result 'blocked' `
        'The requested identifier is not safe as a bundle identifier, marker window class, and directory name.' `
        'Use only alphanumerics, dots, and hyphens, starting with an alphanumeric character.'
}
Add-Step 'gate/distinct-id' 'ok' ('A = "{0}", B = "{1}".' -f $BundleA, $Identifier)

if (-not $RepoRoot) {
    $RepoRoot = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
}
if (-not (Test-Path -LiteralPath $RepoRoot -PathType Container)) {
    Add-Step 'gate/repo' 'failed' ('Missing {0}' -f $RepoRoot)
    Write-Result 'blocked' 'RepoRoot was not found.' 'Pass -RepoRoot pointing at this checkout.'
}
$RepoRoot = (Resolve-Path -LiteralPath $RepoRoot).Path
$HubRoot = Join-Path $RepoRoot 'apps\osl-hub'
$ConfigPath = Join-Path $HubRoot 'tauri.conf.json'
if (-not (Test-Path -LiteralPath $ConfigPath)) {
    Add-Step 'gate/repo' 'failed' ('Missing {0}' -f $ConfigPath)
    Write-Result 'blocked' 'apps/osl-hub/tauri.conf.json was not found under RepoRoot.' 'Pass -RepoRoot pointing at this checkout.'
}
$baseConfig = Get-Content -LiteralPath $ConfigPath -Raw | ConvertFrom-Json
if ($baseConfig.identifier -ne $BundleA) {
    Add-Step 'gate/base-id' 'failed' ('tauri.conf.json has "{0}", expected "{1}".' -f $baseConfig.identifier, $BundleA)
    Write-Result 'blocked' 'The checked-in Tauri identifier does not match -BundleA, so the build would not prove an A/B split.' 'Pass the correct -BundleA or restore the checked-in config.'
}
Add-Step 'gate/repo' 'ok' $RepoRoot

$distRoot = Join-Path $RepoRoot 'apps\osl-hub-ui\dist'
if (-not (Test-Path -LiteralPath $distRoot)) {
    Add-Step 'gate/dist' 'failed' ('Missing {0}' -f $distRoot)
    Write-Result 'blocked' 'apps/osl-hub-ui/dist is missing. Tauri embeds that directory at compile time.' 'Build the renderer before building either A or B.'
}
$distFiles = @(Get-ChildItem -LiteralPath $distRoot -Recurse -File -ErrorAction SilentlyContinue)
if ($distFiles.Count -eq 0) {
    Add-Step 'gate/dist' 'failed' ('Empty {0}' -f $distRoot)
    Write-Result 'blocked' 'apps/osl-hub-ui/dist is empty. Tauri would not embed a usable renderer.' 'Build the renderer before building either A or B.'
}
$srcRoots = @(
    (Join-Path $RepoRoot 'apps\osl-hub-ui\src'),
    (Join-Path $RepoRoot 'apps\osl-hub-ui\index.html')
)
$srcFiles = @()
foreach ($root in $srcRoots) {
    if (Test-Path -LiteralPath $root -PathType Leaf) {
        $srcFiles += Get-Item -LiteralPath $root
    } elseif (Test-Path -LiteralPath $root -PathType Container) {
        $srcFiles += Get-ChildItem -LiteralPath $root -Recurse -File |
            Where-Object { $_.Name -notmatch '\.test\.(ts|tsx|js|jsx)$' }
    }
}
if ($srcFiles.Count -gt 0) {
    $newestSource = ($srcFiles | Sort-Object LastWriteTimeUtc -Descending | Select-Object -First 1).LastWriteTimeUtc
    $newestDist = ($distFiles | Sort-Object LastWriteTimeUtc -Descending | Select-Object -First 1).LastWriteTimeUtc
    if ($newestSource -gt $newestDist) {
        Add-Step 'gate/dist-fresh' 'failed' 'A non-test frontend source is newer than dist.'
        Write-Result 'blocked' 'B would embed a stale renderer, making A/B evidence ambiguous.' 'Rebuild the renderer and rebuild instance A before staging B.'
    }
}
Add-Step 'gate/dist' 'ok' ('{0} files ready for Tauri embedding.' -f $distFiles.Count)

$overlayPath = Join-Path $env:TEMP ('osl-instance-b-tauri-overlay-{0}.json' -f ([Guid]::NewGuid().ToString('N')))
try {
    $overlayJson = ([ordered]@{ identifier = $Identifier } | ConvertTo-Json -Compress)
    [IO.File]::WriteAllText($overlayPath, $overlayJson, [Text.UTF8Encoding]::new($false))

    $buildArgs = @(
        'tauri', 'build',
        '--debug',
        '--features', 'desktop,discord-qa-shell',
        '--config', $overlayPath
    )
    Say '=== OSL instance-B build ===' 'Cyan'
    Say ('identifier: {0} (A stays {1})' -f $Identifier, $BundleA) 'Cyan'
    Say ('log       : {0}' -f $Log) 'DarkGray'

    $buildStarted = Get-Date
    Push-Location $HubRoot
    try {
        & cargo @buildArgs *> $Log
        $buildExit = if (Test-Path Variable:LASTEXITCODE) { $LASTEXITCODE } else { 0 }
    } finally {
        Pop-Location
    }
    $buildSeconds = [int]((Get-Date) - $buildStarted).TotalSeconds
    if ($buildExit -ne 0) {
        Add-Step 'build/tauri' 'failed' ('cargo tauri build exited {0}.' -f $buildExit)
        $tail = ''
        try { $tail = ((Get-Content -LiteralPath $Log -Tail 5) -join ' ') } catch { $tail = $_.Exception.Message }
        Write-Result 'blocked' ('tauri build exited {0} after {1}s. Tail: {2}' -f $buildExit, $buildSeconds, $tail) 'Read the build log and retry.'
    }
    Add-Step 'build/tauri' 'ok' ('completed in {0}s.' -f $buildSeconds)

    $exeCandidates = @(
        (Join-Path $HubRoot 'target\debug\osl-privacy-hub.exe'),
        (Join-Path $RepoRoot 'target\debug\osl-privacy-hub.exe')
    )
    $builtExe = ''
    foreach ($candidate in $exeCandidates) {
        if (Test-Path -LiteralPath $candidate) {
            $builtExe = (Resolve-Path -LiteralPath $candidate).Path
            break
        }
    }
    if (-not $builtExe) {
        Add-Step 'gate/exe' 'failed' 'No debug executable found after build.'
        Write-Result 'blocked' 'tauri build reported success, but osl-privacy-hub.exe was not found in the expected target directories.' 'Read the build log; do not substitute another executable.'
    }
    if (-not (Test-BinaryContainsAscii -Path $builtExe -Needle $Identifier)) {
        Add-Step 'gate/embedded-id' 'failed' ('{0} is absent from the built exe.' -f $Identifier)
        Write-Result 'blocked' 'The built executable does not contain the requested identifier, so the config overlay did not take.' 'Do not run this binary; check tauri build --config handling.'
    }
    Add-Step 'gate/embedded-id' 'ok' 'Requested identifier is present in the built executable.'

    if (-not (Test-Path -LiteralPath $StageDir)) {
        [void](New-Item -ItemType Directory -Path $StageDir -Force)
    }
    $stagedExe = Join-Path $StageDir 'osl-privacy-hub.exe'
    if ($InstanceAExe -and (Test-Path -LiteralPath $InstanceAExe)) {
        $aResolved = (Resolve-Path -LiteralPath $InstanceAExe).Path
        if ((Test-Path -LiteralPath $stagedExe) -and ((Resolve-Path -LiteralPath $stagedExe).Path -eq $aResolved)) {
            Add-Step 'gate/stage' 'failed' 'Stage target resolves to instance A executable.'
            Write-Result 'blocked' 'Refusing to stage over instance A executable.' 'Pick a different -StageDir.'
        }
    }
    Copy-Item -LiteralPath $builtExe -Destination $stagedExe -Force

    if (-not $LoaderDll) {
        $loaderCandidates = @(
            (Join-Path (Split-Path -Parent $builtExe) 'WebView2Loader.dll'),
            (Join-Path $HubRoot 'target\debug\WebView2Loader.dll'),
            (Join-Path $RepoRoot 'target\debug\WebView2Loader.dll')
        )
        foreach ($candidate in $loaderCandidates) {
            if (Test-Path -LiteralPath $candidate) {
                $LoaderDll = (Resolve-Path -LiteralPath $candidate).Path
                break
            }
        }
        if (-not $LoaderDll) {
            $tempLoaders = @(Get-ChildItem -LiteralPath $env:TEMP -Recurse -Filter 'WebView2Loader.dll' -ErrorAction SilentlyContinue |
                Sort-Object LastWriteTimeUtc -Descending)
            if ($tempLoaders.Count -gt 0) { $LoaderDll = $tempLoaders[0].FullName }
        }
    }
    if (-not $LoaderDll -or -not (Test-Path -LiteralPath $LoaderDll)) {
        Add-Step 'gate/webview2loader' 'failed' 'WebView2Loader.dll not found.'
        Write-Result 'blocked' 'WebView2Loader.dll was not found. The executable alone can hang before main(), which is not valid product evidence.' 'Pass -LoaderDll pointing at WebView2Loader.dll from the matching Windows build.'
    }
    $stagedLoader = Join-Path $StageDir 'WebView2Loader.dll'
    Copy-Item -LiteralPath $LoaderDll -Destination $stagedLoader -Force
    Add-Step 'stage/artifacts' 'ok' ('staged to {0}' -f $StageDir)

    $exeSha = Get-Sha256 -Path $stagedExe
    $loaderSha = Get-Sha256 -Path $stagedLoader
    $aSha = ''
    if ($InstanceAExe -and (Test-Path -LiteralPath $InstanceAExe)) { $aSha = Get-Sha256 -Path $InstanceAExe }
    if ($aSha -and $aSha -eq $exeSha) {
        Add-Step 'gate/distinct-binary' 'failed' 'Staged B hash equals instance A hash.'
        Write-Result 'blocked' 'The staged B executable has the same SHA-256 as instance A, so it is not proven to be a distinct-identifier build.' 'Rebuild with a distinct -Identifier and do not run this binary.' `
            @{ exeSha256 = $exeSha; instanceA = [ordered]@{ exe = $InstanceAExe; sha256 = $aSha } }
    }

    Write-Result 'ok' ('instance B staged to {0}' -f $StageDir) `
        ('scripts\qa\osl-launch-instance-b.ps1 -ExeB "{0}" -BundleB {1} -ConfirmCreatesIdentity' -f $stagedExe, $Identifier) `
        ([ordered]@{
            stageDir = $StageDir
            exe = $stagedExe
            exeSha256 = $exeSha
            webview2Loader = $stagedLoader
            webview2LoaderSha256 = $loaderSha
            buildSeconds = $buildSeconds
            log = $Log
            instanceA = [ordered]@{ exe = $InstanceAExe; sha256 = $aSha }
            distinctFromA = [bool]($aSha -and $aSha -ne $exeSha)
        })
} finally {
    if ($overlayPath -and (Test-Path -LiteralPath $overlayPath)) {
        Remove-Item -LiteralPath $overlayPath -Force -ErrorAction SilentlyContinue
    }
}
