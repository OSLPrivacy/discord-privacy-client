<#
.SYNOPSIS
  Builds, drives, or verifies one fail-closed C4 shipping evidence run.

.DESCRIPTION
  This script deliberately has no QA-shell path, F12 gesture, self-test request,
  or `send_native_discord_qa_atomic_text` invocation.

  `BuildShipping` compiles the ordinary frontend and desktop feature only.
  `DriveApprovedSend` is the sole live action. It is unreachable without the
  explicit owner-confirmation switch, an exact conversation name, and an
  owner-supplied plaintext file. It populates the real protected composer and
  invokes the real `#prepare-protected` button through UI Automation.
  `VerifyEvidence` is read-only and can be run on any host with Python.

  The live action is intentionally not exercised by repository self-tests.
#>

[CmdletBinding()]
param(
  [Parameter(Mandatory = $true)]
  [ValidateSet('BuildShipping', 'DriveApprovedSend', 'VerifyEvidence')]
  [string]$Action,

  [string]$RepoRoot = (Split-Path -Parent (Split-Path -Parent $PSScriptRoot)),

  [string]$EvidenceRoot,

  [string]$ExpectedConversation,

  [string]$OslExePath,

  [string]$PlaintextPath,

  [switch]$ConfirmOwnerApprovedSend,

  [string]$PythonCommand = 'python3'
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$script:FrontendCommand = 'npm --prefix apps/osl-hub-ui run build'
$script:CargoCommand = 'osl-cargo -C apps/osl-hub build --release --features desktop --target x86_64-pc-windows-msvc'
$script:SuccessStatus = 'Sent privately through OSL. Discord received only the private-message marker.'
$script:Verifier = Join-Path $PSScriptRoot 'c4-shipping-evidence.py'
$script:EmptySha256 = 'e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855'

function Get-UnixMs {
  return [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
}

function Get-Sha256([string]$Path) {
  return (Get-FileHash -LiteralPath $Path -Algorithm SHA256 -ErrorAction Stop).Hash.ToLowerInvariant()
}

function Get-TextSha256([string]$Value) {
  $sha = [Security.Cryptography.SHA256]::Create()
  try {
    $bytes = [Text.Encoding]::UTF8.GetBytes($Value)
    return ([BitConverter]::ToString($sha.ComputeHash($bytes))).Replace('-', '').ToLowerInvariant()
  } finally {
    $sha.Dispose()
  }
}

function Write-AtomicJson([string]$Path, [object]$Value) {
  $directory = Split-Path -Parent $Path
  [IO.Directory]::CreateDirectory($directory) | Out-Null
  $temporary = Join-Path $directory ('.' + [IO.Path]::GetFileName($Path) + '.' + [Guid]::NewGuid().ToString('N') + '.tmp')
  try {
    $json = $Value | ConvertTo-Json -Depth 12
    [IO.File]::WriteAllText($temporary, $json, [Text.UTF8Encoding]::new($false))
    Move-Item -LiteralPath $temporary -Destination $Path -Force
  } finally {
    if ([IO.File]::Exists($temporary)) {
      Remove-Item -LiteralPath $temporary -Force
    }
  }
}

function Invoke-Checked([string]$Program, [string[]]$Arguments) {
  & $Program @Arguments
  if ($LASTEXITCODE -ne 0) {
    throw "$Program exited with code $LASTEXITCODE."
  }
}

function Assert-NoQaBuildEnvironment {
  foreach ($name in @('VITE_OSL_DISCORD_QA_SHELL', 'CARGO_FEATURE_DISCORD_QA_SHELL')) {
    if ([Environment]::GetEnvironmentVariable($name)) {
      throw "Refusing a shipping build while $name is set."
    }
  }
}

function Resolve-ShippingExecutable([string]$Requested) {
  if (-not [string]::IsNullOrWhiteSpace($Requested)) {
    $resolved = [IO.Path]::GetFullPath($Requested)
  } else {
    $resolved = Join-Path $RepoRoot 'apps\osl-hub\target\x86_64-pc-windows-msvc\release\osl-privacy-hub.exe'
  }
  if (-not [IO.File]::Exists($resolved)) {
    throw 'Exact shipping executable is absent.'
  }
  Invoke-Checked $PythonCommand @($script:Verifier, '--check-executable', $resolved) | Out-Null
  return $resolved
}

function Build-ShippingDesktop {
  Assert-NoQaBuildEnvironment
  if ([string]::IsNullOrWhiteSpace($EvidenceRoot)) {
    throw 'BuildShipping requires a new evidence directory.'
  }
  $evidence = [IO.Path]::GetFullPath($EvidenceRoot)
  if ([IO.Directory]::Exists($evidence) -and
      [IO.Directory]::EnumerateFileSystemEntries($evidence).GetEnumerator().MoveNext()) {
    throw 'BuildShipping evidence directory must be new or empty.'
  }
  [IO.Directory]::CreateDirectory($evidence) | Out-Null
  Push-Location $RepoRoot
  try {
    Invoke-Checked 'npm' @('--prefix', 'apps/osl-hub-ui', 'run', 'build')
    # Exactly one feature, desktop. Never wrap osl-cargo in flock.
    Invoke-Checked 'osl-cargo' @(
      '-C', 'apps/osl-hub',
      'build', '--release',
      '--features', 'desktop',
      '--target', 'x86_64-pc-windows-msvc'
    )
  } finally {
    Pop-Location
  }
  $exact = Resolve-ShippingExecutable $OslExePath
  $receipt = [ordered]@{
    schema = 'osl-c4-shipping-build-v1'
    observedAtUnixMs = Get-UnixMs
    executablePath = $exact
    executableSha256 = Get-Sha256 $exact
    frontendCommand = $script:FrontendCommand
    cargoCommand = $script:CargoCommand
    cargoFeatures = @('desktop')
    qaShell = $false
  }
  $receiptPath = Join-Path $evidence 'build.json'
  Write-AtomicJson $receiptPath $receipt
  $receipt | ConvertTo-Json -Depth 4
}

function Assert-LiveDriveApproval {
  if ([Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT) {
    throw 'DriveApprovedSend is Windows-only.'
  }
  if (-not $ConfirmOwnerApprovedSend) {
    throw 'Live Discord send requires -ConfirmOwnerApprovedSend.'
  }
  if ([string]::IsNullOrWhiteSpace($ExpectedConversation) -or $ExpectedConversation.Length -gt 128) {
    throw 'Live Discord send requires one bounded exact conversation name.'
  }
  if ($ExpectedConversation.IndexOfAny([char[]](0..31)) -ge 0) {
    throw 'Conversation name contains a control character.'
  }
  if ([string]::IsNullOrWhiteSpace($PlaintextPath) -or -not [IO.File]::Exists($PlaintextPath)) {
    throw 'Live Discord send requires an owner-supplied plaintext file.'
  }
  if ([string]::IsNullOrWhiteSpace($EvidenceRoot)) {
    throw 'Live Discord send requires the BuildShipping evidence directory.'
  }
  $fullEvidence = [IO.Path]::GetFullPath($EvidenceRoot)
  $entries = if ([IO.Directory]::Exists($fullEvidence)) {
    @([IO.Directory]::EnumerateFileSystemEntries($fullEvidence))
  } else {
    @()
  }
  if ($entries.Count -ne 1 -or [IO.Path]::GetFileName($entries[0]) -cne 'build.json') {
    throw 'DriveApprovedSend requires exactly the fresh BuildShipping receipt.'
  }
}

function Initialize-Uia {
  Add-Type -AssemblyName UIAutomationClient
  Add-Type -AssemblyName UIAutomationTypes
  Add-Type -AssemblyName System.Drawing
}

function Get-UiaCondition([object]$Property, [object]$Value) {
  return [Windows.Automation.PropertyCondition]::new($Property, $Value)
}

function Get-UniqueElement(
  [Windows.Automation.AutomationElement]$Root,
  [Windows.Automation.TreeScope]$Scope,
  [Windows.Automation.Condition]$Condition,
  [string]$Label
) {
  $matches = $Root.FindAll($Scope, $Condition)
  if ($matches.Count -ne 1) {
    throw "$Label must resolve exactly once; found $($matches.Count)."
  }
  return $matches.Item(0)
}

function Get-UniqueAutomationId(
  [Windows.Automation.AutomationElement]$Root,
  [string]$AutomationId,
  [object]$ExpectedControlType
) {
  $element = Get-UniqueElement $Root ([Windows.Automation.TreeScope]::Descendants) `
    (Get-UiaCondition ([Windows.Automation.AutomationElement]::AutomationIdProperty) $AutomationId) `
    "AutomationId $AutomationId"
  if ($element.Current.ControlType -ne $ExpectedControlType) {
    throw "AutomationId $AutomationId has the wrong control type."
  }
  return $element
}

function Get-UiaValue([Windows.Automation.AutomationElement]$Element) {
  $pattern = $null
  if (-not $Element.TryGetCurrentPattern([Windows.Automation.ValuePattern]::Pattern, [ref]$pattern)) {
    throw 'Required ValuePattern is unavailable.'
  }
  return [string]([Windows.Automation.ValuePattern]$pattern).Current.Value
}

function Set-UiaValue([Windows.Automation.AutomationElement]$Element, [string]$Value) {
  $pattern = $null
  if (-not $Element.TryGetCurrentPattern([Windows.Automation.ValuePattern]::Pattern, [ref]$pattern)) {
    throw 'Required writable ValuePattern is unavailable.'
  }
  ([Windows.Automation.ValuePattern]$pattern).SetValue($Value)
  if ((Get-UiaValue $Element) -cne $Value) {
    throw 'Protected draft UI did not retain the exact owner-supplied text.'
  }
}

function Invoke-UiaButton([Windows.Automation.AutomationElement]$Element) {
  $pattern = $null
  if (-not $Element.Current.IsEnabled) {
    throw 'Production Send button is disabled.'
  }
  if (-not $Element.TryGetCurrentPattern([Windows.Automation.InvokePattern]::Pattern, [ref]$pattern)) {
    throw 'Production Send button has no InvokePattern.'
  }
  ([Windows.Automation.InvokePattern]$pattern).Invoke()
}

function Get-ExactProcess([string]$Path) {
  $matches = @(Get-Process -ErrorAction Stop | Where-Object {
    try { [IO.Path]::GetFullPath($_.Path) -ceq $Path } catch { $false }
  })
  if ($matches.Count -ne 1) {
    throw "Expected exactly one process for $Path; found $($matches.Count)."
  }
  return $matches[0]
}

function Get-ExactTopLevelWindow([int]$ProcessId, [string]$Name) {
  $root = [Windows.Automation.AutomationElement]::RootElement
  $pidCondition = Get-UiaCondition ([Windows.Automation.AutomationElement]::ProcessIdProperty) $ProcessId
  $nameCondition = Get-UiaCondition ([Windows.Automation.AutomationElement]::NameProperty) $Name
  $condition = [Windows.Automation.AndCondition]::new($pidCondition, $nameCondition)
  return Get-UniqueElement $root ([Windows.Automation.TreeScope]::Children) $condition $Name
}

function Get-DiscordSubject {
  $roots = @(Get-Process -Name Discord -ErrorAction Stop | Where-Object {
    $_.MainWindowHandle -ne 0
  })
  if ($roots.Count -ne 1) {
    throw "Expected exactly one visible Discord root; found $($roots.Count)."
  }
  $process = $roots[0]
  $signature = Get-AuthenticodeSignature -FilePath $process.Path
  if ([string]$signature.Status -cne 'Valid' -or
      $null -eq $signature.SignerCertificate -or
      $signature.SignerCertificate.Subject -notmatch 'Discord') {
    throw 'Discord executable signature is not valid and Discord-authored.'
  }
  $root = [Windows.Automation.AutomationElement]::FromHandle($process.MainWindowHandle)
  if ($null -eq $root) {
    throw 'Discord UI Automation root is unavailable.'
  }
  return [pscustomobject]@{
    Process = $process
    Root = $root
    ExeSha256 = Get-Sha256 $process.Path
    Hwnd = [long]$process.MainWindowHandle
  }
}

function Get-DiscordComposer(
  [Windows.Automation.AutomationElement]$Root,
  [string]$Target
) {
  $edits = $Root.FindAll(
    [Windows.Automation.TreeScope]::Descendants,
    (Get-UiaCondition ([Windows.Automation.AutomationElement]::ControlTypeProperty) `
      ([Windows.Automation.ControlType]::Edit))
  )
  $matches = @()
  foreach ($edit in $edits) {
    $name = [string]$edit.Current.Name
    if ($name -ceq "Message @$Target" -or $name -ceq "Message $Target") {
      $matches += $edit
    }
  }
  if ($matches.Count -ne 1) {
    throw "Named Discord composer must resolve exactly once; found $($matches.Count)."
  }
  return $matches[0]
}

function Get-DiscordSnapshot([object]$Discord, [string]$Target) {
  $nameMatches = $Discord.Root.FindAll(
    [Windows.Automation.TreeScope]::Descendants,
    (Get-UiaCondition ([Windows.Automation.AutomationElement]::NameProperty) $Target)
  )
  $visibleNames = @($nameMatches | Where-Object {
    $rect = $_.Current.BoundingRectangle
    -not $_.Current.IsOffscreen -and $rect.Width -gt 0 -and $rect.Height -gt 0
  })
  if ($visibleNames.Count -ne 1) {
    throw "Named conversation must be uniquely visible; found $($visibleNames.Count)."
  }

  $rootRect = $Discord.Root.Current.BoundingRectangle
  $lists = $Discord.Root.FindAll(
    [Windows.Automation.TreeScope]::Descendants,
    (Get-UiaCondition ([Windows.Automation.AutomationElement]::ControlTypeProperty) `
      ([Windows.Automation.ControlType]::List))
  )
  $candidates = @()
  $itemCondition = Get-UiaCondition ([Windows.Automation.AutomationElement]::ControlTypeProperty) `
    ([Windows.Automation.ControlType]::ListItem)
  foreach ($list in $lists) {
    $rect = $list.Current.BoundingRectangle
    $items = $list.FindAll([Windows.Automation.TreeScope]::Children, $itemCondition)
    if (-not $list.Current.IsOffscreen -and
        $rect.Width -ge ($rootRect.Width * 0.45) -and
        $rect.Height -ge ($rootRect.Height * 0.25) -and
        $items.Count -gt 0) {
      $candidates += [pscustomobject]@{ Element = $list; Items = $items }
    }
  }
  if ($candidates.Count -ne 1) {
    throw "Discord transcript must resolve structurally exactly once; found $($candidates.Count)."
  }

  $rows = @()
  foreach ($item in $candidates[0].Items) {
    $identity = [string]$item.Current.Name
    if ([string]::IsNullOrWhiteSpace($identity)) {
      throw 'Discord exposed an unnamed message row; row identity is unmeasurable.'
    }
    $rows += [pscustomobject]@{
      Element = $item
      IdentitySha256 = Get-TextSha256 $identity
    }
  }
  $bindingMaterial = '{0}|{1}|{2}|{3}' -f
    $Discord.ExeSha256, $Discord.Process.Id, $Discord.Hwnd, $Target
  return [pscustomobject]@{
    ObservedAtUnixMs = Get-UnixMs
    NamedConversationMatches = $visibleNames.Count
    TranscriptMatches = $candidates.Count
    RowCount = $rows.Count
    TargetBindingSha256 = Get-TextSha256 $bindingMaterial
    Rows = $rows
    Composer = Get-DiscordComposer $Discord.Root $Target
  }
}

function Find-ExactCarrierInRow([object]$Row, [string]$Carrier) {
  $matches = 0
  if ([string]$Row.Element.Current.Name -ceq $Carrier) {
    $matches++
  }
  $named = $Row.Element.FindAll(
    [Windows.Automation.TreeScope]::Descendants,
    (Get-UiaCondition ([Windows.Automation.AutomationElement]::NameProperty) $Carrier)
  )
  $matches += $named.Count
  return $matches
}

function Save-DiscordScreenshot([object]$Discord, [string]$Path) {
  $rect = $Discord.Root.Current.BoundingRectangle
  if ($rect.Width -lt 1 -or $rect.Height -lt 1) {
    throw 'Discord screenshot geometry is invalid.'
  }
  $foreground = [Windows.Automation.AutomationElement]::FocusedElement
  $bitmap = [Drawing.Bitmap]::new([int]$rect.Width, [int]$rect.Height)
  try {
    $graphics = [Drawing.Graphics]::FromImage($bitmap)
    try {
      $graphics.CopyFromScreen(
        [int]$rect.X, [int]$rect.Y, 0, 0,
        [Drawing.Size]::new([int]$rect.Width, [int]$rect.Height)
      )
    } finally {
      $graphics.Dispose()
    }
    $bitmap.Save($Path, [Drawing.Imaging.ImageFormat]::Png)
  } finally {
    $bitmap.Dispose()
  }
  if ([Windows.Automation.AutomationElement]::FocusedElement.Current.NativeWindowHandle -ne
      $foreground.Current.NativeWindowHandle) {
    throw 'Screenshot capture changed the focused window.'
  }
}

function Drive-ApprovedShippingSend {
  Assert-LiveDriveApproval
  Assert-NoQaBuildEnvironment
  Initialize-Uia
  $evidence = [IO.Path]::GetFullPath($EvidenceRoot)
  $buildReceiptPath = Join-Path $evidence 'build.json'
  $buildBytes = [IO.File]::ReadAllBytes($buildReceiptPath)
  if ($buildBytes.Length -le 0 -or $buildBytes.Length -gt 65536) {
    throw 'Shipping build receipt is unbounded.'
  }
  try {
    $buildReceipt = [Text.Encoding]::UTF8.GetString($buildBytes) | ConvertFrom-Json
  } catch {
    throw 'Shipping build receipt is invalid JSON.'
  }
  if ([string]$buildReceipt.schema -cne 'osl-c4-shipping-build-v1' -or
      [bool]$buildReceipt.qaShell -or
      @($buildReceipt.cargoFeatures).Count -ne 1 -or
      [string]$buildReceipt.cargoFeatures[0] -cne 'desktop' -or
      [string]$buildReceipt.frontendCommand -cne $script:FrontendCommand -or
      [string]$buildReceipt.cargoCommand -cne $script:CargoCommand) {
    throw 'Shipping build receipt does not prove exact desktop-only features.'
  }
  if (-not [string]::IsNullOrWhiteSpace($OslExePath) -and
      [IO.Path]::GetFullPath($OslExePath) -cne [IO.Path]::GetFullPath([string]$buildReceipt.executablePath)) {
    throw 'Requested executable differs from the shipping build receipt.'
  }
  $exactExe = Resolve-ShippingExecutable ([string]$buildReceipt.executablePath)
  $exeSha = Get-Sha256 $exactExe
  if ($exeSha -cne [string]$buildReceipt.executableSha256) {
    throw 'Shipping executable changed after BuildShipping.'
  }
  $plaintext = [IO.File]::ReadAllText([IO.Path]::GetFullPath($PlaintextPath), [Text.Encoding]::UTF8)
  if ([string]::IsNullOrWhiteSpace($plaintext) -or
      [Text.Encoding]::UTF8.GetByteCount($plaintext) -gt 16KB) {
    throw 'Owner-supplied plaintext is empty or exceeds the protected-draft bound.'
  }

  $runId = [Guid]::NewGuid().ToString()
  $runStart = Get-UnixMs
  if ([long]$buildReceipt.observedAtUnixMs -le 0 -or
      [long]$buildReceipt.observedAtUnixMs -gt $runStart) {
    throw 'Shipping build receipt timestamp is invalid for this run.'
  }
  $process = Start-Process -FilePath $exactExe -PassThru

  $deadline = [DateTime]::UtcNow.AddSeconds(30)
  do {
    Start-Sleep -Milliseconds 250
    try {
      $exactProcess = Get-ExactProcess $exactExe
      $overlayRoot = Get-ExactTopLevelWindow $exactProcess.Id 'OSL private composer'
      break
    } catch {
      if ([DateTime]::UtcNow -ge $deadline) { throw }
    }
  } while ($true)
  if ($exactProcess.Id -ne $process.Id) {
    throw 'Shipping launch resolved to an older or single-instance process.'
  }
  $processStarted = [DateTimeOffset]$exactProcess.StartTime
  $processStartedUnixMs = $processStarted.ToUnixTimeMilliseconds()
  if ($processStartedUnixMs -lt ($runStart - 5000)) {
    throw 'Launched process start predates this run.'
  }

  $draft = Get-UniqueAutomationId $overlayRoot 'protected-draft' ([Windows.Automation.ControlType]::Edit)
  $send = Get-UniqueAutomationId $overlayRoot 'prepare-protected' ([Windows.Automation.ControlType]::Button)
  $status = Get-UniqueAutomationId $overlayRoot 'overlay-status' ([Windows.Automation.ControlType]::Text)
  $discord = Get-DiscordSubject
  $before = Get-DiscordSnapshot $discord $ExpectedConversation
  if ((Get-UiaValue $before.Composer).Length -ne 0) {
    throw 'Discord composer must be empty before the owner-approved send.'
  }

  Set-UiaValue $draft $plaintext
  Invoke-UiaButton $send

  # The production Invoke returns before the async renderer path completes. Poll
  # Discord's real composer and retain the longest exact value seen before it is
  # consumed. A shipping `Sent` status is impossible unless native readback also
  # proved that same complete value exact immediately before its one send action.
  $carrier = $null
  $carrierObservedAt = 0L
  $commandObservedAt = 0L
  $postObservedAt = 0L
  $deadline = [DateTime]::UtcNow.AddSeconds(30)
  do {
    Start-Sleep -Milliseconds 10
    $value = Get-UiaValue $before.Composer
    if ($value.Length -gt 0 -and ($null -eq $carrier -or $value.Length -ge $carrier.Length)) {
      $carrier = $value
      $carrierObservedAt = Get-UnixMs
    }
    $rendererStatus = [string]$status.Current.Name
    if ($rendererStatus -ceq $script:SuccessStatus) {
      $commandObservedAt = Get-UnixMs
      if ((Get-UiaValue $draft).Length -ne 0) {
        throw 'Shipping success status appeared while the protected draft remained.'
      }
      if ((Get-UiaValue $before.Composer).Length -ne 0) {
        throw 'Shipping success status appeared while Discord composer was nonempty.'
      }
      $postObservedAt = Get-UnixMs
      break
    }
    if ([DateTime]::UtcNow -ge $deadline) {
      throw 'Shipping renderer never produced its exact production success gate.'
    }
  } while ($true)
  if ([string]::IsNullOrEmpty($carrier) -or $carrierObservedAt -le 0) {
    throw 'No exact pre-Enter Discord composer value was observed.'
  }

  $after = Get-DiscordSnapshot $discord $ExpectedConversation
  # Counted multiset diff: identical old messages are legal, while geometry is
  # deliberately excluded because appending one row can move every visible row.
  $beforeIds = @{}
  foreach ($row in $before.Rows) {
    if (-not $beforeIds.ContainsKey($row.IdentitySha256)) {
      $beforeIds[$row.IdentitySha256] = 0
    }
    $beforeIds[$row.IdentitySha256]++
  }
  $newRows = @()
  foreach ($row in $after.Rows) {
    if ($beforeIds.ContainsKey($row.IdentitySha256) -and
        $beforeIds[$row.IdentitySha256] -gt 0) {
      $beforeIds[$row.IdentitySha256]--
    } else {
      $newRows += $row
    }
  }
  if ($newRows.Count -ne 1 -or ($after.RowCount - $before.RowCount) -ne 1) {
    throw "Expected exactly one new Discord row; identities=$($newRows.Count), delta=$($after.RowCount - $before.RowCount)."
  }
  $carrierMatches = Find-ExactCarrierInRow $newRows[0] $carrier
  if ($carrierMatches -ne 1) {
    throw "New Discord row must contain the exact carrier once; found $carrierMatches."
  }
  $newRowRect = $newRows[0].Element.Current.BoundingRectangle
  if ($newRows[0].Element.Current.IsOffscreen -or
      $newRowRect.Width -lt 1 -or $newRowRect.Height -lt 1) {
    throw 'The one new Discord row is not visibly screenshotable.'
  }

  $screenshotPath = Join-Path $evidence 'after.png'
  $discord.Root.SetFocus()
  Start-Sleep -Milliseconds 100
  if ([Windows.Automation.AutomationElement]::FocusedElement.Current.ProcessId -ne
      $discord.Process.Id) {
    throw 'Discord could not be focused for the evidence screenshot.'
  }
  Save-DiscordScreenshot $discord $screenshotPath
  $screenshotAt = Get-UnixMs
  $runEnd = Get-UnixMs
  $binding = $before.TargetBindingSha256
  if ($after.TargetBindingSha256 -cne $binding) {
    throw 'Named Discord target changed during the send.'
  }

  $bundle = [ordered]@{
    schema = 'osl-c4-shipping-evidence-v1'
    runId = $runId
    runStartUnixMs = $runStart
    runEndUnixMs = $runEnd
    targetConversation = $ExpectedConversation
    build = [ordered]@{
      frontendCommand = $script:FrontendCommand
      cargoCommand = $script:CargoCommand
      cargoFeatures = @('desktop')
      qaShell = $false
      observedAtUnixMs = [long]$buildReceipt.observedAtUnixMs
      executableSha256 = [string]$buildReceipt.executableSha256
    }
    executable = [ordered]@{
      path = $exactExe
      sha256 = $exeSha
      processId = $exactProcess.Id
      processStartedAtUnixMs = $processStartedUnixMs
    }
    commandReceipt = [ordered]@{
      runId = $runId
      executableSha256 = $exeSha
      targetConversation = $ExpectedConversation
      observedAtUnixMs = $commandObservedAt
      source = 'production-overlay-ui'
      receiptAuthority = 'shipping-renderer-success-gate'
      uiControlAutomationId = 'prepare-protected'
      backendCommand = 'send_native_discord_overlay_carrier'
      status = 'sent'
      placed = $true
      enterSent = $true
      qaShell = $false
      rendererStatus = $script:SuccessStatus
    }
    preEnterReadback = [ordered]@{
      runId = $runId
      executableSha256 = $exeSha
      targetConversation = $ExpectedConversation
      observedAtUnixMs = $carrierObservedAt
      authority = 'native-pre-enter-exact-readback'
      relation = 'rawExact'
      readCount = 1
      utf8Bytes = [Text.Encoding]::UTF8.GetByteCount($carrier)
      composerTextSha256 = Get-TextSha256 $carrier
      exact = $true
    }
    postEnterComposer = [ordered]@{
      runId = $runId
      executableSha256 = $exeSha
      targetConversation = $ExpectedConversation
      observedAtUnixMs = $postObservedAt
      readCount = 1
      utf8Bytes = 0
      composerTextSha256 = $script:EmptySha256
      empty = $true
    }
    conversationRows = [ordered]@{
      before = [ordered]@{
        runId = $runId
        executableSha256 = $exeSha
        targetConversation = $ExpectedConversation
        observedAtUnixMs = $before.ObservedAtUnixMs
        namedConversationMatches = $before.NamedConversationMatches
        transcriptMatches = $before.TranscriptMatches
        readCount = 1
        rowCount = $before.RowCount
        targetBindingSha256 = $binding
      }
      after = [ordered]@{
        runId = $runId
        executableSha256 = $exeSha
        targetConversation = $ExpectedConversation
        observedAtUnixMs = $after.ObservedAtUnixMs
        namedConversationMatches = $after.NamedConversationMatches
        transcriptMatches = $after.TranscriptMatches
        readCount = 1
        rowCount = $after.RowCount
        targetBindingSha256 = $binding
      }
      newRows = @([ordered]@{
        targetConversation = $ExpectedConversation
        targetBindingSha256 = $binding
        carrierTextSha256 = Get-TextSha256 $carrier
        rowIdentitySha256 = $newRows[0].IdentitySha256
        matchCount = $carrierMatches
      })
    }
    screenshot = [ordered]@{
      path = 'after.png'
      sha256 = Get-Sha256 $screenshotPath
      observedAtUnixMs = $screenshotAt
      targetConversation = $ExpectedConversation
      namedConversationMatches = $after.NamedConversationMatches
      newRowMatches = $carrierMatches
    }
  }
  $bundlePath = Join-Path $evidence 'bundle.json'
  Write-AtomicJson $bundlePath $bundle
  Invoke-Checked $PythonCommand @(
    $script:Verifier,
    '--bundle', $bundlePath,
    '--expected-target', $ExpectedConversation,
    '--expected-run-id', $runId,
    '--not-before-unix-ms', ([string]$runStart)
  )
}

function Verify-Evidence {
  if ([string]::IsNullOrWhiteSpace($EvidenceRoot) -or
      [string]::IsNullOrWhiteSpace($ExpectedConversation)) {
    throw 'VerifyEvidence requires EvidenceRoot and ExpectedConversation.'
  }
  $bundlePath = Join-Path ([IO.Path]::GetFullPath($EvidenceRoot)) 'bundle.json'
  try {
    $bundle = Get-Content -LiteralPath $bundlePath -Raw -Encoding UTF8 | ConvertFrom-Json
  } catch {
    throw 'Evidence bundle is absent or invalid JSON.'
  }
  Invoke-Checked $PythonCommand @(
    $script:Verifier,
    '--bundle', $bundlePath,
    '--expected-target', $ExpectedConversation,
    '--expected-run-id', ([string]$bundle.runId),
    '--not-before-unix-ms', ([string]$bundle.runStartUnixMs)
  )
}

switch ($Action) {
  'BuildShipping' { Build-ShippingDesktop }
  'DriveApprovedSend' { Drive-ApprovedShippingSend }
  'VerifyEvidence' { Verify-Evidence }
}
