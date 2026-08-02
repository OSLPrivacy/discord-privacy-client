<#
.SYNOPSIS
  TW-12 — verify that closing OSL restores every borrowed top-level window.

.DESCRIPTION
  Capture the borrowed processes immediately before OSL adopts them, close OSL, then capture the
  same processes again.  A pass requires each named substrate to have no surviving borrowed HWND
  and every pre-adoption window to have its original owner, styles, and placement.
  The caller supplies all five substrates; this script deliberately refuses a partial matrix.
#>

[CmdletBinding()]
param(
    [string]$BeforePath,
    [string]$AfterPath,
    [switch]$SelfTest
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$requiredSubstrates = @('native', 'mullvad', 'companion', 'service-host', 'whatsapp')

function Assert-Tw12Report {
    param([Parameter(Mandatory)]$Before, [Parameter(Mandatory)]$After)

    $beforeBySubstrate = @{}
    foreach ($entry in @($Before.substrates)) {
        if ([string]::IsNullOrWhiteSpace([string]$entry.name)) { throw 'TW-12_INVALID_BEFORE_SUBSTRATE' }
        $beforeBySubstrate[$entry.name] = $entry
    }
    $afterBySubstrate = @{}
    foreach ($entry in @($After.substrates)) {
        if ([string]::IsNullOrWhiteSpace([string]$entry.name)) { throw 'TW-12_INVALID_AFTER_SUBSTRATE' }
        $afterBySubstrate[$entry.name] = $entry
    }
    foreach ($name in $requiredSubstrates) {
        if (-not $beforeBySubstrate.ContainsKey($name) -or -not $afterBySubstrate.ContainsKey($name)) {
            throw "TW-12_INCOMPLETE_MATRIX: missing substrate $name"
        }
        $beforeWindows = @($beforeBySubstrate[$name].windows)
        $afterWindows = @($afterBySubstrate[$name].windows)
        $afterByHwnd = @{}
        foreach ($window in $afterWindows) {
            if ([bool]$window.borrowed) { throw "TW-12_ORPHANED_HWND: substrate=$name hwnd=$($window.hwnd)" }
            $afterByHwnd[[string]$window.hwnd] = $window
        }
        foreach ($window in $beforeWindows) {
            foreach ($field in 'hwnd', 'ownerHwnd', 'style', 'exStyle', 'rect') {
                if ($null -eq $window.$field) { throw "TW-12_INVALID_BEFORE_WINDOW: substrate=$name missing=$field" }
            }
            $restored = $afterByHwnd[[string]$window.hwnd]
            if ($null -eq $restored) { throw "TW-12_RESTORED_WINDOW_MISSING: substrate=$name hwnd=$($window.hwnd)" }
            foreach ($field in 'ownerHwnd', 'style', 'exStyle') {
                if ([string]$restored.$field -ne [string]$window.$field) { throw "TW-12_RESTORE_MISMATCH: substrate=$name hwnd=$($window.hwnd) field=$field" }
            }
            foreach ($edge in 'left', 'top', 'right', 'bottom') {
                if ([int]$restored.rect.$edge -ne [int]$window.rect.$edge) { throw "TW-12_RESTORE_MISMATCH: substrate=$name hwnd=$($window.hwnd) rect=$edge" }
            }
        }
    }
    return [ordered]@{ test = 'TW-12'; verdict = 'pass'; substrates = $requiredSubstrates }
}

function New-Tw12Fixture {
    param([bool]$Orphan)
    $before = @()
    $after = @()
    $number = 1
    foreach ($name in $requiredSubstrates) {
        $before += @{ name = $name; windows = @(@{ hwnd = "0x$number"; ownerHwnd = '0x0'; style = '0x10CF0000'; exStyle = '0x40000'; rect = @{ left = 10; top = 10; right = 400; bottom = 300 } }) }
        $restored = @{ hwnd = "0x$number"; borrowed = $false; ownerHwnd = '0x0'; style = '0x10CF0000'; exStyle = '0x40000'; rect = @{ left = 10; top = 10; right = 400; bottom = 300 } }
        if ($Orphan -and $name -eq 'companion') { $restored.borrowed = $true }
        $after += @{ name = $name; windows = @($restored) }
        $number++
    }
    return @{ before = @{ substrates = $before }; after = @{ substrates = $after } }
}

if ($SelfTest) {
    $fixture = New-Tw12Fixture $false
    $result = Assert-Tw12Report -Before ([pscustomobject]$fixture.before) -After ([pscustomobject]$fixture.after)
    if ($result.verdict -ne 'pass') { throw 'TW-12_SELFTEST_POSITIVE_FAILED' }
    $orphan = New-Tw12Fixture $true
    try {
        [void](Assert-Tw12Report -Before ([pscustomobject]$orphan.before) -After ([pscustomobject]$orphan.after))
        throw 'TW-12_SELFTEST_NEGATIVE_CONTROL_ACCEPTED'
    } catch {
        if ($_.Exception.Message -eq 'TW-12_SELFTEST_NEGATIVE_CONTROL_ACCEPTED') { throw }
        if ($_.Exception.Message -notlike 'TW-12_ORPHANED_HWND:*') { throw }
    }
    Write-Output 'TW-12 SELFTEST PASS: five-substrate restoration accepted; surviving companion HWND rejected'
    exit 0
}

if (-not $BeforePath -or -not $AfterPath) { throw 'TW-12_BEFORE_AND_AFTER_PATH_REQUIRED' }
$before = Get-Content -Raw -LiteralPath $BeforePath | ConvertFrom-Json
$after = Get-Content -Raw -LiteralPath $AfterPath | ConvertFrom-Json
(Assert-Tw12Report -Before $before -After $after) | ConvertTo-Json -Compress
