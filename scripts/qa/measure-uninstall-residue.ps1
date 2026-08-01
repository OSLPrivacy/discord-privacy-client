<#
.SYNOPSIS
Captures the before/after evidence for T17-T10 on a clean Windows VM.

.DESCRIPTION
Run this script in two phases.  On a clean VM, install OSL, create an identity,
send one message, and run the Before phase.  Uninstall from Windows, then run the
After phase.  The resulting residue-report.json records a filesystem inventory,
a registry inventory, and a direct status for each data root that must not be
silently omitted from the diff.

The legacy root is deliberately included even though OSL Hub does not use it:
create a harmless probe file there before the Before phase so that a stock
uninstaller's treatment of a previous OSL installation is measured too.

.EXAMPLE
./measure-uninstall-residue.ps1 -Phase Before -EvidenceDirectory C:\OSL-QA\uninstall
# Install, create identity, send one message, then uninstall through Windows.
./measure-uninstall-residue.ps1 -Phase After -EvidenceDirectory C:\OSL-QA\uninstall
#>
[CmdletBinding()]
param(
    [ValidateSet('Before', 'After')]
    [string]$Phase,

    [string]$EvidenceDirectory,

    [switch]$CreateLegacyProbe,

    [switch]$SelfTest
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

function Get-ResidueRoots {
    if ([string]::IsNullOrWhiteSpace($env:APPDATA) -or
        [string]::IsNullOrWhiteSpace($env:LOCALAPPDATA)) {
        throw 'APPDATA and LOCALAPPDATA must be set on the Windows VM.'
    }

    return @(
        [pscustomobject]@{ Name = 'hub-config'; Path = (Join-Path $env:APPDATA 'org.oslprivacy.hub') }
        [pscustomobject]@{ Name = 'hub-local-data'; Path = (Join-Path $env:LOCALAPPDATA 'org.oslprivacy.hub') }
        [pscustomobject]@{ Name = 'legacy-osl'; Path = (Join-Path $env:APPDATA 'osl') }
    )
}

function Get-DirectoryInventory {
    param([Parameter(Mandatory)][string]$Path)

    if (-not (Test-Path -LiteralPath $Path -PathType Container)) {
        return @()
    }

    return @(
        Get-ChildItem -LiteralPath $Path -Force -Recurse -ErrorAction SilentlyContinue |
            ForEach-Object {
                [pscustomobject]@{
                    Path = $_.FullName
                    Kind = if ($_.PSIsContainer) { 'directory' } else { 'file' }
                    Length = if ($_.PSIsContainer) { 0 } else { [int64]$_.Length }
                    LastWriteTimeUtc = $_.LastWriteTimeUtc.ToString('o')
                }
            }
    )
}

function Get-RootSnapshot {
    param([Parameter(Mandatory)]$Root)

    $exists = Test-Path -LiteralPath $Root.Path -PathType Container
    return [pscustomobject]@{
        name = $Root.Name
        path = $Root.Path
        exists = [bool]$exists
        entries = @(Get-DirectoryInventory -Path $Root.Path)
    }
}

function Get-FullFilesystemInventory {
    $systemDrive = [Environment]::GetEnvironmentVariable('SystemDrive')
    if ([string]::IsNullOrWhiteSpace($systemDrive)) {
        throw 'SystemDrive must be set on the Windows VM.'
    }
    $root = "$systemDrive\"
    if (-not (Test-Path -LiteralPath $root -PathType Container)) {
        throw "system drive is unavailable: $root"
    }

    # This is intentionally a complete metadata-only C: inventory.  A clean VM
    # keeps it tractable and avoids mistaking a hand-picked application directory
    # for a filesystem diff.
    return @(
        Get-ChildItem -LiteralPath $root -Force -Recurse -ErrorAction SilentlyContinue |
            ForEach-Object {
                [pscustomobject]@{
                    path = $_.FullName
                    kind = if ($_.PSIsContainer) { 'directory' } else { 'file' }
                    length = if ($_.PSIsContainer) { 0 } else { [int64]$_.Length }
                    lastWriteTimeUtc = $_.LastWriteTimeUtc.ToString('o')
                }
            }
    )
}

function Get-RegistryInventory {
    $locations = @(
        'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall',
        'HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall',
        'HKLM:\Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall'
    )

    return @(
        foreach ($location in $locations) {
            if (-not (Test-Path -LiteralPath $location)) { continue }
            Get-ChildItem -LiteralPath $location -ErrorAction SilentlyContinue |
                ForEach-Object {
                    $entry = Get-ItemProperty -LiteralPath $_.PSPath -ErrorAction SilentlyContinue
                    if ($null -ne $entry -and
                        (($entry.DisplayName -like '*OSL*') -or ($entry.PSChildName -like '*osl*'))) {
                        [pscustomobject]@{
                            key = $_.Name
                            displayName = [string]$entry.DisplayName
                            uninstallString = [string]$entry.UninstallString
                            quietUninstallString = [string]$entry.QuietUninstallString
                        }
                    }
                }
        }
    )
}

function Save-Snapshot {
    param(
        [Parameter(Mandatory)][string]$SnapshotPath,
        [Parameter(Mandatory)][string]$SnapshotPhase
    )

    $snapshot = [ordered]@{
        schemaVersion = 1
        phase = $SnapshotPhase
        capturedAtUtc = [DateTime]::UtcNow.ToString('o')
        roots = @(Get-ResidueRoots | ForEach-Object { Get-RootSnapshot $_ })
        filesystemEntries = @(Get-FullFilesystemInventory)
        registry = @(Get-RegistryInventory)
    }
    $snapshot | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $SnapshotPath -Encoding utf8
    return $snapshot
}

function New-ResidueReport {
    param(
        [Parameter(Mandatory)]$Before,
        [Parameter(Mandatory)]$After
    )

    $surviving = @()
    $missing = @()
    foreach ($beforeRoot in @($Before.roots)) {
        $afterRoot = @($After.roots | Where-Object { $_.name -ceq $beforeRoot.name })
        if ($afterRoot.Count -ne 1) { throw "after snapshot has no unique entry for $($beforeRoot.name)" }
        if ($beforeRoot.exists -and $afterRoot[0].exists) {
            $surviving += [pscustomobject]@{
                name = $beforeRoot.name
                path = $beforeRoot.path
                beforeEntryCount = @($beforeRoot.entries).Count
                afterEntryCount = @($afterRoot[0].entries).Count
            }
        } elseif ($beforeRoot.exists) {
            $missing += [pscustomobject]@{ name = $beforeRoot.name; path = $beforeRoot.path }
        }
    }

    $beforeFilesystem = @($Before.filesystemEntries | ForEach-Object { $_.path })
    $afterFilesystem = @($After.filesystemEntries | ForEach-Object { $_.path })
    $filesystemDiff = @(Compare-Object -ReferenceObject $beforeFilesystem -DifferenceObject $afterFilesystem)

    return [ordered]@{
        schemaVersion = 1
        beforeCapturedAtUtc = $Before.capturedAtUtc
        afterCapturedAtUtc = $After.capturedAtUtc
        survivingDataDirectories = @($surviving)
        removedDataDirectories = @($missing)
        removedFilesystemEntries = @($filesystemDiff | Where-Object { $_.SideIndicator -ceq '<=' } | ForEach-Object { $_.InputObject })
        addedFilesystemEntries = @($filesystemDiff | Where-Object { $_.SideIndicator -ceq '=>' } | ForEach-Object { $_.InputObject })
        beforeRegistryEntries = @($Before.registry)
        afterRegistryEntries = @($After.registry)
    }
}

function Invoke-SelfTest {
    $before = [pscustomobject]@{
        capturedAtUtc = '2026-01-01T00:00:00.0000000Z'
        roots = @(
            [pscustomobject]@{ name = 'hub-config'; path = 'C:\\AppData\\org.oslprivacy.hub'; exists = $true; entries = @([pscustomobject]@{ Path = 'identity' }) }
            [pscustomobject]@{ name = 'hub-local-data'; path = 'C:\\LocalAppData\\org.oslprivacy.hub'; exists = $true; entries = @([pscustomobject]@{ Path = 'message' }) }
            [pscustomobject]@{ name = 'legacy-osl'; path = 'C:\\AppData\\osl'; exists = $true; entries = @([pscustomobject]@{ Path = 'legacy-probe' }) }
        )
        filesystemEntries = @(
            [pscustomobject]@{ path = 'C:\\AppData\\org.oslprivacy.hub\\identity'; kind = 'file' }
            [pscustomobject]@{ path = 'C:\\LocalAppData\\org.oslprivacy.hub\\message'; kind = 'file' }
            [pscustomobject]@{ path = 'C:\\AppData\\osl\\legacy-probe'; kind = 'file' }
        )
        registry = @()
    }
    $after = $before | ConvertTo-Json -Depth 8 | ConvertFrom-Json
    $after.capturedAtUtc = '2026-01-01T00:01:00.0000000Z'
    $report = New-ResidueReport -Before $before -After $after
    if (@($report.survivingDataDirectories).Count -ne 3) {
        throw 'self-test: surviving data directories were not all reported'
    }
    if (@($report.removedFilesystemEntries).Count -ne 0) {
        throw 'self-test: unchanged filesystem inventory was reported as removed'
    }

    $after.roots[2].exists = $false
    $after.roots[2].entries = @()
    $after.filesystemEntries = @($after.filesystemEntries | Where-Object { $_.path -cne 'C:\\AppData\\osl\\legacy-probe' })
    $deletionReport = New-ResidueReport -Before $before -After $after
    if (@($deletionReport.removedDataDirectories | Where-Object { $_.name -ceq 'legacy-osl' }).Count -ne 1) {
        throw 'self-test: a deleted legacy root was not exposed by the diff'
    }
    if (@($deletionReport.removedFilesystemEntries | Where-Object { $_ -ceq 'C:\\AppData\\osl\\legacy-probe' }).Count -ne 1) {
        throw 'self-test: a deleted legacy file was not exposed by the filesystem diff'
    }
    Write-Output 'ok - T17-T10 residue report lists survivors and exposes a deleted legacy root'
}

if ($SelfTest) {
    Invoke-SelfTest
    exit 0
}

if ([string]::IsNullOrWhiteSpace($Phase) -or [string]::IsNullOrWhiteSpace($EvidenceDirectory)) {
    throw 'Phase and EvidenceDirectory are required unless -SelfTest is used.'
}

$resolvedEvidenceDirectory = [IO.Path]::GetFullPath($EvidenceDirectory)
[void](New-Item -ItemType Directory -Path $resolvedEvidenceDirectory -Force)
$beforePath = Join-Path $resolvedEvidenceDirectory 'before-uninstall.json'
$afterPath = Join-Path $resolvedEvidenceDirectory 'after-uninstall.json'
$reportPath = Join-Path $resolvedEvidenceDirectory 'residue-report.json'

if ($Phase -ceq 'Before') {
    if ($CreateLegacyProbe) {
        $legacyPath = (Get-ResidueRoots | Where-Object { $_.Name -ceq 'legacy-osl' }).Path
        [void](New-Item -ItemType Directory -Path $legacyPath -Force)
        Set-Content -LiteralPath (Join-Path $legacyPath 'osl-uninstall-residue-probe.txt') `
            -Value 'T17-C1 legacy-root probe; safe to remove after evidence capture.' -Encoding utf8
    }
    Save-Snapshot -SnapshotPath $beforePath -SnapshotPhase 'before-uninstall' | Out-Null
    Write-Output "captured before-uninstall evidence: $beforePath"
    exit 0
}

if (-not (Test-Path -LiteralPath $beforePath -PathType Leaf)) {
    throw "before-uninstall snapshot is required: $beforePath"
}
Save-Snapshot -SnapshotPath $afterPath -SnapshotPhase 'after-uninstall' | Out-Null
$before = Get-Content -LiteralPath $beforePath -Raw | ConvertFrom-Json -ErrorAction Stop
$after = Get-Content -LiteralPath $afterPath -Raw | ConvertFrom-Json -ErrorAction Stop
$report = New-ResidueReport -Before $before -After $after
$report | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $reportPath -Encoding utf8
Write-Output "captured post-uninstall evidence: $afterPath"
Write-Output "residue report: $reportPath"
@($report.survivingDataDirectories) | ForEach-Object { Write-Output "SURVIVES: $($_.name) $($_.path)" }
