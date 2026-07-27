<#
Runs only on an isolated Azure QA VM because it imports vmqa-win32.ps1, whose IMDS allow-list is
the load-time guard for every input-capable helper in that module.

This test drives the production bounds validator used by the capture precondition. It proves:
  1. DWM extended-frame bounds inside both GetWindowRect and the virtual screen are accepted.
  2. Reusing GetWindowRect (with its invisible resize border) as the capture rect is refused.
  3. Labelling whole-desktop geometry as the surface is refused before capture.
#>

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

. (Join-Path $PSScriptRoot 'vmqa-win32.ps1')

function New-TestRect {
    param(
        [Parameter(Mandatory)][int]$Left,
        [Parameter(Mandatory)][int]$Top,
        [Parameter(Mandatory)][int]$Right,
        [Parameter(Mandatory)][int]$Bottom
    )
    $rect = New-Object VmqaNative+RECT
    $rect.Left = $Left
    $rect.Top = $Top
    $rect.Right = $Right
    $rect.Bottom = $Bottom
    return $rect
}

$virtual = New-TestRect -Left 0 -Top 0 -Right 1024 -Bottom 768
$rawWithInvisibleBorder = New-TestRect -Left 0 -Top 0 -Right 1044 -Bottom 788
$trustedDwm = New-TestRect -Left 0 -Top 0 -Right 1024 -Bottom 728

[VmqaNative]::ValidateExtendedFrameBounds($rawWithInvisibleBorder, $trustedDwm, $virtual)
$trustedSurface = [VmqaNative]::BuildTrustedSurface(
    [IntPtr]::Zero, 1, 'unit-surface', $rawWithInvisibleBorder, $trustedDwm, $virtual, 96)
[void](Assert-VmqaTrustedSurfaceBounds -Surface $trustedSurface)
$passed = 1

try {
    [VmqaNative]::ValidateExtendedFrameBounds(
        $rawWithInvisibleBorder, $rawWithInvisibleBorder, $virtual)
    throw 'NEGATIVE_CONTROL_FAILED: invisible GetWindowRect border was accepted'
} catch {
    if ([string]$_.Exception.Message -like 'NEGATIVE_CONTROL_FAILED:*') { throw }
    $passed++
}

$wholeDesktopRect = New-TestRect -Left 0 -Top 0 -Right 1024 -Bottom 768
try {
    [void][VmqaNative]::BuildTrustedSurface(
        [IntPtr]::Zero, 1, 'unit-surface', $wholeDesktopRect, $wholeDesktopRect, $virtual, 96)
    throw 'NEGATIVE_CONTROL_FAILED: whole-desktop bounds were accepted'
} catch {
    if ([string]$_.Exception.Message -like 'NEGATIVE_CONTROL_FAILED:*') { throw }
    $passed++
}

if ($passed -ne 3) {
    throw "DWM_BOUNDS_SELFTEST_INCOMPLETE: passed=$passed expected=3"
}
Write-Output 'DWM_BOUNDS_SELFTEST_OK passed=3 negativeControls=2'
