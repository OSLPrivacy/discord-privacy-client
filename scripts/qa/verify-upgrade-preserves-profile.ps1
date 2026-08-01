<#
.SYNOPSIS
Proves T17-T12: installing a newer OSL Hub installer preserves an existing profile.

.DESCRIPTION
Run this on a clean Windows VM in this order:

  1. InstallBaseline -BaselineInstaller <0.1.0.exe>
  2. Create an identity and send a message in OSL, then close OSL.
  3. CaptureBeforeUpgrade
  4. InstallUpgrade -UpgradeInstaller <0.1.1.exe>
  5. Unlock OSL and read the message created in step 2, then close OSL.
  6. Verify -MessageRead

The evidence contains hashes and counts only; it never copies identity or message
contents into the report.  A changed bundle identifier is an immediate PROFILE
LOST result because Tauri derives the profile root from that identifier.

.EXAMPLE
./verify-upgrade-preserves-profile.ps1 -Phase InstallBaseline `
  -BaselineInstaller C:\QA\osl-hub-0.1.0.exe -EvidenceDirectory C:\QA\upgrade
./verify-upgrade-preserves-profile.ps1 -Phase CaptureBeforeUpgrade -EvidenceDirectory C:\QA\upgrade
./verify-upgrade-preserves-profile.ps1 -Phase InstallUpgrade `
  -UpgradeInstaller C:\QA\osl-hub-0.1.1.exe -EvidenceDirectory C:\QA\upgrade
./verify-upgrade-preserves-profile.ps1 -Phase Verify -EvidenceDirectory C:\QA\upgrade -MessageRead
#>
[CmdletBinding()]
param(
    [ValidateSet('InstallBaseline', 'CaptureBeforeUpgrade', 'InstallUpgrade', 'Verify')]
    [string]$Phase,

    [string]$BaselineInstaller,
    [string]$UpgradeInstaller,
    [string]$EvidenceDirectory,

    # Keep these next to the two installers in release QA.  CI may call the
    # self-test, which asserts that the shipping config still has this value.
    [string]$BaselineConfig = (Join-Path $PSScriptRoot '../../apps/osl-hub/tauri.conf.json'),
    [string]$UpgradeConfig = (Join-Path $PSScriptRoot '../../apps/osl-hub/tauri.conf.json'),
    [string]$ExpectedIdentifier = 'org.oslprivacy.hub',

    [switch]$MessageRead,
    [switch]$SelfTest,
    [switch]$SabotageDifferentIdentifier
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

function Get-ConfigIdentifier {
    param([Parameter(Mandatory)][string]$ConfigPath)

    if (-not (Test-Path -LiteralPath $ConfigPath -PathType Leaf)) {
        throw "config is missing: $ConfigPath"
    }
    $config = Get-Content -LiteralPath $ConfigPath -Raw | ConvertFrom-Json
    $identifier = [string]$config.identifier
    if ([string]::IsNullOrWhiteSpace($identifier)) {
        throw "config has no non-empty identifier: $ConfigPath"
    }
    return $identifier
}

function Get-IdentifierVerdict {
    param(
        [Parameter(Mandatory)][string]$BaselineConfigPath,
        [Parameter(Mandatory)][string]$UpgradeConfigPath,
        [Parameter(Mandatory)][string]$Expected
    )

    $baseline = Get-ConfigIdentifier -ConfigPath $BaselineConfigPath
    $upgrade = Get-ConfigIdentifier -ConfigPath $UpgradeConfigPath
    if ($baseline -cne $Expected) {
        return [pscustomobject]@{ status = 'LOST'; baselineIdentifier = $baseline; upgradeIdentifier = $upgrade; reason = 'baseline identifier differs from the release identity contract' }
    }
    if ($upgrade -cne $Expected) {
        return [pscustomobject]@{ status = 'LOST'; baselineIdentifier = $baseline; upgradeIdentifier = $upgrade; reason = 'upgrade identifier differs from the release identity contract' }
    }
    if ($baseline -cne $upgrade) {
        return [pscustomobject]@{ status = 'LOST'; baselineIdentifier = $baseline; upgradeIdentifier = $upgrade; reason = 'installer identifiers differ' }
    }
    return [pscustomobject]@{ status = 'PRESERVED'; baselineIdentifier = $baseline; upgradeIdentifier = $upgrade; reason = 'identifiers match the release identity contract' }
}

function Assert-IdentifierPreserved {
    param(
        [Parameter(Mandatory)][string]$BaselineConfigPath,
        [Parameter(Mandatory)][string]$UpgradeConfigPath,
        [Parameter(Mandatory)][string]$Expected
    )

    $verdict = Get-IdentifierVerdict -BaselineConfigPath $BaselineConfigPath -UpgradeConfigPath $UpgradeConfigPath -Expected $Expected
    if ($verdict.status -ne 'PRESERVED') {
        Write-Output ("PROFILE LOST: {0}; baseline='{1}', upgrade='{2}'" -f $verdict.reason, $verdict.baselineIdentifier, $verdict.upgradeIdentifier)
        throw 'T17-T12 failed: an identifier change moves the profile root.'
    }
    return $verdict
}

function Get-ProfileSnapshot {
    param(
        [Parameter(Mandatory)][string]$Identifier,
        [Parameter(Mandatory)][string]$AppDataRoot
    )

    $profileRoot = Join-Path $AppDataRoot $Identifier
    if (-not (Test-Path -LiteralPath $profileRoot -PathType Container)) {
        throw "profile root is missing: $profileRoot. Create and unlock an identity before capturing evidence."
    }
    $files = @(Get-ChildItem -LiteralPath $profileRoot -File -Force -Recurse | ForEach-Object {
        [pscustomobject]@{
            relativePath = $_.FullName.Substring($profileRoot.Length).TrimStart('\\', '/')
            sha256 = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash
            length = [int64]$_.Length
        }
    })
    if ($files.Count -eq 0) {
        throw "profile root contains no files: $profileRoot. Create an identity and send a message before capturing evidence."
    }
    return [ordered]@{
        schemaVersion = 1
        capturedAtUtc = [DateTime]::UtcNow.ToString('o')
        identifier = $Identifier
        profileRoot = $profileRoot
        files = $files
    }
}

function Test-ProfileSnapshotPreserved {
    param(
        [Parameter(Mandatory)]$Before,
        [Parameter(Mandatory)]$After
    )

    if ($Before.identifier -cne $After.identifier) {
        return [pscustomobject]@{ status = 'LOST'; reason = 'captured profile identifiers differ'; missingFiles = 0; changedFiles = 0 }
    }
    $afterByPath = @{}
    foreach ($file in @($After.files)) { $afterByPath[$file.relativePath] = $file }
    $missing = 0
    $changed = 0
    foreach ($file in @($Before.files)) {
        $actual = $afterByPath[$file.relativePath]
        if ($null -eq $actual) { $missing++; continue }
        if ($actual.sha256 -cne $file.sha256 -or [int64]$actual.length -ne [int64]$file.length) { $changed++ }
    }
    if ($missing -gt 0 -or $changed -gt 0) {
        return [pscustomobject]@{ status = 'LOST'; reason = 'pre-upgrade profile files were removed or changed'; missingFiles = $missing; changedFiles = $changed }
    }
    return [pscustomobject]@{ status = 'PRESERVED'; reason = 'every pre-upgrade profile file remains byte-identical'; missingFiles = 0; changedFiles = 0 }
}

function Install-Quietly {
    param([Parameter(Mandatory)][string]$Installer)

    if (-not (Test-Path -LiteralPath $Installer -PathType Leaf)) { throw "installer is missing: $Installer" }
    $process = Start-Process -FilePath $Installer -ArgumentList '/S' -Wait -PassThru
    if ($process.ExitCode -ne 0) { throw "installer failed with exit code $($process.ExitCode): $Installer" }
}

function Invoke-SelfTest {
    # This is the CI assertion: the checked-in shipping configuration may not
    # move the profile root away from the established identifier.
    [void](Assert-IdentifierPreserved -BaselineConfigPath $BaselineConfig -UpgradeConfigPath $UpgradeConfig -Expected $ExpectedIdentifier)
    $temp = Join-Path ([IO.Path]::GetTempPath()) ("osl-upgrade-profile-selftest-" + [guid]::NewGuid().ToString('N'))
    try {
        [void](New-Item -ItemType Directory -Path $temp -Force)
        $baselineConfig = Join-Path $temp 'baseline.json'
        $upgradeConfig = Join-Path $temp 'upgrade.json'
        @{ identifier = 'org.oslprivacy.hub' } | ConvertTo-Json | Set-Content -LiteralPath $baselineConfig -Encoding utf8
        @{ identifier = $(if ($SabotageDifferentIdentifier) { 'org.oslprivacy.hub.changed' } else { 'org.oslprivacy.hub' }) } | ConvertTo-Json | Set-Content -LiteralPath $upgradeConfig -Encoding utf8

        $verdict = Get-IdentifierVerdict -BaselineConfigPath $baselineConfig -UpgradeConfigPath $upgradeConfig -Expected 'org.oslprivacy.hub'
        if ($SabotageDifferentIdentifier) {
            if ($verdict.status -ne 'LOST') { throw 'self-test sabotage was not reported LOST' }
            Write-Output 'PROFILE LOST: self-test detected a different upgrade identifier'
            throw 'T17-T12 RED: different identifier correctly fails the upgrade contract.'
        }
        if ($verdict.status -ne 'PRESERVED') { throw 'self-test: matching identifiers were not preserved' }

        $before = [pscustomobject]@{ identifier = 'org.oslprivacy.hub'; files = @([pscustomobject]@{ relativePath = 'osl-core/identity'; sha256 = 'A'; length = 1 }) }
        $after = [pscustomobject]@{ identifier = 'org.oslprivacy.hub'; files = @([pscustomobject]@{ relativePath = 'osl-core/identity'; sha256 = 'A'; length = 1 }) }
        if ((Test-ProfileSnapshotPreserved -Before $before -After $after).status -ne 'PRESERVED') { throw 'self-test: unchanged profile was not preserved' }
        $after.files[0].sha256 = 'B'
        if ((Test-ProfileSnapshotPreserved -Before $before -After $after).status -ne 'LOST') { throw 'self-test: changed profile file was not reported LOST' }
        Write-Output 'ok - T17-T12 keeps matching identifiers and reports changed identifiers or profile files LOST'
    } finally {
        if (Test-Path -LiteralPath $temp) { Remove-Item -LiteralPath $temp -Recurse -Force }
    }
}

if ($SelfTest) {
    Invoke-SelfTest
    exit 0
}

if ([string]::IsNullOrWhiteSpace($Phase) -or [string]::IsNullOrWhiteSpace($EvidenceDirectory)) {
    throw 'Phase and EvidenceDirectory are required unless -SelfTest is used.'
}
if ([string]::IsNullOrWhiteSpace($env:APPDATA)) { throw 'APPDATA must be set on the Windows VM.' }

$identifierVerdict = Assert-IdentifierPreserved -BaselineConfigPath $BaselineConfig -UpgradeConfigPath $UpgradeConfig -Expected $ExpectedIdentifier
$evidenceRoot = [IO.Path]::GetFullPath($EvidenceDirectory)
[void](New-Item -ItemType Directory -Path $evidenceRoot -Force)
$beforePath = Join-Path $evidenceRoot 'before-upgrade-profile.json'
$afterPath = Join-Path $evidenceRoot 'after-upgrade-profile.json'
$reportPath = Join-Path $evidenceRoot 'upgrade-preserves-profile-report.json'

switch ($Phase) {
    'InstallBaseline' {
        if ([string]::IsNullOrWhiteSpace($BaselineInstaller)) { throw 'BaselineInstaller is required for InstallBaseline.' }
        Install-Quietly -Installer $BaselineInstaller
        Write-Output 'baseline installed. Create an identity, send a message, close OSL, then run CaptureBeforeUpgrade.'
    }
    'CaptureBeforeUpgrade' {
        (Get-ProfileSnapshot -Identifier $ExpectedIdentifier -AppDataRoot $env:APPDATA) | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $beforePath -Encoding utf8
        Write-Output "captured pre-upgrade profile evidence: $beforePath"
    }
    'InstallUpgrade' {
        if ([string]::IsNullOrWhiteSpace($UpgradeInstaller)) { throw 'UpgradeInstaller is required for InstallUpgrade.' }
        Install-Quietly -Installer $UpgradeInstaller
        Write-Output 'upgrade installed. Unlock OSL and read the pre-upgrade message, close OSL, then run Verify -MessageRead.'
    }
    'Verify' {
        if (-not $MessageRead) { throw 'Verify requires -MessageRead after the operator has unlocked OSL and read the pre-upgrade message.' }
        if (-not (Test-Path -LiteralPath $beforePath -PathType Leaf)) { throw "pre-upgrade snapshot is required: $beforePath" }
        $before = Get-Content -LiteralPath $beforePath -Raw | ConvertFrom-Json
        $after = Get-ProfileSnapshot -Identifier $ExpectedIdentifier -AppDataRoot $env:APPDATA
        $after | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $afterPath -Encoding utf8
        $profileVerdict = Test-ProfileSnapshotPreserved -Before $before -After $after
        $report = [ordered]@{
            schemaVersion = 1
            identifier = $ExpectedIdentifier
            identifierContract = $identifierVerdict
            profileContract = $profileVerdict
            operatorConfirmedMessageRead = $true
            beforeFileCount = @($before.files).Count
            afterFileCount = @($after.files).Count
        }
        $report | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $reportPath -Encoding utf8
        if ($profileVerdict.status -ne 'PRESERVED') {
            Write-Output "PROFILE LOST: $($profileVerdict.reason)"
            throw "T17-T12 failed; evidence written to $reportPath"
        }
        Write-Output "ok - T17-T12 profile preserved; evidence: $reportPath"
    }
}
