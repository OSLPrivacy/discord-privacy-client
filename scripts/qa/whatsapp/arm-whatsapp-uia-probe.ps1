param(
  [Parameter(Mandatory = $true)]
  [ValidatePattern('^[a-z0-9][a-z0-9-]{7,63}$')]
  [string]$InvocationId,

  [Parameter(Mandatory = $true)]
  [ValidateRange(1, 65535)]
  [int]$SessionId
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$root = 'C:\ProgramData\OSL-QA\whatsapp-uia-structure-v1'
$invocationRoot = Join-Path $root $InvocationId
$requestPath = Join-Path $invocationRoot 'request.clixml'
$wrapperPath = Join-Path $invocationRoot 'run-interactive.ps1'
$resultPath = Join-Path $invocationRoot 'result.json'
$taskName = "OSL-QA-WhatsApp-Uia-$InvocationId"

$sessions = @()
foreach ($explorer in @(Get-CimInstance Win32_Process -Filter "Name = 'explorer.exe'")) {
  $owner = Invoke-CimMethod -InputObject $explorer -MethodName GetOwner
  if ($owner.ReturnValue -eq 0 -and $owner.User -ceq 'osltest' -and $owner.Domain) {
    $sessions += [pscustomobject]@{ Process = $explorer; Owner = $owner }
  }
}
if ($sessions.Count -ne 1 -or [int]$sessions[0].Process.SessionId -ne $SessionId) {
  throw 'exact osltest interactive session is unavailable or ambiguous'
}
$interactiveUser = "$($sessions[0].Owner.Domain)\$($sessions[0].Owner.User)"

if (Test-Path -LiteralPath $invocationRoot) {
  if (-not (Test-Path -LiteralPath $requestPath -PathType Leaf) -or
      -not (Test-Path -LiteralPath $wrapperPath -PathType Leaf)) {
    throw 'existing invocation is incomplete'
  }
  $existing = Import-Clixml -LiteralPath $requestPath
  $actualHash = (Get-FileHash -LiteralPath $wrapperPath -Algorithm SHA256).Hash.ToLowerInvariant()
  if ($existing.InvocationId -cne $InvocationId -or [int]$existing.SessionId -ne $SessionId -or
      $existing.InteractiveUser -cne $interactiveUser -or $existing.ResultPath -cne $resultPath -or
      $existing.WrapperSha256 -cne $actualHash) {
    throw 'existing invocation identity mismatch'
  }
  [pscustomobject]@{
    InvocationId = $InvocationId
    Status = if (Test-Path -LiteralPath $resultPath -PathType Leaf) { 'alreadyCompleted' } else { 'alreadyArmed' }
    TaskName = $taskName
    SessionId = $SessionId
    WrapperSha256 = $actualHash
  } | ConvertTo-Json -Compress
  exit 0
}

[void](New-Item -ItemType Directory -Path $invocationRoot -Force)
$request = [ordered]@{
  InvocationId = $InvocationId
  SessionId = $SessionId
  InteractiveUser = $interactiveUser
  ResultPath = $resultPath
}
$requestTemporary = "$requestPath.tmp"
$request | Export-Clixml -LiteralPath $requestTemporary -Depth 3
[IO.File]::Move($requestTemporary, $requestPath)

$escapedRequestPath = $requestPath.Replace("'", "''")
$wrapper = @'
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$packageName = '5319275A.WhatsAppDesktop'
$packageFamily = '5319275A.WhatsAppDesktop_cv1g1gvanyjgm'
$publisherId = 'cv1g1gvanyjgm'
$processName = 'WhatsApp.Root.exe'
$windowClass = 'WinUIDesktopWin32WindowClass'
$windowTitle = 'WhatsApp'
$maximumNodes = 512
$maximumDepth = 12
$maximumOutputBytes = 262144
$deadlineSeconds = 15
$geometryQuantum = 16.0
$request = Import-Clixml -LiteralPath '__REQUEST_PATH__'
$resultTemporary = "$($request.ResultPath).tmp"
$started = [DateTime]::UtcNow

Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type -TypeDefinition @"
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;
public static class OslQaWindowInventory {
  public delegate bool EnumWindowsProc(IntPtr hwnd, IntPtr state);
  [DllImport("user32.dll")] private static extern bool EnumWindows(EnumWindowsProc callback, IntPtr state);
  [DllImport("user32.dll")] private static extern bool IsWindowVisible(IntPtr hwnd);
  [DllImport("user32.dll", SetLastError=true)] private static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint pid);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] private static extern int GetClassName(IntPtr hwnd, StringBuilder value, int capacity);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] private static extern int GetWindowText(IntPtr hwnd, StringBuilder value, int capacity);
  public static IntPtr[] Exact(uint pid, string expectedClass, string expectedTitle) {
    var found = new List<IntPtr>();
    EnumWindows((hwnd, state) => {
      uint owner;
      GetWindowThreadProcessId(hwnd, out owner);
      if (owner != pid || !IsWindowVisible(hwnd)) return true;
      var cls = new StringBuilder(256);
      var title = new StringBuilder(256);
      GetClassName(hwnd, cls, cls.Capacity);
      GetWindowText(hwnd, title, title.Capacity);
      if (String.Equals(cls.ToString(), expectedClass, StringComparison.Ordinal) &&
          String.Equals(title.ToString(), expectedTitle, StringComparison.Ordinal)) found.Add(hwnd);
      return true;
    }, IntPtr.Zero);
    return found.ToArray();
  }
}
"@

function Get-ExactPackage {
  $packages = @(Get-AppxPackage -Name $packageName -PackageTypeFilter Main -ErrorAction SilentlyContinue)
  $matches = @($packages | Where-Object {
    $_.Name -ceq $packageName -and $_.PackageFamilyName -ceq $packageFamily -and
    $_.PublisherId -ceq $publisherId -and -not $_.IsFramework -and -not $_.IsResourcePackage -and
    $_.PackageFullName -cmatch '^5319275A[.]WhatsAppDesktop_[0-9]+[.][0-9]+[.][0-9]+[.][0-9]+_(x64|x86|arm64|neutral)__cv1g1gvanyjgm$'
  })
  if ($packages.Count -ne $matches.Count -or $matches.Count -ne 1) {
    throw 'exact WhatsApp Store package is unavailable or ambiguous'
  }
  $location = [IO.Path]::GetFullPath([string]$matches[0].InstallLocation).TrimEnd('\')
  $programFiles = [IO.Path]::GetFullPath([Environment]::GetFolderPath('ProgramFiles')).TrimEnd('\')
  if ([IO.Directory]::GetParent($location).FullName -ine "$programFiles\WindowsApps" -or
      -not $location.StartsWith("$programFiles\WindowsApps\5319275A.WhatsAppDesktop_", [StringComparison]::OrdinalIgnoreCase)) {
    throw 'WhatsApp package location is outside protected WindowsApps'
  }
  return [pscustomobject]@{ Registration = $matches[0]; Location = $location }
}

function Get-StructuralToken([string]$value) {
  if ($null -eq $value -or $value.Length -eq 0) { return $null }
  if ($value.Length -gt 64 -or $value -cnotmatch '^[A-Za-z_][A-Za-z0-9_.:-]{0,63}$') { return $null }
  if ($value -cmatch '[0-9]{4,}' -or $value -cmatch '(?i)(phone|mobile|contact|recipient|conversation|message|chat|text|email|user)') { return $null }
  return $value
}

function Get-SafeAutomationId([string]$value) {
  $token = Get-StructuralToken $value
  if (-not $token -or $token -cnotmatch '(?i)(automation|button|pane|panel|list|item|root|view|control|header|footer|nav|menu|search|compose|input|editor|toolbar|tab|dialog|flyout|scroll|grid|window|page|icon|image|attachment)') {
    return $null
  }
  return $token
}

function Get-SafeClassName([string]$value) {
  $token = Get-StructuralToken $value
  if (-not $token -or $token -cnotmatch '(?i)(window|button|pane|panel|list|item|view|control|edit|textblock|image|scroll|grid|root|content|frame|presenter|border|canvas|stack|navigation|tab|menu|toolbar)') {
    return $null
  }
  return $token
}

function Get-SafeFrameworkId([string]$value) {
  $token = Get-StructuralToken $value
  if ($token -and $token -cmatch '^(Win32|WinForm|WPF|XAML|WinUI|DirectUI|Chrome|Chromium)$') { return $token }
  return $null
}

function Get-StructuralHash([int[]]$runtimeId) {
  if ($null -eq $runtimeId -or $runtimeId.Count -eq 0 -or $runtimeId.Count -gt 32) { return $null }
  $material = [Text.Encoding]::UTF8.GetBytes("$($request.InvocationId)|$($runtimeId -join ',')")
  $hasher = [Security.Cryptography.SHA256]::Create()
  try {
    $digest = $hasher.ComputeHash($material)
    return (([BitConverter]::ToString($digest)).Replace('-', '').ToLowerInvariant()).Substring(0, 24)
  } finally {
    $hasher.Dispose()
    [Array]::Clear($material, 0, $material.Length)
  }
}

function Get-QuantizedRect($rect, $rootRect) {
  if ($rect.IsEmpty) { return $null }
  $q = $geometryQuantum
  return @(
    [int][Math]::Round(($rect.X - $rootRect.X) / $q),
    [int][Math]::Round(($rect.Y - $rootRect.Y) / $q),
    [int][Math]::Round($rect.Width / $q),
    [int][Math]::Round($rect.Height / $q)
  )
}

try {
  if ([Security.Principal.WindowsIdentity]::GetCurrent().Name -cne $request.InteractiveUser -or
      [Diagnostics.Process]::GetCurrentProcess().SessionId -ne [int]$request.SessionId) {
    throw 'interactive runner session identity mismatch'
  }
  $actualWrapperSha256 = (Get-FileHash -LiteralPath $PSCommandPath -Algorithm SHA256).Hash.ToLowerInvariant()
  if ($actualWrapperSha256 -cne $request.WrapperSha256) { throw 'interactive runner hash mismatch' }

  $package = Get-ExactPackage
  $expectedExecutable = [IO.Path]::GetFullPath((Join-Path $package.Location $processName))
  $processes = @(Get-CimInstance Win32_Process -Filter "Name = 'WhatsApp.Root.exe'" | Where-Object {
    [int]$_.SessionId -eq [int]$request.SessionId -and $_.ExecutablePath -and
    [IO.Path]::GetFullPath([string]$_.ExecutablePath).Equals($expectedExecutable, [StringComparison]::OrdinalIgnoreCase)
  })
  if ($processes.Count -ne 1) { throw 'exact WhatsApp process is unavailable or ambiguous' }
  $pidValue = [uint32]$processes[0].ProcessId
  $creationIdentity = [string]$processes[0].CreationDate
  $windows = [OslQaWindowInventory]::Exact($pidValue, $windowClass, $windowTitle)
  if ($windows.Count -ne 1) { throw 'exact WhatsApp main window is unavailable or ambiguous' }

  $rootElement = [Windows.Automation.AutomationElement]::FromHandle($windows[0])
  if ($null -eq $rootElement -or $rootElement.Current.ProcessId -ne [int]$pidValue) {
    throw 'UIA root identity mismatch'
  }
  $rootRect = $rootElement.Current.BoundingRectangle
  $walker = [Windows.Automation.TreeWalker]::RawViewWalker
  $queue = [Collections.Generic.Queue[object]]::new()
  $rootHash = Get-StructuralHash ([int[]]$rootElement.GetRuntimeId())
  if (-not $rootHash) { throw 'UIA root runtime identity unavailable' }
  $queue.Enqueue([pscustomobject]@{ Element = $rootElement; Depth = 0; ParentHash = $null })
  $nodes = [Collections.Generic.List[object]]::new()
  $truncated = $false

  while ($queue.Count -gt 0) {
    if (([DateTime]::UtcNow - $started).TotalSeconds -ge $deadlineSeconds) { throw 'UIA probe deadline exceeded' }
    if ($nodes.Count -ge $maximumNodes) { $truncated = $true; break }
    $item = $queue.Dequeue()
    $element = $item.Element
    try {
      $current = $element.Current
      if ($current.ProcessId -ne [int]$pidValue) { throw 'cross-process UIA node rejected' }
      $runtimeHash = Get-StructuralHash ([int[]]$element.GetRuntimeId())
      if (-not $runtimeHash) { throw 'UIA node runtime identity unavailable' }
      $nodes.Add([ordered]@{
        Depth = [int]$item.Depth
        RuntimeHash = $runtimeHash
        ParentHash = $item.ParentHash
        ControlType = [string]$current.ControlType.ProgrammaticName
        AutomationId = Get-SafeAutomationId ([string]$current.AutomationId)
        ClassName = Get-SafeClassName ([string]$current.ClassName)
        FrameworkId = Get-SafeFrameworkId ([string]$current.FrameworkId)
        GeometryQ16 = Get-QuantizedRect $current.BoundingRectangle $rootRect
        Enabled = [bool]$current.IsEnabled
        Offscreen = [bool]$current.IsOffscreen
      })
      if ([int]$item.Depth -lt $maximumDepth) {
        $child = $walker.GetFirstChild($element)
        while ($null -ne $child) {
          if (([DateTime]::UtcNow - $started).TotalSeconds -ge $deadlineSeconds) { throw 'UIA probe deadline exceeded' }
          $queue.Enqueue([pscustomobject]@{ Element = $child; Depth = [int]$item.Depth + 1; ParentHash = $runtimeHash })
          if ($queue.Count + $nodes.Count -gt ($maximumNodes * 2)) { $truncated = $true; break }
          $child = $walker.GetNextSibling($child)
        }
      } elseif ($null -ne $walker.GetFirstChild($element)) {
        $truncated = $true
      }
    } catch [Windows.Automation.ElementNotAvailableException] {
      throw 'UIA tree changed during bounded capture'
    }
  }

  $postProcesses = @(Get-CimInstance Win32_Process -Filter "ProcessId = $pidValue" | Where-Object {
    $_.Name -ceq $processName -and [int]$_.SessionId -eq [int]$request.SessionId -and
    [string]$_.CreationDate -ceq $creationIdentity -and $_.ExecutablePath -and
    [IO.Path]::GetFullPath([string]$_.ExecutablePath).Equals($expectedExecutable, [StringComparison]::OrdinalIgnoreCase)
  })
  $postWindows = [OslQaWindowInventory]::Exact($pidValue, $windowClass, $windowTitle)
  if ($postProcesses.Count -ne 1 -or $postWindows.Count -ne 1 -or $postWindows[0] -ne $windows[0]) {
    throw 'WhatsApp process or window identity changed during bounded capture'
  }

  $terminal = [ordered]@{
    InvocationId = $request.InvocationId
    WrapperSha256 = $request.WrapperSha256
    Terminal = $true
    Status = 'capturedStructure'
    CompletedUtc = [DateTime]::UtcNow.ToString('o')
    PackageFamily = $packageFamily
    ExactPackageVerified = $true
    ExactProcessVerified = $true
    ExactWindowVerified = $true
    SessionId = [int]$request.SessionId
    RootRuntimeHash = $rootHash
    NodeCount = $nodes.Count
    Truncated = $truncated
    Limits = [ordered]@{ MaximumNodes = $maximumNodes; MaximumDepth = $maximumDepth; DeadlineSeconds = $deadlineSeconds; GeometryQuantum = [int]$geometryQuantum }
    Nodes = $nodes
    ForegroundChanged = $false
    InputInjected = $false
    ProviderStorageRead = $false
    ContentPropertiesRead = $false
    AppLaunched = $false
    ProcessesTerminated = 0
  }
} catch {
  $terminal = [ordered]@{
    InvocationId = $request.InvocationId
    WrapperSha256 = $request.WrapperSha256
    Terminal = $true
    Status = 'failedClosed'
    CompletedUtc = [DateTime]::UtcNow.ToString('o')
    PackageFamily = $packageFamily
    ExactPackageVerified = $false
    ExactProcessVerified = $false
    ExactWindowVerified = $false
    SessionId = [int]$request.SessionId
    NodeCount = 0
    Nodes = @()
    ForegroundChanged = $false
    InputInjected = $false
    ProviderStorageRead = $false
    ContentPropertiesRead = $false
    AppLaunched = $false
    ProcessesTerminated = 0
    Error = 'whatsapp-uia-structure-probe-failed-closed'
    ExceptionType = $_.Exception.GetType().Name
  }
}

$json = $terminal | ConvertTo-Json -Depth 8 -Compress
if ([Text.Encoding]::UTF8.GetByteCount($json) -gt $maximumOutputBytes) {
  $terminal = [ordered]@{
    InvocationId = $request.InvocationId; WrapperSha256 = $request.WrapperSha256; Terminal = $true
    Status = 'failedClosed'; PackageFamily = $packageFamily; NodeCount = 0; Nodes = @()
    ForegroundChanged = $false; InputInjected = $false; ProviderStorageRead = $false
    ContentPropertiesRead = $false; AppLaunched = $false; ProcessesTerminated = 0
    Error = 'whatsapp-uia-structure-output-limit-exceeded'
  }
  $json = $terminal | ConvertTo-Json -Depth 4 -Compress
}
[IO.File]::WriteAllText($resultTemporary, $json, [Text.UTF8Encoding]::new($false))
[IO.File]::Move($resultTemporary, $request.ResultPath)
'@
$wrapper = $wrapper.Replace('__REQUEST_PATH__', $escapedRequestPath)
[IO.File]::WriteAllText($wrapperPath, $wrapper, [Text.UTF8Encoding]::new($false))
$wrapperSha256 = (Get-FileHash -LiteralPath $wrapperPath -Algorithm SHA256).Hash.ToLowerInvariant()
$request['WrapperSha256'] = $wrapperSha256
$request | Export-Clixml -LiteralPath $requestTemporary -Depth 3
[IO.File]::Delete($requestPath)
[IO.File]::Move($requestTemporary, $requestPath)

$action = New-ScheduledTaskAction -Execute 'powershell.exe' -Argument (
  '-NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "{0}"' -f $wrapperPath
)
$principal = New-ScheduledTaskPrincipal -UserId $interactiveUser -LogonType Interactive -RunLevel Limited
$settings = New-ScheduledTaskSettingsSet -ExecutionTimeLimit ([TimeSpan]::FromSeconds(30)) -MultipleInstances IgnoreNew
Register-ScheduledTask -TaskName $taskName -Action $action -Principal $principal -Settings $settings | Out-Null
Start-ScheduledTask -TaskName $taskName

[pscustomobject]@{
  InvocationId = $InvocationId
  Status = 'armed'
  TaskName = $taskName
  SessionId = $SessionId
  WrapperSha256 = $wrapperSha256
} | ConvertTo-Json -Compress
