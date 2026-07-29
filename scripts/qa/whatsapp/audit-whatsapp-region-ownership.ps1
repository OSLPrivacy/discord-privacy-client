param(
  [Parameter(Mandatory)][ValidateRange(1,128)][int]$SessionId,
  [string]$CaptureTranscript='false',
  [string]$ResultPath=''
)
$ErrorActionPreference='Stop'
Set-StrictMode -Version Latest
$captureEnabled=[string]::Equals($CaptureTranscript,'true',[StringComparison]::OrdinalIgnoreCase)
if($ResultPath){
  trap {
    $failure=[ordered]@{
      Schema='whatsapp-region-ownership-audit/v1'
      Status='failedClosed'
      FailureCode=if($_.FullyQualifiedErrorId){[string]$_.FullyQualifiedErrorId}else{'unknown'}
      FailureLine=[int]$_.InvocationInfo.ScriptLineNumber
      FailureType=[string]$_.Exception.GetType().Name
      FailureMessage=[string]$_.Exception.Message
      ProviderContentRead=$false
      WhatsAppPrivateStorageRead=$false
      InputSent=$false
      WindowForegrounded=$false
    }|ConvertTo-Json -Compress
    [IO.File]::WriteAllText($ResultPath,$failure,[Text.UTF8Encoding]::new($false))
    exit 1
  }
}

if(-not $ResultPath){
  $owners=@(Get-CimInstance Win32_Process -Filter "Name = 'explorer.exe'" |
    Where-Object{[int]$_.SessionId -eq $SessionId})
  if($owners.Count -ne 1){throw 'interactive session owner is unavailable or ambiguous'}
  $identity=Invoke-CimMethod -InputObject $owners[0] -MethodName GetOwner
  if($identity.ReturnValue -ne 0 -or $identity.User -cne 'osltest' -or -not $identity.Domain){
    throw 'interactive session owner identity changed'
  }
  $user="$($identity.Domain)\$($identity.User)"
  $root='C:\Users\osltest\AppData\Local\OSL-QA\whatsapp-region-audit-v1'
  [void](New-Item -ItemType Directory -Path $root -Force)
  $inherit=[Security.AccessControl.InheritanceFlags]'ContainerInherit,ObjectInherit'
  $propagate=[Security.AccessControl.PropagationFlags]::None
  $acl=[Security.AccessControl.DirectorySecurity]::new()
  $acl.SetAccessRuleProtection($true,$false)
  $acl.AddAccessRule([Security.AccessControl.FileSystemAccessRule]::new(
    'SYSTEM',[Security.AccessControl.FileSystemRights]::FullControl,$inherit,$propagate,
    [Security.AccessControl.AccessControlType]::Allow))
  $acl.AddAccessRule([Security.AccessControl.FileSystemAccessRule]::new(
    $user,[Security.AccessControl.FileSystemRights]::Modify,$inherit,$propagate,
    [Security.AccessControl.AccessControlType]::Allow))
  Set-Acl -LiteralPath $root -AclObject $acl
  $staged=Join-Path $root 'audit.ps1'
  $runner=Join-Path $root 'runner.ps1'
  $result=Join-Path $root 'result.json'
  Copy-Item -LiteralPath $PSCommandPath -Destination $staged -Force
  Remove-Item -LiteralPath $result -Force -ErrorAction SilentlyContinue
  $runnerSource=@'
param([string]$AuditScript,[int]$ExpectedSession,[string]$CaptureTranscript,[string]$OutputPath)
$ErrorActionPreference='Stop'
try {
  & $AuditScript -SessionId $ExpectedSession -CaptureTranscript $CaptureTranscript -ResultPath $OutputPath
} catch {
  if(-not(Test-Path -LiteralPath $OutputPath -PathType Leaf)){
    $failure=[ordered]@{
      Schema='whatsapp-region-ownership-audit/v1'
      Status='failedClosed'
      FailureCode=if($_.FullyQualifiedErrorId){[string]$_.FullyQualifiedErrorId}else{'unknown'}
      FailureLine=[int]$_.InvocationInfo.ScriptLineNumber
      FailureType=[string]$_.Exception.GetType().Name
      FailureMessage=[string]$_.Exception.Message
      ProviderContentRead=$false
      WhatsAppPrivateStorageRead=$false
      InputSent=$false
      WindowForegrounded=$false
    }|ConvertTo-Json -Compress
    [IO.File]::WriteAllText($OutputPath,$failure,[Text.UTF8Encoding]::new($false))
  }
  exit 1
}
'@
  [IO.File]::WriteAllText($runner,$runnerSource,[Text.UTF8Encoding]::new($false))
  $task='OSL-QA-WhatsApp-Region-Audit'
  $captureFlag=if($captureEnabled){'true'}else{'false'}
  $taskArguments="-NoProfile -NonInteractive -WindowStyle Hidden -ExecutionPolicy Bypass -File `"$runner`" -AuditScript `"$staged`" -ExpectedSession $SessionId -CaptureTranscript $captureFlag -OutputPath `"$result`""
  $action=New-ScheduledTaskAction -Execute 'powershell.exe' -Argument $taskArguments
  $principal=New-ScheduledTaskPrincipal -UserId $user -LogonType Interactive -RunLevel Limited
  $settings=New-ScheduledTaskSettingsSet -ExecutionTimeLimit ([TimeSpan]::FromMinutes(1)) -MultipleInstances IgnoreNew
  Register-ScheduledTask -TaskName $task -Action $action -Principal $principal -Settings $settings -Force|Out-Null
  try{
    Start-ScheduledTask -TaskName $task
    $deadline=[DateTime]::UtcNow.AddSeconds(45)
    while(-not(Test-Path -LiteralPath $result -PathType Leaf) -and [DateTime]::UtcNow -lt $deadline){Start-Sleep -Milliseconds 200}
    if(-not(Test-Path -LiteralPath $result -PathType Leaf)){
      $taskInfo=Get-ScheduledTaskInfo -TaskName $task -ErrorAction SilentlyContinue
      $taskCode=if($taskInfo){[uint32]$taskInfo.LastTaskResult}else{[uint32]::MaxValue}
      throw "interactive region audit timed out with task result $taskCode"
    }
    Get-Content -LiteralPath $result -Raw
  }finally{
    Unregister-ScheduledTask -TaskName $task -Confirm:$false -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $staged,$runner,$result -Force -ErrorAction SilentlyContinue
  }
  exit 0
}

Add-Type @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
public static class OslWhatsAppRegionAudit {
  public delegate bool EnumWindowsProc(IntPtr hwnd,IntPtr state);
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left,Top,Right,Bottom; }
  [StructLayout(LayoutKind.Sequential)] public struct POINT { public int X,Y; }
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumWindowsProc callback,IntPtr state);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hwnd);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hwnd,out uint pid);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hwnd,out RECT rect);
  [DllImport("user32.dll")] public static extern IntPtr WindowFromPoint(POINT point);
  [DllImport("user32.dll")] public static extern IntPtr GetAncestor(IntPtr hwnd,uint flags);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  public static List<IntPtr> VisibleForPid(uint expected){
    var result=new List<IntPtr>();
    EnumWindows((h,s)=>{uint pid;GetWindowThreadProcessId(h,out pid);if(pid==expected&&IsWindowVisible(h))result.Add(h);return true;},IntPtr.Zero);
    return result;
  }
}
'@

$wa=@(Get-CimInstance Win32_Process -Filter "Name = 'WhatsApp.Root.exe'" |
  Where-Object{[int]$_.SessionId -eq $SessionId})
if($wa.Count -ne 1){throw 'official WhatsApp process is unavailable or ambiguous'}
$windows=@([OslWhatsAppRegionAudit]::VisibleForPid([uint32]$wa[0].ProcessId))
if($windows.Count -ne 1){throw 'official WhatsApp main window is unavailable or ambiguous'}
$hwnd=$windows[0]
$rect=New-Object OslWhatsAppRegionAudit+RECT
if(-not[OslWhatsAppRegionAudit]::GetWindowRect($hwnd,[ref]$rect)){throw 'WhatsApp geometry unavailable'}
$width=$rect.Right-$rect.Left
$height=$rect.Bottom-$rect.Top
if($width -lt 720 -or $height -lt 420 -or $width -gt 4096 -or $height -gt 4096){throw 'WhatsApp geometry rejected'}
$sidebar=[Math]::Min(520,[Math]::Max(280,[int](($width*35)/100)))
$header=[Math]::Min(96,[Math]::Max(64,[int](($height*10)/100)))
$composer=[Math]::Min(104,[Math]::Max(58,[int](($height*11)/100)))
$regions=[ordered]@{
  account=@(0,0,$sidebar,$header)
  chat=@($sidebar,0,$width,$header)
  composer=@(($sidebar+8),($height-$composer),($width-8),($height-8))
  transcript=@(($sidebar+8),$header,($width-8),($height-$composer))
}
$foreground=[OslWhatsAppRegionAudit]::GetForegroundWindow()
$points=[ordered]@{}
foreach($name in $regions.Keys){
  $r=$regions[$name]
  $point=New-Object OslWhatsAppRegionAudit+POINT
  $point.X=$rect.Left+[int](($r[0]+$r[2])/2)
  $point.Y=$rect.Top+[int](($r[1]+$r[3])/2)
  $at=[OslWhatsAppRegionAudit]::WindowFromPoint($point)
  $root=[OslWhatsAppRegionAudit]::GetAncestor($at,2)
  [uint32]$ownerPid=0
  [void][OslWhatsAppRegionAudit]::GetWindowThreadProcessId($root,[ref]$ownerPid)
  $owner=Get-Process -Id $ownerPid -ErrorAction SilentlyContinue
  $points[$name]=[ordered]@{
    ExactWhatsAppRoot=($root -eq $hwnd)
    OwnerProcess=if($owner){[string]$owner.ProcessName}else{'unavailable'}
  }
}
$imageBase64=$null
if($captureEnabled){
  Add-Type -AssemblyName System.Drawing
  $r=$regions['transcript']
  $captureTop=[Math]::Max($r[1],$r[3]-260)
  $captureWidth=$r[2]-$r[0]
  $captureHeight=$r[3]-$captureTop
  $bitmap=[Drawing.Bitmap]::new($captureWidth,$captureHeight)
  $graphics=[Drawing.Graphics]::FromImage($bitmap)
  try{
    $graphics.CopyFromScreen($rect.Left+$r[0],$rect.Top+$captureTop,0,0,[Drawing.Size]::new($captureWidth,$captureHeight))
    $previewWidth=[Math]::Min(480,$captureWidth)
    $previewHeight=[Math]::Max(1,[int]($captureHeight*$previewWidth/$captureWidth))
    $preview=[Drawing.Bitmap]::new($previewWidth,$previewHeight)
    $previewGraphics=[Drawing.Graphics]::FromImage($preview)
    $previewGraphics.DrawImage($bitmap,0,0,$previewWidth,$previewHeight)
    $previewGraphics.Dispose()
    $stream=[IO.MemoryStream]::new()
    try{
      $codec=[Drawing.Imaging.ImageCodecInfo]::GetImageEncoders()|Where-Object{$_.MimeType -ceq'image/jpeg'}|Select-Object -First 1
      $encoderParameters=[Drawing.Imaging.EncoderParameters]::new(1)
      $encoderParameters.Param[0]=[Drawing.Imaging.EncoderParameter]::new([Drawing.Imaging.Encoder]::Quality,[long]15)
      $preview.Save($stream,$codec,$encoderParameters)
      $imageBase64=[Convert]::ToBase64String($stream.ToArray())
    }finally{$stream.Dispose();$preview.Dispose()}
  }finally{$graphics.Dispose();$bitmap.Dispose()}
}
$receipt=[ordered]@{
  Schema='whatsapp-region-ownership-audit/v1'
  Status='audited'
  Width=$width
  Height=$height
  Points=$points
  ImageBase64=$imageBase64
  ForegroundWindowUnchanged=([OslWhatsAppRegionAudit]::GetForegroundWindow() -eq $foreground)
  ProviderContentRead=$captureEnabled
  WhatsAppPrivateStorageRead=$false
  InputSent=$false
  WindowForegrounded=$false
}
$json=$receipt|ConvertTo-Json -Compress -Depth 5
[IO.File]::WriteAllText($ResultPath,$json,[Text.UTF8Encoding]::new($false))
