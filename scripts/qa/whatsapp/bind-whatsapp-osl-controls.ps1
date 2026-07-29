param(
  [Parameter(Mandatory)][ValidateSet(1,2)][int]$ClientNumber,
  [Parameter(Mandatory)][ValidatePattern('^[a-z0-9-]{8,80}$')][string]$InvocationId,
  [Parameter(Mandatory)][ValidatePattern('^[0-9a-f]{64}$')][string]$OslExeSha256,
  [Parameter(Mandatory)][ValidateRange(1,128)][int]$SessionId
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$oslPath = [IO.Path]::GetFullPath('C:\Users\osltest\Desktop\OSL Privacy\OSL Privacy.exe')
$resultPath = [IO.Path]::GetFullPath("C:\Users\osltest\AppData\Local\Temp\$InvocationId.result.json")
$leafPath = [IO.Path]::GetFullPath("C:\Users\osltest\AppData\Local\Temp\$InvocationId.leaf.ps1")
$taskName = "OSL-WA-Bind-$InvocationId"

if (-not (Test-Path -LiteralPath $oslPath -PathType Leaf) -or
    (Get-FileHash -LiteralPath $oslPath -Algorithm SHA256).Hash.ToLowerInvariant() -cne $OslExeSha256) {
  throw 'exact OSL QA executable is unavailable'
}

$explorers = @(Get-CimInstance Win32_Process -Filter "Name = 'explorer.exe'" | Where-Object {
  [int]$_.SessionId -eq $SessionId
})
if ($explorers.Count -ne 1) { throw 'interactive Explorer session is unavailable or ambiguous' }
$owner = Invoke-CimMethod -InputObject $explorers[0] -MethodName GetOwner
if ($owner.ReturnValue -ne 0 -or $owner.User -cne 'osltest' -or -not $owner.Domain) {
  throw 'interactive session owner is not exact osltest identity'
}
$interactiveUser = "$($owner.Domain)\$($owner.User)"

$leaf = @'
param(
  [Parameter(Mandatory)][string]$OslPath,
  [Parameter(Mandatory)][string]$OslExeSha256,
  [Parameter(Mandatory)][int]$SessionId,
  [Parameter(Mandatory)][int]$ClientNumber,
  [Parameter(Mandatory)][string]$InvocationId,
  [Parameter(Mandatory)][string]$ResultPath
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type @"
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;
public static class OslBindingNative {
  public delegate bool EnumProc(IntPtr hwnd, IntPtr state);
  [DllImport("user32.dll")] static extern bool EnumChildWindows(IntPtr parent, EnumProc callback, IntPtr state);
  [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr hwnd);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern int GetClassName(IntPtr hwnd, StringBuilder value, int count);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint processId);
  public static IntPtr[] VisibleRenderersFor(IntPtr root) {
    var values = new List<IntPtr>();
    EnumChildWindows(root, (hwnd, ignored) => {
      var kind = new StringBuilder(128);
      GetClassName(hwnd, kind, kind.Capacity);
      if (IsWindowVisible(hwnd) && kind.ToString() == "Chrome_RenderWidgetHostHWND") values.Add(hwnd);
      return true;
    }, IntPtr.Zero);
    return values.ToArray();
  }
}
"@

function Write-Receipt([hashtable]$Receipt) {
  $temporary = "$ResultPath.tmp"
  $Receipt | ConvertTo-Json -Compress | Set-Content -LiteralPath $temporary -Encoding UTF8
  if (Test-Path -LiteralPath $ResultPath) { Remove-Item -LiteralPath $ResultPath -Force }
  [IO.File]::Move($temporary, $ResultPath)
}

try {
  if ((Get-FileHash -LiteralPath $OslPath -Algorithm SHA256).Hash.ToLowerInvariant() -cne $OslExeSha256) {
    throw 'exact OSL QA executable hash changed'
  }
  $processes = @(Get-Process -Name 'OSL Privacy' -ErrorAction Stop | Where-Object {
    $_.SessionId -eq $SessionId -and $_.Path -and
    [IO.Path]::GetFullPath($_.Path).Equals($OslPath, [StringComparison]::OrdinalIgnoreCase)
  })
  if ($processes.Count -ne 1 -or $processes[0].MainWindowHandle -eq [IntPtr]::Zero) {
    throw 'exact OSL QA main window is unavailable or ambiguous'
  }
  $renderers = @([OslBindingNative]::VisibleRenderersFor($processes[0].MainWindowHandle))
  if ($renderers.Count -ne 1) { throw 'exact OSL WebView2 renderer is unavailable or ambiguous' }
  [uint32]$rendererProcessId = 0
  [void][OslBindingNative]::GetWindowThreadProcessId($renderers[0], [ref]$rendererProcessId)
  $rendererProcess = Get-Process -Id ([int]$rendererProcessId) -ErrorAction Stop
  if ($rendererProcess.SessionId -ne $SessionId -or
      [IO.Path]::GetFileName($rendererProcess.Path) -cne 'msedgewebview2.exe') {
    throw 'exact OSL WebView2 renderer identity is invalid'
  }
  $root = [Windows.Automation.AutomationElement]::FromHandle($renderers[0])

  function Get-ExactControl([string]$AutomationId, [Windows.Automation.ControlType]$ControlType, [int]$TimeoutSeconds) {
    $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
    do {
      $condition = [Windows.Automation.PropertyCondition]::new(
        [Windows.Automation.AutomationElement]::AutomationIdProperty, $AutomationId
      )
      $matches = @($root.FindAll([Windows.Automation.TreeScope]::Descendants, $condition) | Where-Object {
        $_.Current.ProcessId -eq $rendererProcessId -and $_.Current.ControlType -eq $ControlType
      })
      if ($matches.Count -eq 1 -and $matches[0].Current.IsEnabled -and -not $matches[0].Current.IsOffscreen) {
        return $matches[0]
      }
      if ($matches.Count -gt 1) { throw "OSL control $AutomationId is ambiguous" }
      Start-Sleep -Milliseconds 150
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "OSL control $AutomationId is unavailable"
  }

  function Invoke-Exact([string]$AutomationId, [int]$TimeoutSeconds = 8) {
    $control = Get-ExactControl $AutomationId ([Windows.Automation.ControlType]::Button) $TimeoutSeconds
    $pattern = $control.GetCurrentPattern([Windows.Automation.InvokePattern]::Pattern)
    $pattern.Invoke()
  }

  Invoke-Exact 'bind-current-chat'
  Invoke-Exact 'binding-begin'
  $attestation = Get-ExactControl 'binding-attestation' ([Windows.Automation.ControlType]::CheckBox) 12
  $toggle = $attestation.GetCurrentPattern([Windows.Automation.TogglePattern]::Pattern)
  if ($toggle.Current.ToggleState -eq [Windows.Automation.ToggleState]::Off) { $toggle.Toggle() }
  Invoke-Exact 'binding-confirm'
  [void](Get-ExactControl 'protected-carrier' ([Windows.Automation.ControlType]::Edit) 15)

  Write-Receipt @{
    Schema = 'whatsapp-osl-control-binding/v1'
    Status = 'bound'
    Terminal = $true
    ClientNumber = $ClientNumber
    InvocationId = $InvocationId
    OslExeSha256 = $OslExeSha256
    ExactOslControlsOnly = $true
    ProviderContentRead = $false
    ProviderInputSent = $false
    ProviderStorageRead = $false
    ClipboardRead = $false
    WindowForegrounded = $false
  }
} catch {
  Write-Receipt @{
    Schema = 'whatsapp-osl-control-binding/v1'
    Status = 'failedClosed'
    Terminal = $true
    ClientNumber = $ClientNumber
    InvocationId = $InvocationId
    Error = 'Exact OSL visual binding could not be completed'
    ExactOslControlsOnly = $true
    ProviderContentRead = $false
    ProviderInputSent = $false
    ProviderStorageRead = $false
    ClipboardRead = $false
    WindowForegrounded = $false
  }
  exit 1
}
'@

if (Test-Path -LiteralPath $resultPath -PathType Leaf) {
  $existing = Get-Content -LiteralPath $resultPath -Raw | ConvertFrom-Json
  if ($existing.Schema -ceq 'whatsapp-osl-control-binding/v1' -and
      $existing.InvocationId -ceq $InvocationId -and $existing.Terminal) {
    Get-Content -LiteralPath $resultPath -Raw
    exit $(if ($existing.Status -ceq 'bound') { 0 } else { 1 })
  }
  throw 'existing OSL binding result is invalid'
}

$leaf | Set-Content -LiteralPath $leafPath -Encoding UTF8
$arguments = "-NoProfile -NonInteractive -ExecutionPolicy Bypass -File `"$leafPath`" " +
  "-OslPath `"$oslPath`" -OslExeSha256 $OslExeSha256 -SessionId $SessionId " +
  "-ClientNumber $ClientNumber -InvocationId $InvocationId -ResultPath `"$resultPath`""
$action = New-ScheduledTaskAction -Execute 'powershell.exe' -Argument $arguments
$principal = New-ScheduledTaskPrincipal -UserId $interactiveUser -LogonType Interactive -RunLevel Limited
$settings = New-ScheduledTaskSettingsSet -ExecutionTimeLimit ([TimeSpan]::FromMinutes(1)) -MultipleInstances IgnoreNew
Register-ScheduledTask -TaskName $taskName -Action $action -Principal $principal -Settings $settings -Force | Out-Null
try {
  Start-ScheduledTask -TaskName $taskName
  $deadline = [DateTime]::UtcNow.AddSeconds(45)
  do {
    if (Test-Path -LiteralPath $resultPath -PathType Leaf) {
      $raw = Get-Content -LiteralPath $resultPath -Raw
      $receipt = $raw | ConvertFrom-Json
      if ($receipt.Schema -cne 'whatsapp-osl-control-binding/v1' -or
          $receipt.InvocationId -cne $InvocationId -or -not $receipt.Terminal) {
        throw 'OSL binding leaf returned an invalid receipt'
      }
      $raw
      exit $(if ($receipt.Status -ceq 'bound') { 0 } else { 1 })
    }
    Start-Sleep -Milliseconds 200
  } while ([DateTime]::UtcNow -lt $deadline)
  throw 'OSL binding leaf did not complete within the bounded deadline'
} finally {
  Unregister-ScheduledTask -TaskName $taskName -Confirm:$false -ErrorAction SilentlyContinue
}
