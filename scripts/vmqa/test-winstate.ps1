$ErrorActionPreference = 'Stop'
$script = Join-Path $PSScriptRoot 'winstate.ps1'
$beforePath = Join-Path ([IO.Path]::GetTempPath()) ("winstate-before-" + [guid]::NewGuid() + '.json')
$afterPath = Join-Path ([IO.Path]::GetTempPath()) ("winstate-after-" + [guid]::NewGuid() + '.json')

function New-Fixture([int]$Left, [int]$ZOrderIndex) {
    return @(
        [ordered]@{
            hwnd = '0x100'; processId = 10; class = 'OSLMain'; style = '0x10'; exStyle = '0x20'; ownerHwnd = '0x0'
            rect = [ordered]@{ left = $Left; top = 40; right = $Left + 800; bottom = 640 }
            isIconic = $false; isWindowVisible = $true; zOrderIndex = $ZOrderIndex; dpi = 96; monitorId = '\\.\DISPLAY1'
        },
        [ordered]@{
            hwnd = '0x200'; processId = 99; class = 'OtherProcess'; style = '0x10'; exStyle = '0x20'; ownerHwnd = '0x0'
            rect = [ordered]@{ left = 1; top = 2; right = 3; bottom = 4 }
            isIconic = $false; isWindowVisible = $true; zOrderIndex = 0; dpi = 96; monitorId = '\\.\DISPLAY1'
        }
    )
}

try {
    (New-Fixture -Left 20 -ZOrderIndex 3) | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $beforePath -Encoding utf8
    (New-Fixture -Left 220 -ZOrderIndex 1) | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $afterPath -Encoding utf8
    $before = & $script -ProcessId 10 -FixtureWindowsPath $beforePath | ConvertFrom-Json
    $after = & $script -ProcessId 10 -FixtureWindowsPath $afterPath | ConvertFrom-Json
    if ($before.windows.Count -ne 1 -or $after.windows.Count -ne 1) { throw 'Expected only the selected process windows' }
    foreach ($property in 'class', 'style', 'exStyle', 'ownerHwnd', 'rect', 'isIconic', 'isWindowVisible', 'zOrderIndex', 'dpi', 'monitorId') {
        if ($null -eq $before.windows[0].PSObject.Properties[$property]) { throw "Missing required snapshot field: $property" }
    }
    if ($before.windows[0].rect.left -eq $after.windows[0].rect.left) { throw 'TW-0_MOVE_NOT_OBSERVED: move did not change the recorded rect' }
    if ($before.windows[0].zOrderIndex -eq $after.windows[0].zOrderIndex) { throw 'TW-0_ZORDER_NOT_OBSERVED: z-order change was not recorded' }
    Write-Output 'PASS: TW-0 snapshot records required native facts and move transition'
} finally {
    Remove-Item -LiteralPath $beforePath, $afterPath -Force -ErrorAction SilentlyContinue
}
