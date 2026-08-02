<#
T17-T13 — run on a disposable Windows VM only.

Publishes no release and never changes a production feed.  The caller supplies
the test-feed installer URLs: a deliberately startup-broken 0.1.2 and its
higher-version repair.  It records the manual recovery steps, because a
startup-bricked app cannot invoke its updater.
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)] [uri] $BrokenInstaller,
    [Parameter(Mandatory = $true)] [uri] $RepairInstaller,
    [Parameter(Mandatory = $true)] [string] $EvidenceDirectory,
    [switch] $ConfirmDisposableVm
)

$ErrorActionPreference = 'Stop'
if (-not $ConfirmDisposableVm) {
    throw 'Refusing T17-T13: run only in a disposable Windows VM and pass -ConfirmDisposableVm.'
}
if ($BrokenInstaller.Host -eq 'github.com' -or $RepairInstaller.Host -eq 'github.com') {
    throw 'Refusing production GitHub URLs: T17-T13 requires a dedicated test feed.'
}

New-Item -ItemType Directory -Force -Path $EvidenceDirectory | Out-Null
$brokenPath = Join-Path $EvidenceDirectory 'osl-bad-0.1.2.exe'
$repairPath = Join-Path $EvidenceDirectory 'osl-repair.exe'
Invoke-WebRequest -Uri $BrokenInstaller -OutFile $brokenPath
Start-Process -FilePath $brokenPath -ArgumentList '/S' -Wait

# The operator must attempt launch and record the failure in the VM capture.
$manualSteps = 0
$manualSteps++ # open the out-of-band recovery instruction
$manualSteps++ # download the higher-version repair
Invoke-WebRequest -Uri $RepairInstaller -OutFile $repairPath
$manualSteps++ # run the repair installer
Start-Process -FilePath $repairPath -ArgumentList '/S' -Wait
$manualSteps++ # reopen OSL and confirm it starts

[ordered]@{
    test = 'T17-T13'
    broken_version = '0.1.2-test'
    recovery = 'manual higher-version reinstall'
    manual_steps = $manualSteps
    broken_installer_sha256 = (Get-FileHash $brokenPath -Algorithm SHA256).Hash
    repair_installer_sha256 = (Get-FileHash $repairPath -Algorithm SHA256).Hash
    executed_at_utc = (Get-Date).ToUniversalTime().ToString('o')
} | ConvertTo-Json | Set-Content -NoNewline (Join-Path $EvidenceDirectory 't17-t13-result.json')

Write-Host "T17-T13 complete; manual recovery required $manualSteps steps. Attach VM screenshots and t17-t13-result.json to the release evidence."
