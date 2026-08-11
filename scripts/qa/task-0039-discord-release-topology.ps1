[CmdletBinding()]
param(
  [Parameter(Mandatory)]
  [string] $OutputDirectory,
  [switch] $NoCapture,
  [ValidateSet('None', 'ReuseAccount', 'ReuseStore')]
  [string] $Mutation = 'None'
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

Add-Type -AssemblyName System.Drawing
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;

public static class Task0039Native {
  public delegate bool EnumWindowsProc(IntPtr hwnd, IntPtr state);

  [StructLayout(LayoutKind.Sequential)]
  public struct Rect {
    public int Left;
    public int Top;
    public int Right;
    public int Bottom;
  }

  [DllImport("user32.dll")]
  public static extern bool GetWindowRect(IntPtr hwnd, out Rect rect);

  [DllImport("user32.dll")]
  public static extern bool EnumWindows(EnumWindowsProc callback, IntPtr state);

  [DllImport("user32.dll")]
  public static extern bool IsWindowVisible(IntPtr hwnd);

  [DllImport("user32.dll")]
  public static extern bool ShowWindowAsync(IntPtr hwnd, int command);

  [DllImport("user32.dll")]
  public static extern bool SetForegroundWindow(IntPtr hwnd);

  [DllImport("user32.dll")]
  public static extern bool SetCursorPos(int x, int y);

  [DllImport("user32.dll")]
  public static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint processId);

  [DllImport("user32.dll", CharSet = CharSet.Unicode)]
  public static extern int GetClassName(IntPtr hwnd, StringBuilder value, int capacity);

  [DllImport("user32.dll")]
  public static extern bool PrintWindow(IntPtr hwnd, IntPtr hdc, uint flags);

  public static IntPtr[] VisibleChromiumWindows(uint[] processIds) {
    var expected = new HashSet<uint>(processIds);
    var result = new List<IntPtr>();
    EnumWindows((hwnd, ignored) => {
      uint processId;
      GetWindowThreadProcessId(hwnd, out processId);
      var className = new StringBuilder(256);
      GetClassName(hwnd, className, className.Capacity);
      if (expected.Contains(processId) && IsWindowVisible(hwnd) &&
          className.ToString() == "Chrome_WidgetWin_1") result.Add(hwnd);
      return true;
    }, IntPtr.Zero);
    return result.ToArray();
  }
}
'@

function Stop-Topology([string] $Message) {
  [Console]::Error.WriteLine("TASK0039_TOPOLOGY_FAIL $Message")
  exit 1
}

function Get-DirectoryBytes([string] $Path) {
  $total = [int64]0
  foreach ($file in Get-ChildItem -LiteralPath $Path -File -Recurse -Force -ErrorAction SilentlyContinue) {
    $total += [int64]$file.Length
  }
  $total
}

function Get-RootDocument([Windows.Automation.AutomationElement] $Root) {
  $condition = [Windows.Automation.PropertyCondition]::new(
    [Windows.Automation.AutomationElement]::ControlTypeProperty,
    [Windows.Automation.ControlType]::Document
  )
  $documents = @($Root.FindAll([Windows.Automation.TreeScope]::Descendants, $condition) |
    Where-Object { $_.Current.AutomationId -ceq 'RootWebArea' })
  if ($documents.Count -ne 1) { return $null }
  $documents[0]
}

function Get-ExactAccountElement(
  [Windows.Automation.AutomationElement] $Root,
  [string] $ExpectedAccountId
) {
  $buttonCondition = [Windows.Automation.PropertyCondition]::new(
    [Windows.Automation.AutomationElement]::NameProperty,
    'Manage profile and status'
  )
  $button = $Root.FindFirst([Windows.Automation.TreeScope]::Descendants, $buttonCondition)
  if (-not $button) { return $null }
  $buttonBounds = $button.Current.BoundingRectangle
  $all = $Root.FindAll(
    [Windows.Automation.TreeScope]::Descendants,
    [Windows.Automation.Condition]::TrueCondition
  )
  $matches = @($all | Where-Object {
    $_.Current.ControlType -eq [Windows.Automation.ControlType]::Text -and
    $_.Current.Name -ceq $ExpectedAccountId -and
    $_.Current.BoundingRectangle.Left -ge ($buttonBounds.Left + 30) -and
    $_.Current.BoundingRectangle.Left -le ($buttonBounds.Right + 4) -and
    $_.Current.BoundingRectangle.Top -ge ($buttonBounds.Top - 4) -and
    $_.Current.BoundingRectangle.Top -le ($buttonBounds.Bottom + 16)
  })
  if ($matches.Count -ne 1) { return $null }
  [pscustomobject]@{ Button = $button; Account = $matches[0] }
}

function Save-BoundedWindowCapture(
  [Windows.Automation.AutomationElement] $Document,
  [string] $Path
) {
  $rect = $Document.Current.BoundingRectangle
  $width = [int][Math]::Round($rect.Width)
  $height = [int][Math]::Round($rect.Height)
  if ($width -lt 360 -or $height -lt 90 -or $width -gt 4096 -or $height -gt 2160) {
    throw "window bounds outside capture limit: ${width}x${height}"
  }

  # The account panel is the lower-left 360x68 pixels. This deliberately omits
  # conversations and message content while retaining the channel's own account.
  $cropWidth = [Math]::Min(360, $width)
  $cropHeight = 68
  $crop = [Drawing.Bitmap]::new($cropWidth, $cropHeight)
  $cropGraphics = [Drawing.Graphics]::FromImage($crop)
  try {
    $cropGraphics.CopyFromScreen(
      [int][Math]::Round($rect.Left),
      [int][Math]::Round($rect.Bottom) - $cropHeight,
      0,
      0,
      $crop.Size
    )
    $crop.Save($Path, [Drawing.Imaging.ImageFormat]::Png)
  } finally {
    $cropGraphics.Dispose()
    $crop.Dispose()
  }
}

$specs = @(
  [pscustomobject]@{
    Channel = 'Stable'; ProcessName = 'Discord'; StoreName = 'discord';
    AccountId = 'oslpriv1'; DisplayName = 'OSL Privacy test account';
    HostPattern = '^https://(discordapp|discord)\.com/'
  },
  [pscustomobject]@{
    Channel = 'PTB'; ProcessName = 'DiscordPTB'; StoreName = 'discordptb';
    AccountId = 'osltest2'; DisplayName = 'OSL TEST 2';
    HostPattern = '^https://ptb\.discord\.com/'
  },
  [pscustomobject]@{
    Channel = 'Canary'; ProcessName = 'DiscordCanary'; StoreName = 'discordcanary';
    AccountId = 'oslprivacy'; DisplayName = 'OSL';
    HostPattern = '^https://canary\.discord\.com/'
  }
)

$output = [IO.Path]::GetFullPath($OutputDirectory)
[void](New-Item -ItemType Directory -Path $output -Force)
$windowShell = New-Object -ComObject WScript.Shell
$roster = @()

foreach ($spec in $specs) {
  $processes = @(Get-Process -Name $spec.ProcessName -ErrorAction SilentlyContinue)
  if ($processes.Count -eq 0) {
    Stop-Topology "$($spec.Channel) release-channel session is starved: process $($spec.ProcessName) is absent"
  }
  $pids = [uint32[]]@($processes | ForEach-Object { [uint32]$_.Id })
  $candidateWindows = @([Task0039Native]::VisibleChromiumWindows($pids))
  $matchingWindows = @()
  foreach ($candidate in $candidateWindows) {
    $candidateRoot = [Windows.Automation.AutomationElement]::FromHandle($candidate)
    $candidateDocument = Get-RootDocument $candidateRoot
    if (-not $candidateDocument) { continue }
    $candidatePattern = $candidateDocument.GetCurrentPattern([Windows.Automation.ValuePattern]::Pattern)
    $candidateUrl = [string]$candidatePattern.Current.Value
    if ($candidateUrl -match $spec.HostPattern) {
      $matchingWindows += [pscustomobject]@{
        Hwnd = [IntPtr]$candidate
        Root = $candidateRoot
        Document = $candidateDocument
        Url = $candidateUrl
      }
    }
  }
  if ($matchingWindows.Count -ne 1) {
    Stop-Topology "$($spec.Channel) release-channel session is starved: expected one signed-in window, observed $($matchingWindows.Count)"
  }

  [uint32]$ownerPid = 0
  [void][Task0039Native]::GetWindowThreadProcessId($matchingWindows[0].Hwnd, [ref]$ownerPid)
  $main = @($processes | Where-Object { [uint32]$_.Id -eq $ownerPid })
  if ($main.Count -ne 1) { Stop-Topology "$($spec.Channel) window owner is unavailable" }

  $expectedLocalRoot = [IO.Path]::GetFullPath((Join-Path $env:LOCALAPPDATA $spec.ProcessName))
  $processPath = [IO.Path]::GetFullPath([string]$main[0].Path)
  if (-not $processPath.StartsWith($expectedLocalRoot + [IO.Path]::DirectorySeparatorChar,
      [StringComparison]::OrdinalIgnoreCase)) {
    Stop-Topology "$($spec.Channel) process is outside its official release-channel install: $processPath"
  }

  $root = $matchingWindows[0].Root
  $document = $matchingWindows[0].Document
  $url = $matchingWindows[0].Url

  $accountElement = Get-ExactAccountElement $root $spec.AccountId
  if (-not $accountElement) {
    Stop-Topology "$($spec.Channel) exact account $($spec.AccountId) is absent from its account panel"
  }
  $displayMatches = @($root.FindAll(
    [Windows.Automation.TreeScope]::Descendants,
    [Windows.Automation.PropertyCondition]::new(
      [Windows.Automation.AutomationElement]::NameProperty,
      $spec.DisplayName
    )
  ))
  if ($displayMatches.Count -eq 0) {
    Stop-Topology "$($spec.Channel) account display name $($spec.DisplayName) is absent"
  }

  $store = [IO.Path]::GetFullPath((Join-Path $env:APPDATA $spec.StoreName))
  if (-not (Test-Path -LiteralPath $store -PathType Container)) {
    Stop-Topology "$($spec.Channel) Discord store is absent: $store"
  }
  $storeBytes = Get-DirectoryBytes $store
  if ($storeBytes -lt 1048576) {
    Stop-Topology "$($spec.Channel) Discord store is starved: $storeBytes bytes"
  }

  $oslDataPath = [IO.Path]::GetFullPath((Join-Path $env:LOCALAPPDATA (
    'OSL Privacy\qa\discord-release-channels\' + $spec.Channel.ToLowerInvariant()
  )))
  [void](New-Item -ItemType Directory -Path $oslDataPath -Force)

  $captureBase = Join-Path $output ("{0}-account" -f $spec.Channel.ToLowerInvariant())
  $capturePng = $captureBase + '.png'
  $captureJson = $captureBase + '.json'
  if (-not $NoCapture) {
    $activationProcess = @($processes | Where-Object { $_.MainWindowHandle -ne 0 } | Select-Object -First 1)
    if ($activationProcess.Count -ne 1 -or -not $windowShell.AppActivate([int]$activationProcess[0].Id)) {
      Stop-Topology "$($spec.Channel) account capture could not activate its release-channel window"
    }
    [void][Task0039Native]::ShowWindowAsync($matchingWindows[0].Hwnd, 9)
    [void][Task0039Native]::SetForegroundWindow($matchingWindows[0].Hwnd)
    $expand = $accountElement.Button.GetCurrentPattern(
      [Windows.Automation.ExpandCollapsePattern]::Pattern
    )
    if ($expand.Current.ExpandCollapseState -ne [Windows.Automation.ExpandCollapseState]::Expanded) {
      $expand.Expand()
    }
    $documentBounds = $document.Current.BoundingRectangle
    [void][Task0039Native]::SetCursorPos(
      [int][Math]::Round($documentBounds.Left) + 90,
      [int][Math]::Round($documentBounds.Bottom) - 30
    )
    Start-Sleep -Milliseconds 700
    Save-BoundedWindowCapture $document $capturePng
  }
  $bounded = [ordered]@{
    schema = 'osl.task0039.bounded-account-capture.v1'
    release_channel = $spec.Channel
    process_name = $spec.ProcessName
    account_id = [string]$accountElement.Account.Current.Name
    display_name = $spec.DisplayName
    root_origin = ([Uri]$url).GetLeftPart([UriPartial]::Authority)
    bound = 'release-channel account panel only; no message content'
    png = if ($NoCapture) { $null } else { [IO.Path]::GetFileName($capturePng) }
  }
  $bounded | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath $captureJson -Encoding UTF8

  $record = [ordered]@{
    machine = $env:COMPUTERNAME
    windows_session_id = [int]$main[0].SessionId
    release_channel = $spec.Channel
    account_id = [string]$accountElement.Account.Current.Name
    display_name = $spec.DisplayName
    process_name = $spec.ProcessName
    process_id = [int]$main[0].Id
    process_ids = @($processes | ForEach-Object { [int]$_.Id } | Sort-Object)
    process_path = $processPath
    discord_store = $store
    discord_store_bytes = $storeBytes
    osl_data_path = $oslDataPath
    bounded_capture = $captureJson
  }
  $record | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (
    Join-Path $oslDataPath 'topology-0039.json'
  ) -Encoding UTF8
  $roster += [pscustomobject]$record
}

if ($Mutation -ceq 'ReuseAccount') {
  $roster[1].account_id = $roster[0].account_id
}
if ($Mutation -ceq 'ReuseStore') {
  $roster[1].discord_store = $roster[0].discord_store
}

$accountIds = @($roster | ForEach-Object account_id | Sort-Object -Unique)
$stores = @($roster | ForEach-Object discord_store | ForEach-Object { $_.ToLowerInvariant() } | Sort-Object -Unique)
$oslPaths = @($roster | ForEach-Object osl_data_path | ForEach-Object { $_.ToLowerInvariant() } | Sort-Object -Unique)
if ($accountIds.Count -ne 3) {
  Stop-Topology "account identity reuse detected: expected 3 distinct account ids, observed $($accountIds.Count)"
}
if ($stores.Count -ne 3) {
  Stop-Topology "Discord store reuse detected: expected 3 distinct stores, observed $($stores.Count)"
}
if ($oslPaths.Count -ne 3) {
  Stop-Topology "OSL data path reuse detected: expected 3 distinct paths, observed $($oslPaths.Count)"
}

$rosterPath = Join-Path $output 'roster.json'
$roster | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $rosterPath -Encoding UTF8
foreach ($record in $roster) {
  Write-Output ("TASK0039_CHANNEL channel={0} account_id={1} process={2} store={3} osl_data={4} capture={5}" -f
    $record.release_channel, $record.account_id, $record.process_name,
    $record.discord_store, $record.osl_data_path, $record.bounded_capture)
}
Write-Output "TASK0039_TOPOLOGY_OK channels=3 distinct_accounts=3 distinct_stores=3 distinct_osl_paths=3 roster=$rosterPath"
