param(
  [Parameter(Mandatory)][ValidateSet(1,2)][int]$ClientNumber,
  [Parameter(Mandatory)][ValidatePattern('^[a-z0-9-]{8,80}$')][string]$InvocationId,
  [Parameter(Mandatory)][ValidatePattern('^[0-9a-f]{64}$')][string]$OslExeSha256,
  [Parameter(Mandatory)][ValidateRange(1,128)][int]$SessionId,
  [Parameter(Mandatory)][ValidateRange(0,4095)][int]$WindowX,
  [Parameter(Mandatory)][ValidateRange(0,1023)][int]$WindowY,
  [Parameter(Mandatory)][ValidateSet('Enter','Space')][string]$ActivationKey
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$oslPath = [IO.Path]::GetFullPath('C:\Users\osltest\Desktop\OSL Privacy\OSL Privacy.exe')
$resultPath = [IO.Path]::GetFullPath("C:\Users\osltest\AppData\Local\Temp\$InvocationId.result.json")
$leafPath = [IO.Path]::GetFullPath("C:\Users\osltest\AppData\Local\Temp\$InvocationId.leaf.ps1")
$stagePath = [IO.Path]::GetFullPath("C:\Users\osltest\AppData\Local\Temp\$InvocationId.stage.txt")
$taskName = "OSL-WA-Click-$InvocationId"

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
  [Parameter(Mandatory)][string]$ResultPath,
  [Parameter(Mandatory)][string]$StagePath,
  [Parameter(Mandatory)][int]$WindowX,
  [Parameter(Mandatory)][int]$WindowY,
  [Parameter(Mandatory)][ValidateSet('Enter','Space')][string]$ActivationKey
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
'started' | Set-Content -LiteralPath $StagePath -Encoding Ascii
Add-Type @"
using System;
using System.Runtime.InteropServices;
public static class OslBoundedClick {
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left,Top,Right,Bottom; }
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hwnd,out RECT rect);
  [DllImport("user32.dll")] public static extern bool IsWindow(IntPtr hwnd);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hwnd);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hwnd);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x,int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint flags,uint dx,uint dy,uint data,UIntPtr extra);
  [DllImport("user32.dll")] public static extern void keybd_event(byte key,byte scan,uint flags,UIntPtr extra);
  [DllImport("user32.dll")] public static extern IntPtr WindowFromPoint(POINT point);
  [DllImport("user32.dll")] public static extern IntPtr GetAncestor(IntPtr hwnd,uint flags);
  [StructLayout(LayoutKind.Sequential)] public struct POINT { public int X,Y; }
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
  'hashChecked' | Set-Content -LiteralPath $StagePath -Encoding Ascii
  $processes = @(Get-Process -Name 'OSL Privacy' -ErrorAction Stop | Where-Object {
    $_.SessionId -eq $SessionId -and $_.Path -and
    [IO.Path]::GetFullPath($_.Path).Equals($OslPath, [StringComparison]::OrdinalIgnoreCase)
  })
  if ($processes.Count -ne 1 -or $processes[0].MainWindowHandle -eq [IntPtr]::Zero) {
    throw 'exact OSL QA main window is unavailable or ambiguous'
  }
  'windowResolved' | Set-Content -LiteralPath $StagePath -Encoding Ascii
  $hwnd = $processes[0].MainWindowHandle
  if (-not [OslBoundedClick]::IsWindow($hwnd) -or -not [OslBoundedClick]::IsWindowVisible($hwnd)) {
    throw 'exact OSL QA main window is not visible'
  }
  $rect = New-Object OslBoundedClick+RECT
  if (-not [OslBoundedClick]::GetWindowRect($hwnd, [ref]$rect)) { throw 'OSL window geometry is unavailable' }
  $width = $rect.Right - $rect.Left
  $height = $rect.Bottom - $rect.Top
  if ($WindowX -lt 0 -or $WindowY -lt 0 -or $WindowX -ge $width -or $WindowY -ge $height) {
    throw 'bounded OSL click is outside the exact window'
  }
  if (-not [OslBoundedClick]::SetForegroundWindow($hwnd)) { throw 'VM rejected exact OSL foreground selection' }
  'vmOslForegrounded' | Set-Content -LiteralPath $StagePath -Encoding Ascii
  Start-Sleep -Milliseconds 150
  $screenX = $rect.Left + $WindowX
  $screenY = $rect.Top + $WindowY
  $point = New-Object OslBoundedClick+POINT
  $point.X = $screenX
  $point.Y = $screenY
  $atPoint = [OslBoundedClick]::WindowFromPoint($point)
  if ($atPoint -eq [IntPtr]::Zero -or [OslBoundedClick]::GetAncestor($atPoint, 2) -ne $hwnd) {
    throw 'bounded click does not resolve to the exact OSL surface'
  }
  if (-not [OslBoundedClick]::SetCursorPos($screenX, $screenY)) { throw 'VM rejected bounded cursor placement' }
  [OslBoundedClick]::mouse_event(0x0002,0,0,0,[UIntPtr]::Zero)
  [OslBoundedClick]::mouse_event(0x0004,0,0,0,[UIntPtr]::Zero)
  Start-Sleep -Milliseconds 100
  $key = if ($ActivationKey -ceq 'Enter') { [byte]0x0D } else { [byte]0x20 }
  $scan = if ($ActivationKey -ceq 'Enter') { [byte]0x1C } else { [byte]0x39 }
  [OslBoundedClick]::keybd_event($key,$scan,0,[UIntPtr]::Zero)
  [OslBoundedClick]::keybd_event($key,$scan,2,[UIntPtr]::Zero)
  'clickPosted' | Set-Content -LiteralPath $StagePath -Encoding Ascii
  Write-Receipt @{
    Schema='whatsapp-osl-bounded-click/v1';Status='clicked';Terminal=$true
    ClientNumber=$ClientNumber;InvocationId=$InvocationId;OslExeSha256=$OslExeSha256
    ExactOslSurfaceOnly=$true;ProviderContentRead=$false;ProviderInputSent=$false
    ProviderStorageRead=$false;LocalRdpForegrounded=$false;VmOslForegrounded=$true
  }
} catch {
  Write-Receipt @{
    Schema='whatsapp-osl-bounded-click/v1';Status='failedClosed';Terminal=$true
    ClientNumber=$ClientNumber;InvocationId=$InvocationId;Error='Exact bounded OSL click was rejected'
    ExactOslSurfaceOnly=$true;ProviderContentRead=$false;ProviderInputSent=$false
    ProviderStorageRead=$false;LocalRdpForegrounded=$false
  }
  exit 1
}
'@

if (Test-Path -LiteralPath $resultPath -PathType Leaf) {
  Get-Content -LiteralPath $resultPath -Raw
  exit 0
}
$leaf | Set-Content -LiteralPath $leafPath -Encoding UTF8
$arguments = "-NoProfile -NonInteractive -ExecutionPolicy Bypass -File `"$leafPath`" " +
  "-OslPath `"$oslPath`" -OslExeSha256 $OslExeSha256 -SessionId $SessionId " +
  "-ClientNumber $ClientNumber -InvocationId $InvocationId -ResultPath `"$resultPath`" " +
  "-StagePath `"$stagePath`" " +
  "-WindowX $WindowX -WindowY $WindowY -ActivationKey $ActivationKey"
$action = New-ScheduledTaskAction -Execute 'powershell.exe' -Argument $arguments
$principal = New-ScheduledTaskPrincipal -UserId $interactiveUser -LogonType Interactive -RunLevel Limited
$settings = New-ScheduledTaskSettingsSet -ExecutionTimeLimit ([TimeSpan]::FromSeconds(30)) -MultipleInstances IgnoreNew
Register-ScheduledTask -TaskName $taskName -Action $action -Principal $principal -Settings $settings -Force | Out-Null
try {
  Start-ScheduledTask -TaskName $taskName
  $deadline = [DateTime]::UtcNow.AddSeconds(15)
  do {
    if (Test-Path -LiteralPath $resultPath -PathType Leaf) {
      $raw = Get-Content -LiteralPath $resultPath -Raw
      $receipt = $raw | ConvertFrom-Json
      if ($receipt.Schema -cne 'whatsapp-osl-bounded-click/v1' -or
          $receipt.InvocationId -cne $InvocationId -or -not $receipt.Terminal) {
        throw 'bounded OSL click returned an invalid receipt'
      }
      $raw
      exit $(if ($receipt.Status -ceq 'clicked') { 0 } else { 1 })
    }
    Start-Sleep -Milliseconds 100
  } while ([DateTime]::UtcNow -lt $deadline)
  $stage = if (Test-Path -LiteralPath $stagePath -PathType Leaf) {
    (Get-Content -LiteralPath $stagePath -Raw).Trim()
  } else {
    'notStarted'
  }
  $taskInfo = Get-ScheduledTaskInfo -TaskName $taskName -ErrorAction SilentlyContinue
  $lastTaskResult = if ($taskInfo) { [int]$taskInfo.LastTaskResult } else { -1 }
  throw "bounded OSL click did not complete; stage=$stage; taskResult=$lastTaskResult"
} finally {
  Unregister-ScheduledTask -TaskName $taskName -Confirm:$false -ErrorAction SilentlyContinue
}
