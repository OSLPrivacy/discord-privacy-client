[CmdletBinding()]
param(
  [Parameter(Mandatory)]
  [string] $OutputPath
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;

public static class Task5130CensusNative {
  public delegate bool EnumWindowsProc(IntPtr hwnd, IntPtr state);

  [StructLayout(LayoutKind.Sequential)]
  public struct Rect {
    public int Left;
    public int Top;
    public int Right;
    public int Bottom;
  }

  [DllImport("user32.dll")]
  public static extern bool EnumWindows(EnumWindowsProc callback, IntPtr state);

  [DllImport("user32.dll")]
  public static extern bool IsWindowVisible(IntPtr hwnd);

  [DllImport("user32.dll")]
  public static extern IntPtr GetForegroundWindow();

  [DllImport("user32.dll")]
  public static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint processId);

  [DllImport("user32.dll")]
  public static extern bool GetWindowRect(IntPtr hwnd, out Rect rect);

  [DllImport("user32.dll", CharSet = CharSet.Unicode)]
  public static extern int GetWindowText(IntPtr hwnd, StringBuilder value, int capacity);

  [DllImport("user32.dll", CharSet = CharSet.Unicode)]
  public static extern int GetClassName(IntPtr hwnd, StringBuilder value, int capacity);

  public static object[] VisibleBrowserWindows(uint[] processIds) {
    var expected = new HashSet<uint>(processIds);
    var result = new List<object>();
    EnumWindows((hwnd, ignored) => {
      uint processId;
      GetWindowThreadProcessId(hwnd, out processId);
      if (!expected.Contains(processId) || !IsWindowVisible(hwnd)) return true;
      var title = new StringBuilder(512);
      var className = new StringBuilder(256);
      GetWindowText(hwnd, title, title.Capacity);
      GetClassName(hwnd, className, className.Capacity);
      Rect rect;
      if (!GetWindowRect(hwnd, out rect)) return true;
      result.Add(new object[] {
        hwnd.ToInt64(), processId, title.ToString(), className.ToString(),
        rect.Left, rect.Top, rect.Right, rect.Bottom
      });
      return true;
    }, IntPtr.Zero);
    return result.ToArray();
  }
}
'@

function Get-PatternValue([Windows.Automation.AutomationElement] $Element) {
  try {
    $pattern = $Element.GetCurrentPattern([Windows.Automation.ValuePattern]::Pattern)
    [string]$pattern.Current.Value
  } catch {
    ''
  }
}

function Get-RuntimeIdHash([Windows.Automation.AutomationElement] $Element) {
  $runtimeId = [string]::Join(',', $Element.GetRuntimeId())
  Get-StringHash $runtimeId
}

function Get-StringHash([string] $Value) {
  $bytes = [Text.Encoding]::UTF8.GetBytes($Value)
  $sha = [Security.Cryptography.SHA256]::Create()
  try {
    ([BitConverter]::ToString($sha.ComputeHash($bytes))).Replace('-', '').ToLowerInvariant()
  } finally {
    $sha.Dispose()
  }
}

function Get-BrowserProfileBinding([Diagnostics.Process] $Process) {
  $cim = Get-CimInstance Win32_Process -Filter "ProcessId = $($Process.Id)"
  $commandLine = [string]$cim.CommandLine
  $profilePath = ''
  $strategy = ''
  if ($commandLine -match '(?i)(?:--user-data-dir=|--user-data-dir\s+|--profile-directory=|--profile-directory\s+|(?:^|\s)-profile\s+)(?:"([^"]+)"|(\S+))') {
    $profilePath = if ($matches[1]) { $matches[1] } else { $matches[2] }
    $strategy = 'process-command-line'
  } elseif ($Process.ProcessName -ieq 'firefox') {
    $profilesIni = Join-Path $env:APPDATA 'Mozilla\Firefox\profiles.ini'
    $default = @(Get-Content -LiteralPath $profilesIni | Where-Object { $_ -match '^Default=Profiles/' } | Select-Object -Last 1)
    if ($default.Count -eq 1) {
      $relative = $default[0].Substring('Default='.Length).Replace('/', '\')
      $profilePath = Join-Path (Split-Path -Parent $profilesIni) $relative
      $strategy = 'firefox-install-default'
    }
  } elseif ($Process.ProcessName -ieq 'msedge') {
    $profilePath = Join-Path $env:LOCALAPPDATA 'Microsoft\Edge\User Data\Default'
    $strategy = 'edge-default'
  }
  if ([string]::IsNullOrWhiteSpace($profilePath)) {
    throw "browser profile is unavailable for process $($Process.Id)"
  }
  $fullPath = [IO.Path]::GetFullPath($profilePath)
  $marker = Join-Path $fullPath 'compatibility.ini'
  $markerHash = if (Test-Path -LiteralPath $marker -PathType Leaf) {
    (Get-FileHash -Algorithm SHA256 -LiteralPath $marker).Hash.ToLowerInvariant()
  } else {
    Get-StringHash $fullPath.ToLowerInvariant()
  }
  [ordered]@{
    profileId = Split-Path -Leaf $fullPath
    profilePathSha256 = Get-StringHash $fullPath.ToLowerInvariant()
    profileMarkerSha256 = $markerHash
    bindingStrategy = $strategy
  }
}

function Get-LiveChannel([Windows.Automation.AutomationElement[]] $Elements) {
  $names = @($Elements | ForEach-Object { [string]$_.Current.Name })
  if (@($names | Where-Object { $_ -match '(?i)community chats|community chat' }).Count -gt 0) {
    return 'community'
  }
  if (@($names | Where-Object { $_ -match '(?i)chat members|group members|members of this chat' }).Count -gt 0) {
    return 'group'
  }
  if (@($names | Where-Object { $_ -match '(?i)^view profile$|^profile$' }).Count -gt 0) {
    return 'direct-message'
  }
  'unclassified'
}

function Get-ExecutableTrust([Diagnostics.Process] $Process) {
  $path = [string]$Process.Path
  $signature = Get-AuthenticodeSignature -LiteralPath $path
  [ordered]@{
    executablePath = $path
    executableSha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $path).Hash.ToLowerInvariant()
    signer = if ($signature.SignerCertificate) { [string]$signature.SignerCertificate.Subject } else { '' }
    signatureStatus = ([string]$signature.Status).ToLowerInvariant()
    version = [string]$Process.MainModule.FileVersionInfo.FileVersion
  }
}

$browserProcesses = @(Get-Process firefox,msedge -ErrorAction SilentlyContinue)
$browserProcessIds = [uint32[]]@($browserProcesses | ForEach-Object { [uint32]$_.Id })
$windows = @([Task5130CensusNative]::VisibleBrowserWindows($browserProcessIds))
$foreground = [Task5130CensusNative]::GetForegroundWindow().ToInt64()
$windowRecords = @()
$observations = @()

foreach ($window in $windows) {
  $values = [object[]]$window
  $hwnd = [int64]$values[0]
  $processId = [uint32]$values[1]
  $process = @($browserProcesses | Where-Object { [uint32]$_.Id -eq $processId } | Select-Object -First 1)
  if ($process.Count -ne 1) { continue }

  $record = [ordered]@{
    hwnd = $hwnd
    processId = $processId
    processName = [string]$process[0].ProcessName
    title = [string]$values[2]
    windowClass = [string]$values[3]
    foreground = ($hwnd -eq $foreground)
    bounds = [ordered]@{
      left = [int]$values[4]
      top = [int]$values[5]
      right = [int]$values[6]
      bottom = [int]$values[7]
    }
  }
  $windowRecords += $record

  try {
    $root = [Windows.Automation.AutomationElement]::FromHandle([IntPtr]$hwnd)
    $elements = @($root.FindAll(
      [Windows.Automation.TreeScope]::Descendants,
      [Windows.Automation.Condition]::TrueCondition
    ))
  } catch {
    continue
  }

  $urlCandidates = @($elements | ForEach-Object { Get-PatternValue $_ } |
    Where-Object { $_ -match '^https://(www\.)?messenger\.com(?:/|$)' } |
    Sort-Object -Unique)
  if ($urlCandidates.Count -ne 1) { continue }

  $composers = @($elements | Where-Object {
    -not $_.Current.IsOffscreen -and
    $_.Current.ControlType -eq [Windows.Automation.ControlType]::Edit -and
    $_.Current.Name -match '(?i)^(message|message .+|aa)$' -and
    $_.Current.Name -notmatch '(?i)search'
  })
  $channel = Get-LiveChannel $elements
  foreach ($composer in $composers) {
    $bounds = $composer.Current.BoundingRectangle
    $value = Get-PatternValue $composer
    $state = if ($value -ceq 'Task 5130 benign composer probe') {
      'ordinary-unprotected-probe'
    } elseif ([string]::IsNullOrEmpty($value)) {
      'ordinary-empty'
    } else {
      'ordinary-draft'
    }
    $observations += [ordered]@{
      key = "https://www.messenger.com/$channel/$state"
      origin = 'https://www.messenger.com'
      channel = $channel
      composerState = $state
      currentValueLength = $value.Length
      hwnd = $hwnd
      hwndGeneration = [uint64]$process[0].StartTime.ToUniversalTime().Ticks
      processId = $processId
      browser = [string]$process[0].ProcessName
      browserTrust = Get-ExecutableTrust $process[0]
      browserProfile = Get-BrowserProfileBinding $process[0]
      uiaRuntimeIdSha256 = Get-RuntimeIdHash $composer
      uiaBounds = [ordered]@{
        left = [int][Math]::Round($bounds.Left)
        top = [int][Math]::Round($bounds.Top)
        right = [int][Math]::Round($bounds.Right)
        bottom = [int][Math]::Round($bounds.Bottom)
      }
    }
  }
}

$operatingSystem = Get-CimInstance Win32_OperatingSystem
$census = [ordered]@{
  schema = 'osl-messenger-live-census-v1'
  observedAtUtc = [DateTime]::UtcNow.ToString('o')
  source = 'windows-powershell-uia-interactive-session'
  independence = 'created-before-and-without-reading-composer-contract-or-manifests'
  windowsVersion = [string]$operatingSystem.Version
  windowsBuild = [string]$operatingSystem.BuildNumber
  tasklistProcessCount = $browserProcesses.Count
  visibleBrowserWindowCount = $windowRecords.Count
  visibleBrowserWindows = $windowRecords
  observedComposerCount = $observations.Count
  installedChannels = @($observations | ForEach-Object { $_.channel } | Sort-Object -Unique)
  observedComposers = $observations
}

$fullOutputPath = [IO.Path]::GetFullPath($OutputPath)
$parent = Split-Path -Parent $fullOutputPath
[void](New-Item -ItemType Directory -Path $parent -Force)
$census | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath $fullOutputPath -Encoding UTF8
$census | ConvertTo-Json -Depth 12 -Compress
