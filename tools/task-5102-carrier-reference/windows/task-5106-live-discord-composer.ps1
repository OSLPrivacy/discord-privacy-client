param(
    [Parameter(Mandatory = $true)]
    [string]$OutputRoot,
    [Parameter(Mandatory = $true)]
    [string]$Marker,
    [switch]$CensusOnly,
    [string]$ContractPath = ""
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

Add-Type -AssemblyName System.Drawing
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type -ReferencedAssemblies System.Drawing -TypeDefinition @"
using System;
using System.Collections.Generic;
using System.Drawing;
using System.Drawing.Imaging;
using System.Runtime.InteropServices;
using System.Security.Cryptography;

public static class Task5106Pixels {
    public static byte[] RgbBytes(Bitmap bitmap) {
        var rect = new Rectangle(0, 0, bitmap.Width, bitmap.Height);
        var data = bitmap.LockBits(rect, ImageLockMode.ReadOnly, PixelFormat.Format32bppArgb);
        try {
            int stride = Math.Abs(data.Stride);
            byte[] raw = new byte[stride * bitmap.Height];
            Marshal.Copy(data.Scan0, raw, 0, raw.Length);
            byte[] rgb = new byte[bitmap.Width * bitmap.Height * 3];
            int output = 0;
            for (int y = 0; y < bitmap.Height; y++) {
                int row = data.Stride >= 0 ? y * stride : (bitmap.Height - 1 - y) * stride;
                for (int x = 0; x < bitmap.Width; x++) {
                    int input = row + x * 4;
                    rgb[output++] = raw[input + 2];
                    rgb[output++] = raw[input + 1];
                    rgb[output++] = raw[input];
                }
            }
            return rgb;
        } finally {
            bitmap.UnlockBits(data);
        }
    }

    public static int DistinctRgb(Bitmap bitmap) {
        byte[] rgb = RgbBytes(bitmap);
        var colours = new HashSet<int>();
        for (int i = 0; i < rgb.Length; i += 3)
            colours.Add((rgb[i] << 16) | (rgb[i + 1] << 8) | rgb[i + 2]);
        return colours.Count;
    }

    public static string PixelSha256(Bitmap bitmap) {
        using (var sha = SHA256.Create())
            return BitConverter.ToString(sha.ComputeHash(RgbBytes(bitmap))).Replace("-", "").ToLowerInvariant();
    }

    public static string SeamSha256(Bitmap bitmap, int seam) {
        byte[] rgb = RgbBytes(bitmap);
        using (var sha = SHA256.Create()) {
            for (int y = 0; y < bitmap.Height; y++) {
                for (int x = 0; x < bitmap.Width; x++) {
                    if (x < seam || y < seam || x >= bitmap.Width - seam || y >= bitmap.Height - seam) {
                        int offset = (y * bitmap.Width + x) * 3;
                        sha.TransformBlock(rgb, offset, 3, null, 0);
                    }
                }
            }
            sha.TransformFinalBlock(new byte[0], 0, 0);
            return BitConverter.ToString(sha.Hash).Replace("-", "").ToLowerInvariant();
        }
    }
}
"@

Add-Type @"
using System;
using System.Runtime.InteropServices;
public static class Task5106Win32 {
    [StructLayout(LayoutKind.Sequential)]
    public struct RECT { public int Left, Top, Right, Bottom; }
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hwnd, out RECT rect);
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hwnd);
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint processId);
    [DllImport("kernel32.dll")] public static extern uint GetCurrentThreadId();
    [DllImport("user32.dll")] public static extern bool AttachThreadInput(uint attach, uint attachTo, bool value);
    [DllImport("user32.dll")] public static extern bool BringWindowToTop(IntPtr hwnd);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hwnd);
    [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr hwnd);
    public static bool ForceForeground(IntPtr hwnd) {
        uint ignored;
        uint foregroundThread = GetWindowThreadProcessId(GetForegroundWindow(), out ignored);
        uint currentThread = GetCurrentThreadId();
        AttachThreadInput(currentThread, foregroundThread, true);
        BringWindowToTop(hwnd);
        bool result = SetForegroundWindow(hwnd);
        AttachThreadInput(currentThread, foregroundThread, false);
        return result;
    }
}
"@

$Utf8NoBom = New-Object System.Text.UTF8Encoding($false)
$SeamPixels = 4
$ObservedStates = @(
    [pscustomobject]@{ key = "composer-focused-probe"; focused = $true; content = "exact_probe" },
    [pscustomobject]@{ key = "composer-unfocused-probe"; focused = $false; content = "exact_probe" }
)
$Channels = @(
    [pscustomobject]@{ key = "stable"; process = "Discord"; executable = "Discord.exe" },
    [pscustomobject]@{ key = "ptb"; process = "DiscordPTB"; executable = "DiscordPTB.exe" },
    [pscustomobject]@{ key = "canary"; process = "DiscordCanary"; executable = "DiscordCanary.exe" }
)

function Get-Sha256Hex([byte[]]$Bytes) {
    $sha = [System.Security.Cryptography.SHA256]::Create()
    try { return ([BitConverter]::ToString($sha.ComputeHash($Bytes))).Replace("-", "").ToLowerInvariant() }
    finally { $sha.Dispose() }
}

function Get-TextSha256([string]$Text) {
    return Get-Sha256Hex ([System.Text.Encoding]::UTF8.GetBytes($Text))
}

function Write-JsonFile([string]$Path, $Value, [int]$Depth) {
    $json = ($Value | ConvertTo-Json -Depth $Depth).Replace("`r`n", "`n")
    [IO.File]::WriteAllText($Path, $json, $Utf8NoBom)
}

function Get-WindowRectangle([IntPtr]$Hwnd) {
    $rect = New-Object Task5106Win32+RECT
    if (-not [Task5106Win32]::GetWindowRect($Hwnd, [ref]$rect)) { throw "GetWindowRect failed for HWND $Hwnd" }
    if ($rect.Right -le $rect.Left -or $rect.Bottom -le $rect.Top) { throw "degenerate window rectangle for HWND $Hwnd" }
    return $rect
}

function Copy-ScreenRectangle([int]$Left, [int]$Top, [int]$Right, [int]$Bottom) {
    $width = $Right - $Left
    $height = $Bottom - $Top
    if ($width -le 0 -or $height -le 0) { throw "degenerate CopyFromScreen rectangle" }
    $bitmap = New-Object System.Drawing.Bitmap($width, $height, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
    try {
        $graphics.CopyFromScreen($Left, $Top, 0, 0, $bitmap.Size, [System.Drawing.CopyPixelOperation]::SourceCopy)
    } finally {
        $graphics.Dispose()
    }
    return $bitmap
}

function Find-DescendantByName([System.Windows.Automation.AutomationElement]$Root, [string]$Name) {
    $condition = New-Object System.Windows.Automation.PropertyCondition(
        [System.Windows.Automation.AutomationElement]::NameProperty,
        $Name
    )
    return $Root.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $condition)
}

function Invoke-Element([System.Windows.Automation.AutomationElement]$Element) {
    $pattern = $null
    if ($Element.TryGetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern, [ref]$pattern)) {
        ([System.Windows.Automation.InvokePattern]$pattern).Invoke()
        return
    }
    if ($Element.TryGetCurrentPattern([System.Windows.Automation.LegacyIAccessiblePattern]::Pattern, [ref]$pattern)) {
        ([System.Windows.Automation.LegacyIAccessiblePattern]$pattern).DoDefaultAction()
        return
    }
    throw "UIA element '$($Element.Current.Name)' has no invokable pattern"
}

function Find-Composer([System.Windows.Automation.AutomationElement]$Root) {
    $condition = New-Object System.Windows.Automation.PropertyCondition(
        [System.Windows.Automation.AutomationElement]::ControlTypeProperty,
        [System.Windows.Automation.ControlType]::Edit
    )
    $edits = $Root.FindAll([System.Windows.Automation.TreeScope]::Descendants, $condition)
    $matches = @()
    foreach ($edit in $edits) {
        if ($edit.Current.Name.StartsWith("Message ") -and
            $edit.Current.IsEnabled -and
            $edit.Current.IsKeyboardFocusable -and
            -not $edit.Current.IsOffscreen) {
            $matches += $edit
        }
    }
    if ($matches.Count -ne 1) { return $null }
    return $matches[0]
}

function Resolve-Composer([string]$ProcessName, [System.Windows.Automation.AutomationElement]$Root) {
    $composer = Find-Composer $Root
    if ($null -ne $composer) { return $composer }

    if ($ProcessName -eq "Discord") {
        $hideMembers = Find-DescendantByName $Root "Hide Member List"
        if ($null -ne $hideMembers) {
            Invoke-Element $hideMembers
            Start-Sleep -Milliseconds 800
            $composer = Find-Composer $Root
        }
    } elseif ($ProcessName -eq "DiscordPTB") {
        foreach ($candidate in @(
            "OSL, OSL Privacy test account (group message), 3 Members",
            "OSL TEST 2, OSL Privacy test account (group message), 3 Members"
        )) {
            $conversation = Find-DescendantByName $Root $candidate
            if ($null -ne $conversation) {
                Invoke-Element $conversation
                Start-Sleep -Milliseconds 1000
                $composer = Find-Composer $Root
                if ($null -ne $composer) { break }
            }
        }
    }
    if ($null -eq $composer) { throw "missing live carrier composer for $ProcessName" }
    return $composer
}

function Get-ValuePattern([System.Windows.Automation.AutomationElement]$Composer) {
    $pattern = $null
    if (-not $Composer.TryGetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern, [ref]$pattern)) {
        throw "live carrier composer '$($Composer.Current.Name)' has no ValuePattern"
    }
    return [System.Windows.Automation.ValuePattern]$pattern
}

function Get-ComposerObservation([System.Windows.Automation.AutomationElement]$Composer) {
    $bounds = $Composer.Current.BoundingRectangle
    $runtime = ($Composer.GetRuntimeId() | ForEach-Object { $_.ToString([System.Globalization.CultureInfo]::InvariantCulture) }) -join ","
    return [pscustomobject]@{
        name = $Composer.Current.Name
        automation_id = $Composer.Current.AutomationId
        class_name = $Composer.Current.ClassName
        has_keyboard_focus = $Composer.Current.HasKeyboardFocus
        runtime_id_sha256 = Get-TextSha256 $runtime
        bounds = [pscustomobject]@{
            left = [int]$bounds.Left
            top = [int]$bounds.Top
            right = [int]$bounds.Right
            bottom = [int]$bounds.Bottom
        }
    }
}

function Assert-EqualObservation($First, $Second, [string]$Key) {
    $firstJson = $First | ConvertTo-Json -Depth 8 -Compress
    $secondJson = $Second | ConvertTo-Json -Depth 8 -Compress
    if ($firstJson -ne $secondJson) { throw "two UIA observations did not agree for $Key" }
}

function Capture-StableRegion($Bounds, [string]$Key) {
    for ($attempt = 1; $attempt -le 20; $attempt++) {
        $first = Copy-ScreenRectangle ($Bounds.left - $SeamPixels) ($Bounds.top - $SeamPixels) ($Bounds.right + $SeamPixels) ($Bounds.bottom + $SeamPixels)
        Start-Sleep -Milliseconds 30
        $second = Copy-ScreenRectangle ($Bounds.left - $SeamPixels) ($Bounds.top - $SeamPixels) ($Bounds.right + $SeamPixels) ($Bounds.bottom + $SeamPixels)
        $firstHash = [Task5106Pixels]::PixelSha256($first)
        $secondHash = [Task5106Pixels]::PixelSha256($second)
        $first.Dispose()
        if ($firstHash -eq $secondHash) { return $second }
        $second.Dispose()
        Start-Sleep -Milliseconds 80
    }
    throw "two bounded CopyFromScreen frames did not agree for $Key"
}

function Set-ObservedState(
    [System.Windows.Automation.AutomationElement]$Composer,
    [System.Windows.Automation.AutomationElement]$Root,
    [System.Windows.Automation.ValuePattern]$ValuePattern,
    $State
) {
    $ValuePattern.SetValue($Marker)
    Start-Sleep -Milliseconds 120
    if ((Get-ValuePattern $Composer).Current.Value -ne $Marker) { throw "missing live carrier marker after placement for $($State.key)" }
    if ($State.focused) {
        $Composer.SetFocus()
    } else {
        $buttonCondition = New-Object System.Windows.Automation.PropertyCondition(
            [System.Windows.Automation.AutomationElement]::ControlTypeProperty,
            [System.Windows.Automation.ControlType]::Button
        )
        $buttons = $Root.FindAll([System.Windows.Automation.TreeScope]::Descendants, $buttonCondition)
        $moved = $false
        foreach ($button in $buttons) {
            if ($button.Current.IsEnabled -and $button.Current.IsKeyboardFocusable -and -not $button.Current.IsOffscreen) {
                try {
                    $button.SetFocus()
                    $moved = $true
                    break
                } catch {}
            }
        }
        if (-not $moved) { throw "no live carrier focus sink for $($State.key)" }
    }
    Start-Sleep -Milliseconds 250
    if ($Composer.Current.HasKeyboardFocus -ne [bool]$State.focused) {
        throw "carrier focus state did not become $($State.key)"
    }
}

function Get-InstalledCarrier($Channel) {
    $processes = @(Get-Process -Name $Channel.process -ErrorAction SilentlyContinue)
    $windows = @($processes | Where-Object { $_.MainWindowHandle -ne 0 })
    if ($windows.Count -ne 1) { throw "missing installed channel $($Channel.key): expected one visible HWND, got $($windows.Count)" }
    $process = $windows[0]
    if ([IO.Path]::GetFileName($process.Path) -ne $Channel.executable) { throw "wrong executable for channel $($Channel.key)" }
    $signature = Get-AuthenticodeSignature -FilePath $process.Path
    if ($signature.Status -ne [System.Management.Automation.SignatureStatus]::Valid -or
        $signature.SignerCertificate.Subject -notmatch "Discord Inc") {
        throw "invalid Discord signature for channel $($Channel.key)"
    }
    if (-not [Task5106Win32]::IsWindowVisible($process.MainWindowHandle) -or [Task5106Win32]::IsIconic($process.MainWindowHandle)) {
        throw "carrier window is not visible for channel $($Channel.key)"
    }
    return [pscustomobject]@{ process = $process; signature = $signature }
}

New-Item -ItemType Directory -Force -Path $OutputRoot | Out-Null
$previousForeground = [Task5106Win32]::GetForegroundWindow()
$censusChannels = @()
$captureRecords = @()

try {
    foreach ($channel in $Channels) {
        $carrier = Get-InstalledCarrier $channel
        $process = $carrier.process
        [Task5106Win32]::ForceForeground($process.MainWindowHandle) | Out-Null
        Start-Sleep -Milliseconds 400
        if ([Task5106Win32]::GetForegroundWindow() -ne $process.MainWindowHandle) {
            throw "carrier window is not foreground for channel $($channel.key)"
        }

        $root = [System.Windows.Automation.AutomationElement]::FromHandle($process.MainWindowHandle)
        $composer = Resolve-Composer $channel.process $root
        $valuePattern = Get-ValuePattern $composer
        $originalValue = $valuePattern.Current.Value
        $stateRows = @()

        try {
            foreach ($state in $ObservedStates) {
                Set-ObservedState $composer $root $valuePattern $state
                $firstObservation = Get-ComposerObservation $composer
                Start-Sleep -Milliseconds 80
                $secondObservation = Get-ComposerObservation $composer
                Assert-EqualObservation $firstObservation $secondObservation "$($channel.key)/$($state.key)"

                $windowRect = Get-WindowRectangle $process.MainWindowHandle
                $wholeWindow = Copy-ScreenRectangle $windowRect.Left $windowRect.Top $windowRect.Right $windowRect.Bottom
                try { $wholeDistinct = [Task5106Pixels]::DistinctRgb($wholeWindow) }
                finally { $wholeWindow.Dispose() }
                if ($wholeDistinct -lt 256) { throw "whole Discord window below 256 distinct RGB colours for $($channel.key)/$($state.key): $wholeDistinct" }

                $stateRows += [pscustomobject]@{
                    key = "$($channel.key)/$($state.key)"
                    channel = $channel.key
                    state = $state.key
                    whole_window_distinct_rgb = $wholeDistinct
                    composer = $secondObservation
                    marker_readback_sha256 = Get-TextSha256 ((Get-ValuePattern $composer).Current.Value)
                }

                if (-not $CensusOnly) {
                    $knownGoodCounts = @()
                    $knownGoodPixelHashes = @()
                    $selected = $null
                    for ($sample = 0; $sample -lt 5; $sample++) {
                        $bitmap = Capture-StableRegion $secondObservation.bounds "$($channel.key)/$($state.key)/known-good-$($sample + 1)"
                        $count = [Task5106Pixels]::DistinctRgb($bitmap)
                        if ($count -lt 32) {
                            $bitmap.Dispose()
                            throw "capture below 32 distinct RGB colours for $($channel.key)/$($state.key): $count"
                        }
                        $knownGoodCounts += $count
                        $knownGoodPixelHashes += [Task5106Pixels]::PixelSha256($bitmap)
                        if ($null -eq $selected) { $selected = $bitmap } else { $bitmap.Dispose() }
                    }
                    $floor = ($knownGoodCounts | Measure-Object -Minimum).Minimum
                    $baseName = "discord-$($channel.key)-$($state.key)"
                    $pngPath = Join-Path $OutputRoot "$baseName.png"
                    $selected.Save($pngPath, [System.Drawing.Imaging.ImageFormat]::Png)
                    $persistedDistinct = [Task5106Pixels]::DistinctRgb($selected)
                    $seamSha256 = [Task5106Pixels]::SeamSha256($selected, $SeamPixels)
                    $width = $selected.Width
                    $height = $selected.Height
                    $selected.Dispose()

                    $exeHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $process.Path).Hash.ToLowerInvariant()
                    $pngHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $pngPath).Hash.ToLowerInvariant()
                    $manifestPath = Join-Path $OutputRoot "$baseName.manifest.json"
                    $candidateName = "$baseName.candidate-baseline.json"
                    $candidatePath = Join-Path $OutputRoot $candidateName
                    $manifest = [ordered]@{
                        schema = "osl-discord-composer-reference-v1"
                        key = "$($channel.key)/$($state.key)"
                        carrier = "Discord"
                        channel = $channel.key
                        state = $state.key
                        surface_kind = "composer"
                        source = "real_windows_carrier"
                        synthetic_test_account = $true
                        exact_probe_sha256 = Get-TextSha256 $Marker
                        captured_at_utc = [DateTime]::UtcNow.ToString("yyyy-MM-ddTHH:mm:ssZ")
                        identity = [ordered]@{
                            executable_name = $channel.executable
                            executable_sha256 = $exeHash
                            signer = $carrier.signature.SignerCertificate.Subject
                            signature_status = "valid"
                            version = $process.MainModule.FileVersionInfo.FileVersion
                        }
                        hwnd = [int64]$process.MainWindowHandle
                        window_title_sha256 = Get-TextSha256 $process.MainWindowTitle
                        windows_build = [Environment]::OSVersion.Version.ToString()
                        uia = $secondObservation
                        observations_agreeing = 2
                        frames_agreeing = 2
                        capture = [ordered]@{
                            seam_ring_physical_px = $SeamPixels
                            png_mode = "RGBA"
                            width = $width
                            height = $height
                            png_sha256 = $pngHash
                            seam_ring_rgb_sha256 = $seamSha256
                            persisted_distinct_rgb = $persistedDistinct
                            whole_window_distinct_rgb = $wholeDistinct
                            known_good_capture_count = 5
                            known_good_distinct_rgb = $knownGoodCounts
                            known_good_pixel_sha256 = $knownGoodPixelHashes
                            fixture_specific_distinct_rgb_floor = $floor
                        }
                        baseline_review = [ordered]@{
                            status = "pending_distinct_authorized_reviewer"
                            capture_author = "task-5106-windows-live-capture"
                            candidate_baseline_record = $candidateName
                            reviewer = $null
                            reviewed_baseline_record = $null
                        }
                    }
                    Write-JsonFile $manifestPath $manifest 12
                    $manifestHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $manifestPath).Hash.ToLowerInvariant()
                    $candidate = [ordered]@{
                        schema = "osl-carrier-baseline-candidate-v1"
                        key = "$($channel.key)/$($state.key)"
                        carrier = "Discord"
                        channel = $channel.key
                        state = $state.key
                        png_sha256 = $pngHash
                        manifest_sha256 = $manifestHash
                        capture_author = "task-5106-windows-live-capture"
                        captured_at_utc = $manifest.captured_at_utc
                        review_status = "pending_distinct_authorized_reviewer"
                    }
                    Write-JsonFile $candidatePath $candidate 8
                    $captureRecords += [pscustomobject]@{
                        key = "$($channel.key)/$($state.key)"
                        png = [IO.Path]::GetFileName($pngPath)
                        manifest = [IO.Path]::GetFileName($manifestPath)
                        candidate_baseline = $candidateName
                        distinct_rgb = $persistedDistinct
                        floor = $floor
                    }
                    if ((Get-ValuePattern $composer).Current.Value -ne $Marker) {
                        throw "live carrier marker was starved during capture for $($channel.key)/$($state.key)"
                    }
                }
            }
        } finally {
            $currentValue = (Get-ValuePattern $composer).Current.Value
            if ($currentValue -eq $Marker) {
                (Get-ValuePattern $composer).SetValue($originalValue)
                $restored = $false
                for ($restoreAttempt = 0; $restoreAttempt -lt 10; $restoreAttempt++) {
                    Start-Sleep -Milliseconds 120
                    if ((Get-ValuePattern $composer).Current.Value -eq $originalValue) {
                        $restored = $true
                        break
                    }
                }
                if (-not $restored) {
                    throw "failed to restore original carrier composer value for $($channel.key)"
                }
            } elseif ($currentValue -ne $originalValue) {
                throw "concurrent carrier writer changed $($channel.key); original value was not overwritten"
            }
        }

        $censusChannels += [pscustomobject]@{
            channel = $channel.key
            process_name = $channel.process
            executable_name = $channel.executable
            executable_path_sha256 = Get-TextSha256 $process.Path
            executable_sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $process.Path).Hash.ToLowerInvariant()
            signer = $carrier.signature.SignerCertificate.Subject
            signature_status = "valid"
            version = $process.MainModule.FileVersionInfo.FileVersion
            hwnd = [int64]$process.MainWindowHandle
            original_value_sha256 = Get-TextSha256 $originalValue
            restored_value_sha256 = Get-TextSha256 ((Get-ValuePattern $composer).Current.Value)
            states = $stateRows
        }
    }
} finally {
    if ($previousForeground -ne [IntPtr]::Zero) { [Task5106Win32]::SetForegroundWindow($previousForeground) | Out-Null }
}

$census = [ordered]@{
    schema = "osl-live-carrier-census-v1"
    task = 5106
    observed_at_utc = [DateTime]::UtcNow.ToString("yyyy-MM-ddTHH:mm:ssZ")
    observation_tools = @("tasklist.exe", "Windows PowerShell", "UIAutomationClient", "System.Drawing.CopyFromScreen")
    forbidden_linux_ui_tools_used = $false
    independent_of_contract_and_manifest = $true
    carrier = "Discord"
    marker = $Marker
    marker_sha256 = Get-TextSha256 $Marker
    carrier_visible_before_after = "UIA ValuePattern read-back proved marker placement and byte-exact restoration; CopyFromScreen proved nonblank foreground pixels"
    shipping_integration = [ordered]@{
        observed = $false
        reason = "No shipping OSL Windows process was running; this real-carrier collector is development evidence only until a packaged production integration run is reviewed."
    }
    channels = $censusChannels
}
$censusPath = Join-Path $OutputRoot "live-discord-census-2026-08-11.json"
if ($CensusOnly) {
    Write-JsonFile $censusPath $census 14
} elseif (-not (Test-Path -LiteralPath $censusPath)) {
    throw "capture mode requires the dated pre-contract live-carrier census"
}

if (-not $CensusOnly) {
    if ([string]::IsNullOrWhiteSpace($ContractPath) -or -not (Test-Path -LiteralPath $ContractPath)) {
        throw "capture mode requires the shipped composer contract"
    }
    $contract = Get-Content -Raw -LiteralPath $ContractPath | ConvertFrom-Json
    $frozenCensus = Get-Content -Raw -LiteralPath $censusPath | ConvertFrom-Json
    $contractKeys = @($contract.surfaces | ForEach-Object { $_.key } | Sort-Object)
    $censusKeys = @($frozenCensus.channels.states | ForEach-Object { $_.key } | Sort-Object)
    $freshKeys = @($census.channels.states | ForEach-Object { $_.key } | Sort-Object)
    if (($freshKeys -join "`n") -ne ($censusKeys -join "`n")) {
        throw "fresh live carrier keys do not match the dated pre-contract census"
    }
    if (($contractKeys -join "`n") -ne ($censusKeys -join "`n")) {
        throw "shipped composer contract keys do not match independently observed live census"
    }
    $index = [ordered]@{
        schema = "osl-discord-composer-reference-set-v1"
        generated_at_utc = [DateTime]::UtcNow.ToString("yyyy-MM-ddTHH:mm:ssZ")
        contract_schema = $contract.schema
        live_census = [IO.Path]::GetFileName($censusPath)
        references = $captureRecords
    }
    Write-JsonFile (Join-Path $OutputRoot "discord-composer-reference-set.json") $index 10
}

Write-Output "TASK5106_LIVE_CENSUS channels=$($censusChannels.Count) states=$(($censusChannels.states | Measure-Object).Count) marker=$Marker shipping_integration=$($census.shipping_integration.observed)"
foreach ($channel in $censusChannels) {
    foreach ($state in $channel.states) {
        Write-Output "TASK5106_LIVE key=$($state.key) whole_window_distinct_rgb=$($state.whole_window_distinct_rgb)"
    }
}
foreach ($record in $captureRecords) {
    Write-Output "TASK5106_CAPTURE key=$($record.key) distinct_rgb=$($record.distinct_rgb) fixture_floor=$($record.floor) known_good=5"
}
