[CmdletBinding()]
param(
  [Parameter(Mandatory)] [string] $CensusPath,
  [Parameter(Mandatory)] [string] $ContractPath,
  [Parameter(Mandatory)] [string] $ShippingReceiptPath,
  [Parameter(Mandatory)] [string] $OutputDirectory
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
Add-Type -AssemblyName System.Drawing
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;

public static class Task5130CaptureNative {
  [DllImport("user32.dll")]
  public static extern bool SetForegroundWindow(IntPtr hwnd);

  [DllImport("user32.dll")]
  public static extern IntPtr GetForegroundWindow();

  [DllImport("user32.dll")]
  public static extern bool IsWindow(IntPtr hwnd);

  [DllImport("user32.dll")]
  public static extern bool IsWindowVisible(IntPtr hwnd);
}
'@

function Stop-Capture([string] $Message) {
  [Console]::Error.WriteLine("TASK5130_CAPTURE_REFUSED missing carrier state before diffing: $Message")
  exit 1
}

function Read-Json([string] $Path, [string] $Label) {
  try {
    Get-Content -LiteralPath $Path -Raw | ConvertFrom-Json
  } catch {
    Stop-Capture "$Label is unavailable or malformed: $Path"
  }
}

function Get-HashBytes([byte[]] $Bytes) {
  $sha = [Security.Cryptography.SHA256]::Create()
  try {
    ([BitConverter]::ToString($sha.ComputeHash($Bytes))).Replace('-', '').ToLowerInvariant()
  } finally {
    $sha.Dispose()
  }
}

function Get-FileHashLower([string] $Path) {
  (Get-FileHash -Algorithm SHA256 -LiteralPath $Path).Hash.ToLowerInvariant()
}

function Get-PatternValue([Windows.Automation.AutomationElement] $Element) {
  try {
    [string]$Element.GetCurrentPattern([Windows.Automation.ValuePattern]::Pattern).Current.Value
  } catch {
    ''
  }
}

function Get-RuntimeIdHash([Windows.Automation.AutomationElement] $Element) {
  $value = [string]::Join(',', $Element.GetRuntimeId())
  Get-HashBytes ([Text.Encoding]::UTF8.GetBytes($value))
}

function Get-KeyString($Key) {
  "$($Key.origin)|$($Key.channel)|$($Key.composerState)"
}

function Get-BitmapRgb([Drawing.Bitmap] $Bitmap) {
  $bytes = [Collections.Generic.List[byte]]::new($Bitmap.Width * $Bitmap.Height * 3)
  for ($y = 0; $y -lt $Bitmap.Height; $y++) {
    for ($x = 0; $x -lt $Bitmap.Width; $x++) {
      $colour = $Bitmap.GetPixel($x, $y)
      $bytes.Add($colour.R)
      $bytes.Add($colour.G)
      $bytes.Add($colour.B)
    }
  }
  $bytes.ToArray()
}

function Get-ColourFacts([byte[]] $Rgb, [int] $Width, [int] $Height, [int] $Seam) {
  $all = [Collections.Generic.HashSet[int]]::new()
  $roi = [Collections.Generic.HashSet[int]]::new()
  $seamBytes = [Collections.Generic.List[byte]]::new()
  for ($y = 0; $y -lt $Height; $y++) {
    for ($x = 0; $x -lt $Width; $x++) {
      $offset = ($y * $Width + $x) * 3
      $packed = ([int]$Rgb[$offset] -shl 16) -bor ([int]$Rgb[$offset + 1] -shl 8) -bor [int]$Rgb[$offset + 2]
      [void]$all.Add($packed)
      if ($x -lt $Seam -or $x -ge ($Width - $Seam) -or $y -lt $Seam -or $y -ge ($Height - $Seam)) {
        $seamBytes.Add($Rgb[$offset])
        $seamBytes.Add($Rgb[$offset + 1])
        $seamBytes.Add($Rgb[$offset + 2])
      } else {
        [void]$roi.Add($packed)
      }
    }
  }
  [ordered]@{
    distinctRgbColours = $all.Count
    roiDistinctRgbColours = $roi.Count
    seamRingSha256 = Get-HashBytes $seamBytes.ToArray()
  }
}

$census = Read-Json $CensusPath 'independent live census'
$contract = Read-Json $ContractPath 'shipped composer contract'
$receipt = Read-Json $ShippingReceiptPath 'shipping integration receipt'

if ($census.schema -cne 'osl-messenger-live-census-v1' -or
    $census.source -cne 'windows-powershell-uia-interactive-session' -or
    $census.independence -cne 'created-before-and-without-reading-composer-contract-or-manifests') {
  Stop-Capture 'independent Windows live census receipt'
}
if ($contract.schema -cne 'osl-messenger-composer-contract-v1' -or
    $contract.origin -cne 'https://www.messenger.com' -or
    $contract.benignProbeText -cne 'Task 5130 benign composer probe' -or
    [int]$contract.captureCountPerKey -ne 5 -or [int]$contract.seamRingPx -ne 4) {
  Stop-Capture 'shipped composer contract constants changed'
}
if ($receipt.schema -cne 'osl-messenger-shipping-live-receipt-v1' -or
    -not $receipt.shippingIntegration.integrationEnabled) {
  Stop-Capture 'shipping integration marker receipt is absent or disabled'
}

$shippingProcess = @(Get-Process -Id ([int]$receipt.shippingProcessId) -ErrorAction SilentlyContinue)
if ($shippingProcess.Count -ne 1 -or $shippingProcess[0].Path -cne [string]$receipt.shippingExecutablePath) {
  Stop-Capture 'shipping Windows release process is absent'
}
$shippingSignature = Get-AuthenticodeSignature -LiteralPath $shippingProcess[0].Path
if ($shippingSignature.Status -ne [Management.Automation.SignatureStatus]::Valid -or
    (Get-FileHashLower $shippingProcess[0].Path) -cne [string]$receipt.shippingIntegration.executableSha256 -or
    [string]$shippingSignature.SignerCertificate.Subject -cne [string]$receipt.shippingIntegration.signer) {
  Stop-Capture 'shipping Windows release signature/build identity'
}

$expected = @{}
foreach ($key in @($contract.keys)) {
  $name = Get-KeyString $key
  if ($expected.ContainsKey($name)) { Stop-Capture "duplicate contract key $name" }
  $expected[$name] = $key
}
$observed = @{}
foreach ($composer in @($census.observedComposers)) {
  $name = Get-KeyString $composer
  if ($observed.ContainsKey($name)) { Stop-Capture "duplicate live census key $name" }
  $observed[$name] = $composer
}
$expectedNames = @($expected.Keys | Sort-Object)
$observedNames = @($observed.Keys | Sort-Object)
if ([string]::Join("`n", $expectedNames) -cne [string]::Join("`n", $observedNames)) {
  $missing = @($expectedNames | Where-Object { -not $observed.ContainsKey($_) } | Select-Object -First 1)
  Stop-Capture "live census is missing $(if ($missing.Count) { $missing[0] } else { 'an exact contract key' })"
}

$entryMap = @{}
foreach ($entry in @($receipt.entries)) {
  $name = Get-KeyString $entry.key
  if ($entryMap.ContainsKey($name)) { Stop-Capture "duplicate shipping receipt key $name" }
  $entryMap[$name] = $entry
}
if ([string]::Join("`n", @($entryMap.Keys | Sort-Object)) -cne [string]::Join("`n", $expectedNames)) {
  Stop-Capture 'shipping receipt is missing an exact carrier state'
}

$tasklist = & (Join-Path $env:windir 'System32\tasklist.exe') /FO CSV /NH
if ($LASTEXITCODE -ne 0) { Stop-Capture 'tasklist.exe observation failed' }
$output = [IO.Path]::GetFullPath($OutputDirectory)
[void](New-Item -ItemType Directory -Path $output -Force)
if (@(Get-ChildItem -LiteralPath $output -Force).Count -ne 0) {
  Stop-Capture "output directory must be empty: $output"
}

foreach ($name in $expectedNames) {
  $composerReceipt = $observed[$name]
  $entry = $entryMap[$name]
  if ([string]::IsNullOrWhiteSpace([string]$entry.review.captureAuthor) -or
      [string]::IsNullOrWhiteSpace([string]$entry.review.reviewer) -or
      [string]$entry.review.captureAuthor -ceq [string]$entry.review.reviewer -or
      [string]$entry.review.signatureHex -notmatch '^[0-9a-fA-F]{128}$') {
    Stop-Capture "$name independent 5136 review receipt"
  }
  $hwnd = [IntPtr][int64]$composerReceipt.hwnd
  if (-not [Task5130CaptureNative]::IsWindow($hwnd) -or -not [Task5130CaptureNative]::IsWindowVisible($hwnd)) {
    Stop-Capture "$name verified browser HWND disappeared"
  }
  $browserProcess = @(Get-Process -Id ([int]$composerReceipt.processId) -ErrorAction SilentlyContinue)
  if ($browserProcess.Count -ne 1 -or $tasklist -notmatch [Regex]::Escape($browserProcess[0].ProcessName + '.exe')) {
    Stop-Capture "$name tasklist browser process binding"
  }
  $browserSignature = Get-AuthenticodeSignature -LiteralPath $browserProcess[0].Path
  if ($browserSignature.Status -ne [Management.Automation.SignatureStatus]::Valid -or
      (Get-FileHashLower $browserProcess[0].Path) -cne [string]$composerReceipt.browserTrust.executableSha256) {
    Stop-Capture "$name signed browser identity changed"
  }

  $root = [Windows.Automation.AutomationElement]::FromHandle($hwnd)
  $elements = @($root.FindAll([Windows.Automation.TreeScope]::Descendants, [Windows.Automation.Condition]::TrueCondition))
  $originMatches = @($elements | ForEach-Object { Get-PatternValue $_ } | Where-Object {
    $_ -match '^https://(www\.)?messenger\.com(?:/|$)'
  } | Sort-Object -Unique)
  if ($originMatches.Count -ne 1) { Stop-Capture "$name messenger.com origin is absent or ambiguous" }
  $composerMatches = @($elements | Where-Object {
    -not $_.Current.IsOffscreen -and (Get-RuntimeIdHash $_) -ceq [string]$composerReceipt.uiaRuntimeIdSha256
  })
  if ($composerMatches.Count -ne 1) { Stop-Capture "$name exact UIA composer is absent or ambiguous" }
  $composer = $composerMatches[0]
  if ((Get-PatternValue $composer) -cne [string]$contract.benignProbeText) {
    Stop-Capture "$name exact benign probe is not carrier-visible"
  }
  if (-not [Task5130CaptureNative]::SetForegroundWindow($hwnd)) {
    Stop-Capture "$name browser window could not be foregrounded"
  }
  Start-Sleep -Milliseconds 250
  if ([Task5130CaptureNative]::GetForegroundWindow() -ne $hwnd) {
    Stop-Capture "$name browser HWND is not foreground"
  }

  $bounds = $composer.Current.BoundingRectangle
  $roi = [ordered]@{
    left = [int][Math]::Round($bounds.Left)
    top = [int][Math]::Round($bounds.Top)
    right = [int][Math]::Round($bounds.Right)
    bottom = [int][Math]::Round($bounds.Bottom)
  }
  if ($roi.left -ne [int]$composerReceipt.uiaBounds.left -or $roi.top -ne [int]$composerReceipt.uiaBounds.top -or
      $roi.right -ne [int]$composerReceipt.uiaBounds.right -or $roi.bottom -ne [int]$composerReceipt.uiaBounds.bottom) {
    Stop-Capture "$name UIA composer ROI changed after census"
  }
  $capturedBounds = [ordered]@{
    left = $roi.left - 4; top = $roi.top - 4; right = $roi.right + 4; bottom = $roi.bottom + 4
  }
  $width = $capturedBounds.right - $capturedBounds.left
  $height = $capturedBounds.bottom - $capturedBounds.top
  if ($width -le 8 -or $height -le 8 -or $width * $height -gt 8388608) {
    Stop-Capture "$name bounded composer ROI is invalid"
  }

  $stem = "messenger-$($entry.key.channel)-$($entry.key.composerState)"
  $captures = @()
  $seamHash = ''
  $floor = [int]::MaxValue
  for ($ordinal = 1; $ordinal -le 5; $ordinal++) {
    if ([Task5130CaptureNative]::GetForegroundWindow() -ne $hwnd -or (Get-PatternValue $composer) -cne [string]$contract.benignProbeText) {
      Stop-Capture "$name carrier state starved before capture $ordinal"
    }
    $pngName = "$stem-capture-$ordinal.png"
    $pngPath = Join-Path $output $pngName
    $bitmap = [Drawing.Bitmap]::new($width, $height, [Drawing.Imaging.PixelFormat]::Format24bppRgb)
    $graphics = [Drawing.Graphics]::FromImage($bitmap)
    try {
      $graphics.CopyFromScreen($capturedBounds.left, $capturedBounds.top, 0, 0, $bitmap.Size)
      $rgb = Get-BitmapRgb $bitmap
      $facts = Get-ColourFacts $rgb $width $height 4
      if ($facts.distinctRgbColours -lt 32 -or $facts.roiDistinctRgbColours -le 2) {
        Stop-Capture "$name capture $ordinal colour floor all=$($facts.distinctRgbColours) roi=$($facts.roiDistinctRgbColours)"
      }
      if ($seamHash -and $seamHash -cne $facts.seamRingSha256) {
        Stop-Capture "$name untouched seam ring changed across five captures"
      }
      $seamHash = $facts.seamRingSha256
      $floor = [Math]::Min($floor, [int]$facts.distinctRgbColours)
      $bitmap.Save($pngPath, [Drawing.Imaging.ImageFormat]::Png)
    } finally {
      $graphics.Dispose()
      $bitmap.Dispose()
    }
    $captures += [ordered]@{
      ordinal = $ordinal; pngPath = $pngName; pngSha256 = Get-FileHashLower $pngPath
      roi = $roi; capturedBounds = $capturedBounds; seamRingPx = 4
      seamRingSha256 = $facts.seamRingSha256
      distinctRgbColours = $facts.distinctRgbColours
      roiDistinctRgbColours = $facts.roiDistinctRgbColours
    }
    Start-Sleep -Milliseconds 100
  }

  $manifest = [ordered]@{
    schema = 'osl-messenger-composer-reference-v1'; key = $entry.key
    benignProbeText = [string]$contract.benignProbeText
    sourceKind = 'live-carrier-windows-release'
    captureMethod = 'windows-powershell-uia-copyfromscreen'
    uiaProvider = 'Windows UI Automation'
    observationTools = @('tasklist.exe', 'Windows PowerShell', 'CopyFromScreen')
    routeKind = 'live-carrier'; catalogueOnly = $false; hiddenOverlay = $false; testRoute = $false
    fixtureSpecificDistinctColourFloor = $floor
    browser = [ordered]@{
      browserName = [string]$composerReceipt.browser
      executableSha256 = [string]$composerReceipt.browserTrust.executableSha256
      signer = [string]$composerReceipt.browserTrust.signer
      signatureStatus = 'valid'
      profileId = [string]$composerReceipt.browserProfile.profileId
      profilePath = "sha256:$($composerReceipt.browserProfile.profilePathSha256)"
      independentlySignedIn = [bool]$entry.independentlySignedIn
      carrierAccountId = [string]$entry.carrierAccountId
      hwnd = [uint64]$composerReceipt.hwnd
      hwndGeneration = [uint64]$composerReceipt.hwndGeneration
      foreground = $true; visible = $true; occluded = $false; origin = 'https://www.messenger.com'
    }
    shippingIntegration = [ordered]@{
      buildKind = 'shipping-windows-release'
      executableName = [string]$receipt.shippingIntegration.executableName
      executableSha256 = [string]$receipt.shippingIntegration.executableSha256
      signer = [string]$receipt.shippingIntegration.signer
      signatureStatus = 'valid'; integrationEnabled = $true
      uniqueMarker = [string]$entry.uniqueMarker
      carrierVisibleBefore = [string]$entry.carrierVisibleBefore
      carrierVisibleAfter = [string]$entry.carrierVisibleAfter
    }
    review = $entry.review
    captures = $captures
  }
  $manifest | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath (Join-Path $output "$stem.manifest.json") -Encoding UTF8
}

Write-Output "TASK5130_CAPTURE_OK keys=$($expected.Count) captures=$($expected.Count * 5) origin=https://www.messenger.com"
