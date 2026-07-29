param(
  [Parameter(Mandatory)][ValidatePattern('^[a-f0-9]{64}$')][string]$OslExeSha256,
  [ValidateRange(1,128)][int]$SessionId=2,
  [string]$ResultPath=''
)
$ErrorActionPreference='Stop';Set-StrictMode -Version Latest
$captureStage='controller'
if($ResultPath){
  trap {
    $failure=[pscustomobject]@{Schema='whatsapp-structural-preview/v1';Status='failedClosed';FailureStage=$captureStage;ProviderTextReadable=$false;WindowForegrounded=$false;InputSent=$false;WhatsAppPrivateStorageRead=$false}|ConvertTo-Json -Compress
    $temporary="$ResultPath.tmp";[IO.File]::WriteAllText($temporary,$failure,[Text.UTF8Encoding]::new($false));Move-Item -LiteralPath $temporary -Destination $ResultPath -Force
    exit 1
  }
}
if(-not $ResultPath){
  $ownerCandidates=@(Get-CimInstance Win32_Process -Filter "Name = 'explorer.exe'"|Where-Object{[int]$_.SessionId -eq $SessionId})
  if($ownerCandidates.Count -eq 0){$ownerCandidates=@(Get-CimInstance Win32_Process -Filter "Name = 'WhatsApp.Root.exe'"|Where-Object{[int]$_.SessionId -eq $SessionId});$ownerCandidates+=@(Get-CimInstance Win32_Process -Filter "Name = 'OSL Privacy.exe'"|Where-Object{[int]$_.SessionId -eq $SessionId -and $_.ExecutablePath -and [string]::Equals([IO.Path]::GetFullPath([string]$_.ExecutablePath),'C:\Users\osltest\Desktop\OSL Privacy\OSL Privacy.exe',[StringComparison]::OrdinalIgnoreCase)})}
  if($ownerCandidates.Count -eq 0){throw 'interactive session owner evidence is unavailable'}
  $owners=@($ownerCandidates|ForEach-Object{$value=Invoke-CimMethod -InputObject $_ -MethodName GetOwner;if($value.ReturnValue -ne 0 -or $value.User -cne 'osltest' -or -not $value.Domain){throw 'interactive owner identity changed'};"$($value.Domain)\$($value.User)"}|Sort-Object -Unique)
  if($owners.Count -ne 1){throw 'interactive session owner is ambiguous'}
  $root='C:\Users\osltest\AppData\Local\OSL-QA\whatsapp-structural-preview-v1';[void](New-Item -ItemType Directory -Path $root -Force)
  $inherit=[Security.AccessControl.InheritanceFlags]'ContainerInherit,ObjectInherit';$propagate=[Security.AccessControl.PropagationFlags]::None
  $acl=[Security.AccessControl.DirectorySecurity]::new();$acl.SetAccessRuleProtection($true,$false)
  $acl.AddAccessRule([Security.AccessControl.FileSystemAccessRule]::new('SYSTEM',[Security.AccessControl.FileSystemRights]::FullControl,$inherit,$propagate,[Security.AccessControl.AccessControlType]::Allow))
  $acl.AddAccessRule([Security.AccessControl.FileSystemAccessRule]::new($owners[0],[Security.AccessControl.FileSystemRights]::Modify,$inherit,$propagate,[Security.AccessControl.AccessControlType]::Allow))
  Set-Acl -LiteralPath $root -AclObject $acl
  $staged=Join-Path $root 'capture.ps1';$runner=Join-Path $root 'runner.ps1';$result=Join-Path $root 'result.json';Copy-Item -LiteralPath $PSCommandPath -Destination $staged -Force;Remove-Item -LiteralPath $result -Force -ErrorAction SilentlyContinue
  $runnerSource=@'
param([string]$CaptureScript,[string]$ExpectedSha,[int]$ExpectedSession,[string]$OutputPath)
$ErrorActionPreference='Stop'
try {
  & $CaptureScript -OslExeSha256 $ExpectedSha -SessionId $ExpectedSession -ResultPath $OutputPath
} catch {
  if(-not(Test-Path -LiteralPath $OutputPath -PathType Leaf)){
    $failureCode=if($_.FullyQualifiedErrorId){[string]$_.FullyQualifiedErrorId}else{'unknown'}
    $failure=[pscustomobject]@{Schema='whatsapp-structural-preview/v1';Status='failedClosed';FailureStage='runner';FailureCode=$failureCode;ProviderTextReadable=$false;WindowForegrounded=$false;InputSent=$false;WhatsAppPrivateStorageRead=$false}|ConvertTo-Json -Compress
    [IO.File]::WriteAllText($OutputPath,$failure,[Text.UTF8Encoding]::new($false))
  }
  exit 1
}
'@
  [IO.File]::WriteAllText($runner,$runnerSource,[Text.UTF8Encoding]::new($false))
  $task='OSL-QA-WhatsApp-Structural-Preview';$arguments="-NoProfile -NonInteractive -WindowStyle Hidden -ExecutionPolicy Bypass -File `"$runner`" -CaptureScript `"$staged`" -ExpectedSha $OslExeSha256 -ExpectedSession $SessionId -OutputPath `"$result`""
  $action=New-ScheduledTaskAction -Execute 'powershell.exe' -Argument $arguments;$principal=New-ScheduledTaskPrincipal -UserId $owners[0] -LogonType Interactive -RunLevel Limited;$settings=New-ScheduledTaskSettingsSet -ExecutionTimeLimit ([TimeSpan]::FromMinutes(2)) -MultipleInstances IgnoreNew
  Register-ScheduledTask -TaskName $task -Action $action -Principal $principal -Settings $settings -Force|Out-Null
  try{Start-ScheduledTask -TaskName $task;$deadline=[DateTime]::UtcNow.AddSeconds(60);while(-not(Test-Path -LiteralPath $result -PathType Leaf) -and [DateTime]::UtcNow -lt $deadline){Start-Sleep -Milliseconds 250};if(-not(Test-Path -LiteralPath $result -PathType Leaf)){$taskInfo=Get-ScheduledTaskInfo -TaskName $task -ErrorAction SilentlyContinue;$taskCode=if($taskInfo){[uint32]$taskInfo.LastTaskResult}else{[uint32]::MaxValue};throw "structural preview timed out with task result $taskCode"};Get-Content -LiteralPath $result -Raw}finally{Unregister-ScheduledTask -TaskName $task -Confirm:$false -ErrorAction SilentlyContinue;Remove-Item -LiteralPath $staged,$runner,$result -Force -ErrorAction SilentlyContinue}
  exit 0
}
$captureStage='identity'
Add-Type -AssemblyName System.Drawing
Add-Type @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;
public static class OslWhatsAppStructuralCapture {
  public delegate bool EnumWindowsProc(IntPtr hwnd,IntPtr state);
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left,Top,Right,Bottom; }
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumWindowsProc callback,IntPtr state);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hwnd);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hwnd,out uint pid);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hwnd,out RECT rect);
  [DllImport("user32.dll",CharSet=CharSet.Unicode)] static extern int GetWindowText(IntPtr hwnd,StringBuilder value,int capacity);
  [DllImport("user32.dll")] public static extern IntPtr GetWindowDC(IntPtr hwnd);
  [DllImport("user32.dll")] public static extern int ReleaseDC(IntPtr hwnd,IntPtr dc);
  [DllImport("gdi32.dll")] public static extern bool BitBlt(IntPtr destination,int x,int y,int width,int height,IntPtr source,int sourceX,int sourceY,uint operation);
  public static List<IntPtr> ForPid(uint expected){var result=new List<IntPtr>();EnumWindows((h,s)=>{uint pid;GetWindowThreadProcessId(h,out pid);if(pid==expected&&IsWindowVisible(h))result.Add(h);return true;},IntPtr.Zero);return result;}
  public static int ExactTitleCount(uint expected,string title){var count=0;EnumWindows((h,s)=>{uint pid;GetWindowThreadProcessId(h,out pid);if(pid==expected&&IsWindowVisible(h)){var value=new StringBuilder(128);GetWindowText(h,value,value.Capacity);if(value.ToString()==title)count++;}return true;},IntPtr.Zero);return count;}
}
'@
$oslPath='C:\Users\osltest\Desktop\OSL Privacy\OSL Privacy.exe'
if(-not(Test-Path -LiteralPath $oslPath -PathType Leaf) -or (Get-FileHash -LiteralPath $oslPath -Algorithm SHA256).Hash.ToLowerInvariant() -cne $OslExeSha256){throw 'exact OSL executable identity mismatch'}
$processes=@(Get-CimInstance Win32_Process -Filter "Name = 'OSL Privacy.exe'"|Where-Object{
  $_.ExecutablePath -and
  [string]::Equals([IO.Path]::GetFullPath([string]$_.ExecutablePath),$oslPath,[StringComparison]::OrdinalIgnoreCase) -and
  [int]$_.SessionId -eq $SessionId -and
  [string]$_.CommandLine -cnotmatch '(?:^|\s)--osl-whatsapp-window-guardian-v1(?:\s|$)'
})
if($processes.Count -ne 1){throw 'exact OSL process is unavailable or ambiguous'}
$primary=Get-Process -Id ([int]$processes[0].ProcessId) -ErrorAction Stop
if($primary.MainWindowHandle -eq [IntPtr]::Zero){throw 'exact OSL window is unavailable or ambiguous'}
$windows=@($primary.MainWindowHandle)
$protectedOverlayCount=[OslWhatsAppStructuralCapture]::ExactTitleCount([uint32]$processes[0].ProcessId,'OSL protected WhatsApp composer')
$captureStage='geometry'
$rect=New-Object OslWhatsAppStructuralCapture+RECT;if(-not[OslWhatsAppStructuralCapture]::GetWindowRect($windows[0],[ref]$rect)){throw 'exact OSL geometry unavailable'}
$width=$rect.Right-$rect.Left;$height=$rect.Bottom-$rect.Top;if($width -lt 960 -or $height -lt 680 -or $width -gt 4096 -or $height -gt 2160){throw 'exact OSL geometry rejected'}
$captureStage='surface'
$full=[Drawing.Bitmap]::new($width,$height,[Drawing.Imaging.PixelFormat]::Format32bppArgb);$graphics=[Drawing.Graphics]::FromImage($full);$dc=$graphics.GetHdc();$source=[OslWhatsAppStructuralCapture]::GetWindowDC($windows[0])
try{if($source -eq [IntPtr]::Zero -or -not[OslWhatsAppStructuralCapture]::BitBlt($dc,0,0,$width,$height,$source,0,0,0x40CC0020)){throw 'background OSL render failed'}}finally{if($source -ne [IntPtr]::Zero){[void][OslWhatsAppStructuralCapture]::ReleaseDC($windows[0],$source)};$graphics.ReleaseHdc($dc);$graphics.Dispose()}
$captureStage='encode'
$cropWidth=[Math]::Min(936,$width);$cropLeft=0;$cropTop=[Math]::Min(38,[Math]::Max(0,$height-1));$cropHeight=[Math]::Min(60,$height-$cropTop)
$preview=[Drawing.Bitmap]::new($cropWidth,$cropHeight);$g=[Drawing.Graphics]::FromImage($preview);$g.DrawImage($full,[Drawing.Rectangle]::new(0,0,$cropWidth,$cropHeight),[Drawing.Rectangle]::new($cropLeft,$cropTop,$cropWidth,$cropHeight),[Drawing.GraphicsUnit]::Pixel);$g.Dispose();$full.Dispose()
$stream=[IO.MemoryStream]::new()
try{
  $codec=[Drawing.Imaging.ImageCodecInfo]::GetImageEncoders()|Where-Object{$_.MimeType -ceq'image/jpeg'}|Select-Object -First 1
  $parameters=[Drawing.Imaging.EncoderParameters]::new(1)
  $parameters.Param[0]=[Drawing.Imaging.EncoderParameter]::new([Drawing.Imaging.Encoder]::Quality,[long]30)
  $preview.Save($stream,$codec,$parameters);$bytes=$stream.ToArray()
}finally{$preview.Dispose();$stream.Dispose()}
$json=[pscustomobject]@{Schema='whatsapp-structural-preview/v1';Status='captured';Width=$width;Height=$height;ProtectedOverlayWindowCount=$protectedOverlayCount;PreviewWidth=$cropWidth;PreviewHeight=$cropHeight;CropRegion='oslTrustedStripRight';ImageFormat='jpeg';ImageBase64=[Convert]::ToBase64String($bytes);ProviderTextReadable=$false;WindowForegrounded=$false;InputSent=$false;WhatsAppPrivateStorageRead=$false}|ConvertTo-Json -Compress
[Array]::Clear($bytes,0,$bytes.Length);$temporary="$ResultPath.tmp";[IO.File]::WriteAllText($temporary,$json,[Text.UTF8Encoding]::new($false));Move-Item -LiteralPath $temporary -Destination $ResultPath -Force
