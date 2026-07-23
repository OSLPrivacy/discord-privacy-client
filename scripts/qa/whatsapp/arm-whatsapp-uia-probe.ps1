param(
  [Parameter(Mandatory = $true)]
  [ValidatePattern('^[a-z0-9][a-z0-9-]{7,63}$')]
  [string]$InvocationId,

  [Parameter(Mandatory = $true)]
  [ValidateRange(1, 65535)]
  [int]$SessionId,

  [ValidateSet('observe', 'gracefulRelaunch', 'verifiedRelaunch')]
  [string]$Mode = 'observe'
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
      $existing.Mode -cne $Mode -or
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
  Mode = $Mode
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
$appUserModelId = '5319275A.WhatsAppDesktop_cv1g1gvanyjgm!App'
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
  [DllImport("user32.dll")] private static extern bool EnumChildWindows(IntPtr parent, EnumWindowsProc callback, IntPtr state);
  [DllImport("user32.dll")] private static extern bool IsWindowVisible(IntPtr hwnd);
  [DllImport("user32.dll", SetLastError=true)] private static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint pid);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] private static extern int GetClassName(IntPtr hwnd, StringBuilder value, int capacity);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] private static extern int GetWindowText(IntPtr hwnd, StringBuilder value, int capacity);
  [DllImport("oleacc.dll")]
  private static extern int AccessibleObjectFromWindow(IntPtr hwnd, uint objectId, ref Guid interfaceId,
    [MarshalAs(UnmanagedType.Interface)] out object accessible);
  [DllImport("user32.dll", EntryPoint="SystemParametersInfoW", SetLastError=true)]
  private static extern bool SystemParametersInfoGet(uint action, uint parameter, out bool value, uint flags);
  [DllImport("user32.dll", EntryPoint="SystemParametersInfoW", SetLastError=true)]
  private static extern bool SystemParametersInfoSet(uint action, uint parameter, IntPtr value, uint flags);
  [DllImport("user32.dll", SetLastError=true)]
  private static extern IntPtr SendMessageTimeout(IntPtr hwnd, uint message, IntPtr wParam, IntPtr lParam,
    uint flags, uint timeout, out IntPtr result);
  public static bool ScreenReaderHint() {
    bool value;
    if (!SystemParametersInfoGet(0x0046, 0, out value, 0)) throw new InvalidOperationException("screen reader hint unavailable");
    return value;
  }
  public static void SetScreenReaderHint(bool enabled) {
    if (!SystemParametersInfoSet(0x0047, enabled ? 1u : 0u, IntPtr.Zero, 0x0002))
      throw new InvalidOperationException("screen reader hint update rejected");
  }
  public static bool RequestGracefulClose(IntPtr hwnd) {
    IntPtr result;
    return SendMessageTimeout(hwnd, 0x0010, IntPtr.Zero, IntPtr.Zero, 0x0002, 2000, out result) != IntPtr.Zero;
  }
  public static object AccessibleClient(IntPtr hwnd) {
    if (hwnd == IntPtr.Zero) return null;
    var iid = new Guid("618736E0-3C3D-11CF-810C-00AA00389B71");
    object accessible;
    var result = AccessibleObjectFromWindow(hwnd, unchecked((uint)-4), ref iid, out accessible);
    return result == 0 ? accessible : null;
  }
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
  public static IntPtr[] Descendants(IntPtr root) {
    var found = new List<IntPtr>();
    EnumChildWindows(root, (hwnd, state) => { found.Add(hwnd); return found.Count < 128; }, IntPtr.Zero);
    return found.ToArray();
  }
  public static uint WindowProcessId(IntPtr hwnd) { uint pid; GetWindowThreadProcessId(hwnd, out pid); return pid; }
  public static string WindowClass(IntPtr hwnd) {
    var value = new StringBuilder(256); GetClassName(hwnd, value, value.Capacity); return value.ToString();
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
  if (-not $token -or $token -cnotmatch '(?i)(window|button|pane|panel|list|item|view|control|edit|textblock|image|scroll|grid|root|content|frame|presenter|border|canvas|stack|navigation|tab|menu|toolbar|chrome|render|widget|host)') {
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

$originalScreenReaderHint=$false
$screenReaderHintChanged=$false
$screenReaderHintRestored=$false
$appRelaunched=$false
$gracefulCloseOnly=$false
$processesTerminated=0
$overrideKey='HKCU:\Software\Policies\Microsoft\Edge\WebView2\AdditionalBrowserArguments'
$overrideNames=@($appUserModelId, $processName)
$overridePrevious=@{}
$overrideKeyExisted=Test-Path -LiteralPath $overrideKey
$overrideRestored=$false
$accessibilityFlagObserved=$false
try {
  if ([Security.Principal.WindowsIdentity]::GetCurrent().Name -cne $request.InteractiveUser -or
      [Diagnostics.Process]::GetCurrentProcess().SessionId -ne [int]$request.SessionId) {
    throw 'interactive runner session identity mismatch'
  }
  $actualWrapperSha256 = (Get-FileHash -LiteralPath $PSCommandPath -Algorithm SHA256).Hash.ToLowerInvariant()
  if ($actualWrapperSha256 -cne $request.WrapperSha256) { throw 'interactive runner hash mismatch' }

  $originalScreenReaderHint=[OslQaWindowInventory]::ScreenReaderHint()
  if(-not $originalScreenReaderHint){
    [OslQaWindowInventory]::SetScreenReaderHint($true)
    $screenReaderHintChanged=$true
    Start-Sleep -Milliseconds 500
  }

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

  if ($request.Mode -cin @('gracefulRelaunch', 'verifiedRelaunch')) {
    if (-not [OslQaWindowInventory]::RequestGracefulClose($windows[0])) {
      throw 'exact WhatsApp window rejected graceful close'
    }
    $gracefulCloseOnly=$true
    $closeDeadline=[DateTime]::UtcNow.AddSeconds(20)
    do {
      Start-Sleep -Milliseconds 250
      $remaining=@(Get-CimInstance Win32_Process -Filter "ProcessId = $pidValue" -ErrorAction SilentlyContinue)
    } while ($remaining.Count -gt 0 -and [DateTime]::UtcNow -lt $closeDeadline)
    if ($remaining.Count -gt 0) {
      if ($request.Mode -cne 'verifiedRelaunch') {
        throw 'exact WhatsApp process did not exit after graceful close'
      }
      $verified=@(Get-CimInstance Win32_Process -Filter "ProcessId = $pidValue" | Where-Object {
        $_.Name -ceq $processName -and [int]$_.SessionId -eq [int]$request.SessionId -and
        [string]$_.CreationDate -ceq $creationIdentity -and $_.ExecutablePath -and
        [IO.Path]::GetFullPath([string]$_.ExecutablePath).Equals($expectedExecutable, [StringComparison]::OrdinalIgnoreCase)
      })
      if ($verified.Count -ne 1) { throw 'exact WhatsApp process changed before bounded restart' }
      Stop-Process -Id $pidValue -Force
      $processesTerminated=1
      $gracefulCloseOnly=$false
      $terminateDeadline=[DateTime]::UtcNow.AddSeconds(10)
      do {
        Start-Sleep -Milliseconds 100
        $remaining=@(Get-CimInstance Win32_Process -Filter "ProcessId = $pidValue" -ErrorAction SilentlyContinue)
      } while ($remaining.Count -gt 0 -and [DateTime]::UtcNow -lt $terminateDeadline)
      if ($remaining.Count -gt 0) { throw 'exact WhatsApp process did not terminate within bound' }
    }

    [void](New-Item -Path $overrideKey -Force)
    foreach($name in $overrideNames){
      $existingValue=Get-ItemProperty -LiteralPath $overrideKey -Name $name -ErrorAction SilentlyContinue
      if($null -ne $existingValue){$overridePrevious[$name]=[string]$existingValue.$name}else{$overridePrevious[$name]=$null}
      Set-ItemProperty -LiteralPath $overrideKey -Name $name -Type String -Value '--force-renderer-accessibility=complete'
    }
    Start-Process -FilePath 'explorer.exe' -ArgumentList "shell:AppsFolder\$appUserModelId"
    $appRelaunched=$true
    $launchDeadline=[DateTime]::UtcNow.AddSeconds(90)
    do {
      Start-Sleep -Milliseconds 250
      $processes = @(Get-CimInstance Win32_Process -Filter "Name = 'WhatsApp.Root.exe'" | Where-Object {
        [int]$_.SessionId -eq [int]$request.SessionId -and $_.ExecutablePath -and
        [IO.Path]::GetFullPath([string]$_.ExecutablePath).Equals($expectedExecutable, [StringComparison]::OrdinalIgnoreCase)
      })
      if ($processes.Count -eq 1) {
        $pidValue=[uint32]$processes[0].ProcessId
        $creationIdentity=[string]$processes[0].CreationDate
        $windows=[OslQaWindowInventory]::Exact($pidValue, $windowClass, $windowTitle)
      }
    } while (($processes.Count -ne 1 -or $windows.Count -ne 1) -and [DateTime]::UtcNow -lt $launchDeadline)
    if ($processes.Count -ne 1 -or $windows.Count -ne 1) {
      throw 'exact WhatsApp accessibility relaunch did not become ready'
    }
    $flagDeadline=[DateTime]::UtcNow.AddSeconds(30)
    do {
      Start-Sleep -Milliseconds 250
      $flagged=@(Get-CimInstance Win32_Process -Filter "Name = 'msedgewebview2.exe'" | Where-Object {
        [int]$_.SessionId -eq [int]$request.SessionId -and $_.CommandLine -and
        $_.CommandLine.Contains('--force-renderer-accessibility=complete')
      })
      $accessibilityFlagObserved=$flagged.Count -gt 0
    } while (-not $accessibilityFlagObserved -and [DateTime]::UtcNow -lt $flagDeadline)
    if(-not $accessibilityFlagObserved){throw 'WhatsApp WebView did not receive the bounded accessibility override'}
    foreach($name in $overrideNames){
      if($null -eq $overridePrevious[$name]){Remove-ItemProperty -LiteralPath $overrideKey -Name $name -ErrorAction Stop}
      else{Set-ItemProperty -LiteralPath $overrideKey -Name $name -Type String -Value $overridePrevious[$name]}
    }
    if(-not $overrideKeyExisted){
      $remainingNames=@((Get-ItemProperty -LiteralPath $overrideKey).PSObject.Properties.Name | Where-Object {$_ -notmatch '^PS(Path|ParentPath|ChildName|Drive|Provider)$'})
      if($remainingNames.Count -eq 0){Remove-Item -LiteralPath $overrideKey -Force}
    }
    $overrideRestored=$true
  } elseif ($request.Mode -cne 'observe') {
    throw 'UIA probe mode is invalid'
  }

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
  $hitTestNodes = [Collections.Generic.List[object]]::new()
  $descendantWindows = [Collections.Generic.List[object]]::new()
  $truncated = $false
  $captureStarted=[DateTime]::UtcNow

  while ($queue.Count -gt 0) {
    if (([DateTime]::UtcNow - $captureStarted).TotalSeconds -ge $deadlineSeconds) { throw 'UIA probe deadline exceeded' }
    if ($nodes.Count -ge $maximumNodes) { $truncated = $true; break }
    $item = $queue.Dequeue()
    $element = $item.Element
    try {
      $current = $element.Current
      if ($current.ProcessId -ne [int]$pidValue) { throw 'cross-process UIA node rejected' }
      $runtimeHash = Get-StructuralHash ([int[]]$element.GetRuntimeId())
      if (-not $runtimeHash) { throw 'UIA node runtime identity unavailable' }
      $msaaChildCount = $null
      $msaaRole = $null
      $accessible = $null
      try {
        $accessible = [OslQaWindowInventory]::AccessibleClient([IntPtr]$current.NativeWindowHandle)
        if ($null -ne $accessible) {
          $candidateChildCount = [int]$accessible.accChildCount
          $candidateRole = [int]$accessible.accRole(0)
          if ($candidateChildCount -ge 0 -and $candidateChildCount -le $maximumNodes -and
              $candidateRole -ge 0 -and $candidateRole -le 255) {
            $msaaChildCount = $candidateChildCount
            $msaaRole = $candidateRole
          }
        }
      } finally {
        if ($null -ne $accessible -and [Runtime.InteropServices.Marshal]::IsComObject($accessible)) {
          [void][Runtime.InteropServices.Marshal]::FinalReleaseComObject($accessible)
        }
      }
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
        MsaaChildCount = $msaaChildCount
        MsaaRole = $msaaRole
      })
      if ([int]$item.Depth -lt $maximumDepth) {
        $child = $walker.GetFirstChild($element)
        while ($null -ne $child) {
          if (([DateTime]::UtcNow - $captureStarted).TotalSeconds -ge $deadlineSeconds) { throw 'UIA probe deadline exceeded' }
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

  $trustedBrowserPids=@(Get-CimInstance Win32_Process -Filter "Name = 'msedgewebview2.exe'" | Where-Object {
    [int]$_.SessionId -eq [int]$request.SessionId -and $_.CommandLine -and
    $_.CommandLine.Contains('--force-renderer-accessibility=complete')
  } | ForEach-Object {[int]$_.ProcessId})
  foreach($childWindow in [OslQaWindowInventory]::Descendants($windows[0])){
    if($descendantWindows.Count -ge 128){break}
    $childPid=[int][OslQaWindowInventory]::WindowProcessId($childWindow)
    if($childPid -ne [int]$pidValue -and $childPid -notin $trustedBrowserPids){continue}
    try{
      $childElement=[Windows.Automation.AutomationElement]::FromHandle($childWindow)
      $childCurrent=$childElement.Current
      $childWalker=[Windows.Automation.TreeWalker]::RawViewWalker
      $descendantWindows.Add([ordered]@{
        ProcessKind=if($childPid -eq [int]$pidValue){'host'}else{'verifiedWebView'}
        ClassName=Get-SafeClassName ([OslQaWindowInventory]::WindowClass($childWindow))
        ControlType=[string]$childCurrent.ControlType.ProgrammaticName
        FrameworkId=Get-SafeFrameworkId ([string]$childCurrent.FrameworkId)
        GeometryQ16=Get-QuantizedRect $childCurrent.BoundingRectangle $rootRect
        HasRawChild=$null -ne $childWalker.GetFirstChild($childElement)
      })
    }catch [Windows.Automation.ElementNotAvailableException]{continue}
  }
  $seenHitIds=[Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
  foreach($xPercent in @(15,30,45,60,75,90)){
    foreach($yPercent in @(8,20,35,50,65,78,88,94,97)){
      if (([DateTime]::UtcNow - $captureStarted).TotalSeconds -ge $deadlineSeconds) { throw 'UIA probe deadline exceeded' }
      $point=[Windows.Point]::new(
        $rootRect.X + ($rootRect.Width * $xPercent / 100.0),
        $rootRect.Y + ($rootRect.Height * $yPercent / 100.0)
      )
      try{
        $hit=[Windows.Automation.AutomationElement]::FromPoint($point)
        if($null -eq $hit){continue}
        $hitCurrent=$hit.Current
        $hitPid=[int]$hitCurrent.ProcessId
        if($hitPid -ne [int]$pidValue -and $hitPid -notin $trustedBrowserPids){continue}
        $hitHash=Get-StructuralHash ([int[]]$hit.GetRuntimeId())
        if(-not $hitHash -or -not $seenHitIds.Add($hitHash)){continue}
        $hitTestNodes.Add([ordered]@{
          RuntimeHash=$hitHash
          ProcessKind=if($hitPid -eq [int]$pidValue){'host'}else{'verifiedWebView'}
          ControlType=[string]$hitCurrent.ControlType.ProgrammaticName
          AutomationId=Get-SafeAutomationId ([string]$hitCurrent.AutomationId)
          ClassName=Get-SafeClassName ([string]$hitCurrent.ClassName)
          FrameworkId=Get-SafeFrameworkId ([string]$hitCurrent.FrameworkId)
          GeometryQ16=Get-QuantizedRect $hitCurrent.BoundingRectangle $rootRect
          Enabled=[bool]$hitCurrent.IsEnabled
          Offscreen=[bool]$hitCurrent.IsOffscreen
        })
      }catch [Windows.Automation.ElementNotAvailableException]{continue}
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
    HitTestNodeCount = $hitTestNodes.Count
    HitTestNodes = $hitTestNodes
    DescendantWindowCount = $descendantWindows.Count
    DescendantWindows = $descendantWindows
    ForegroundChanged = $false
    InputInjected = $false
    ProviderStorageRead = $false
    ContentPropertiesRead = $false
    AppLaunched = $appRelaunched
    GracefulCloseOnly = $gracefulCloseOnly
    ProcessesTerminated = $processesTerminated
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
    AppLaunched = $appRelaunched
    GracefulCloseOnly = $gracefulCloseOnly
    ProcessesTerminated = $processesTerminated
    Error = 'whatsapp-uia-structure-probe-failed-closed'
    ExceptionType = $_.Exception.GetType().Name
  }
} finally {
  if($screenReaderHintChanged){
    try{
      [OslQaWindowInventory]::SetScreenReaderHint($originalScreenReaderHint)
      $screenReaderHintRestored=([OslQaWindowInventory]::ScreenReaderHint() -eq $originalScreenReaderHint)
    }catch{$screenReaderHintRestored=$false}
  }else{$screenReaderHintRestored=$true}
  if($request.Mode -cne 'observe' -and -not $overrideRestored){
    try{
      foreach($name in $overrideNames){
        if($overridePrevious.ContainsKey($name) -and $null -ne $overridePrevious[$name]){
          Set-ItemProperty -LiteralPath $overrideKey -Name $name -Type String -Value $overridePrevious[$name]
        }else{Remove-ItemProperty -LiteralPath $overrideKey -Name $name -ErrorAction SilentlyContinue}
      }
      if(-not $overrideKeyExisted -and (Test-Path -LiteralPath $overrideKey)){
        $remainingNames=@((Get-ItemProperty -LiteralPath $overrideKey).PSObject.Properties.Name | Where-Object {$_ -notmatch '^PS(Path|ParentPath|ChildName|Drive|Provider)$'})
        if($remainingNames.Count -eq 0){Remove-Item -LiteralPath $overrideKey -Force}
      }
      $overrideRestored=$true
    }catch{$overrideRestored=$false}
  }elseif($request.Mode -ceq 'observe'){$overrideRestored=$true}
  $terminal['AccessibilityHintTemporarilyEnabled']=$screenReaderHintChanged
  $terminal['AccessibilityHintRestored']=$screenReaderHintRestored
  $terminal['AccessibilityOverrideRestored']=$overrideRestored
  $terminal['AccessibilityFlagObserved']=$accessibilityFlagObserved
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
$settings = New-ScheduledTaskSettingsSet -ExecutionTimeLimit ([TimeSpan]::FromMinutes(4)) -MultipleInstances IgnoreNew
Register-ScheduledTask -TaskName $taskName -Action $action -Principal $principal -Settings $settings | Out-Null
Start-ScheduledTask -TaskName $taskName

[pscustomobject]@{
  InvocationId = $InvocationId
  Status = 'armed'
  TaskName = $taskName
  SessionId = $SessionId
  WrapperSha256 = $wrapperSha256
} | ConvertTo-Json -Compress
