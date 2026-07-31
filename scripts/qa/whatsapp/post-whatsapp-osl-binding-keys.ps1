param(
  [Parameter(Mandatory)][ValidateSet(1,2)][int]$ClientNumber,
  [Parameter(Mandatory)][ValidatePattern('^[a-z0-9-]{8,80}$')][string]$InvocationId,
  [Parameter(Mandatory)][ValidatePattern('^[0-9a-f]{64}$')][string]$OslExeSha256,
  [Parameter(Mandatory)][ValidateRange(1,128)][int]$SessionId,
  [Parameter(Mandatory)][ValidateSet('OpenAndBind','RetryAndBind','DirectClick','DirectBindFlow')][string]$Sequence,
  [Parameter(Mandatory)][ValidateRange(0,4095)][int]$WindowX,
  [Parameter(Mandatory)][ValidateRange(0,1023)][int]$WindowY
)
$ErrorActionPreference='Stop';Set-StrictMode -Version Latest
$oslPath=[IO.Path]::GetFullPath('C:\Users\osltest\Desktop\OSL Privacy\OSL Privacy.exe')
$resultPath=[IO.Path]::GetFullPath("C:\Users\osltest\AppData\Local\Temp\$InvocationId.result.json")
$leafPath=[IO.Path]::GetFullPath("C:\Users\osltest\AppData\Local\Temp\$InvocationId.leaf.ps1")
$taskName="OSL-WA-Keys-$InvocationId"
if((Get-FileHash -LiteralPath $oslPath -Algorithm SHA256).Hash.ToLowerInvariant()-cne$OslExeSha256){throw'exact OSL QA executable is unavailable'}
$explorers=@(Get-CimInstance Win32_Process -Filter "Name = 'explorer.exe'"|Where-Object{[int]$_.SessionId-eq$SessionId})
if($explorers.Count-ne1){throw'interactive Explorer session is unavailable or ambiguous'}
$owner=Invoke-CimMethod -InputObject $explorers[0] -MethodName GetOwner
if($owner.ReturnValue-ne0-or$owner.User-cne'osltest'-or-not$owner.Domain){throw'interactive session owner is invalid'}
$interactiveUser="$($owner.Domain)\$($owner.User)"
$leaf=@'
param([string]$OslPath,[string]$OslExeSha256,[int]$SessionId,[int]$ClientNumber,[string]$InvocationId,[string]$ResultPath,[string]$Sequence,[int]$WindowX,[int]$WindowY)
$ErrorActionPreference='Stop';Set-StrictMode -Version Latest
Add-Type @"
using System;using System.Collections.Generic;using System.Runtime.InteropServices;using System.Text;
public static class OslBindingKeys{
 public delegate bool EnumProc(IntPtr h,IntPtr s);
 [DllImport("user32.dll")]static extern bool EnumChildWindows(IntPtr p,EnumProc c,IntPtr s);
 [DllImport("user32.dll")]static extern bool IsWindowVisible(IntPtr h);
 [DllImport("user32.dll",CharSet=CharSet.Unicode)]static extern int GetClassName(IntPtr h,StringBuilder v,int n);
 [DllImport("user32.dll")]public static extern uint GetWindowThreadProcessId(IntPtr h,out uint p);
 [DllImport("user32.dll")]public static extern bool PostMessage(IntPtr h,uint m,IntPtr w,IntPtr l);
 [DllImport("user32.dll")]public static extern bool GetWindowRect(IntPtr h,out RECT r);
 [DllImport("user32.dll")]public static extern bool ScreenToClient(IntPtr h,ref POINT p);
 [DllImport("user32.dll")]public static extern IntPtr GetForegroundWindow();
 [StructLayout(LayoutKind.Sequential)]public struct RECT{public int Left,Top,Right,Bottom;}
 [StructLayout(LayoutKind.Sequential)]public struct POINT{public int X,Y;}
 public static IntPtr[] Renderers(IntPtr root){var r=new List<IntPtr>();EnumChildWindows(root,(h,s)=>{var b=new StringBuilder(128);GetClassName(h,b,b.Capacity);if(IsWindowVisible(h)&&b.ToString()=="Chrome_RenderWidgetHostHWND")r.Add(h);return true;},IntPtr.Zero);return r.ToArray();}
 public static bool Key(IntPtr h,int vk,int scan){var d=(IntPtr)(1|(scan<<16));var u=(IntPtr)(1|(scan<<16)|(1<<30)|unchecked((int)0x80000000));return PostMessage(h,0x100,(IntPtr)vk,d)&&PostMessage(h,0x101,(IntPtr)vk,u);}
 public static bool Click(IntPtr h,int screenX,int screenY){var p=new POINT{X=screenX,Y=screenY};if(!ScreenToClient(h,ref p))return false;var l=(IntPtr)((p.X&0xffff)|((p.Y&0xffff)<<16));return PostMessage(h,0x200,IntPtr.Zero,l)&&PostMessage(h,0x201,(IntPtr)1,l)&&PostMessage(h,0x202,IntPtr.Zero,l);}
}
"@
function Receipt([string]$Status){
 $value=@{Schema='whatsapp-osl-binding-keys/v1';Status=$Status;Terminal=$true;ClientNumber=$ClientNumber;InvocationId=$InvocationId;OslExeSha256=$OslExeSha256;ExactOslRendererOnly=$true;ProviderInputSent=$false;ProviderContentRead=$false;ProviderStorageRead=$false;WindowForegrounded=$false}
 $tmp="$ResultPath.tmp";$value|ConvertTo-Json -Compress|Set-Content -LiteralPath $tmp -Encoding UTF8;if(Test-Path -LiteralPath $ResultPath){Remove-Item -LiteralPath $ResultPath -Force};[IO.File]::Move($tmp,$ResultPath)
}
try{
 if((Get-FileHash -LiteralPath $OslPath -Algorithm SHA256).Hash.ToLowerInvariant()-cne$OslExeSha256){throw'hash changed'}
 $p=@(Get-Process -Name 'OSL Privacy'|Where-Object{$_.SessionId-eq$SessionId-and$_.Path-and[IO.Path]::GetFullPath($_.Path).Equals($OslPath,[StringComparison]::OrdinalIgnoreCase)})
 if($p.Count-ne1-or$p[0].MainWindowHandle-eq[IntPtr]::Zero){throw'OSL window ambiguous'}
 $r=@([OslBindingKeys]::Renderers($p[0].MainWindowHandle));if($r.Count-ne1){throw'OSL renderer ambiguous'}
 [uint32]$rendererPid=0;[void][OslBindingKeys]::GetWindowThreadProcessId($r[0],[ref]$rendererPid)
 $rp=Get-Process -Id $rendererPid;if($rp.SessionId-ne$SessionId-or[IO.Path]::GetFileName($rp.Path)-cne'msedgewebview2.exe'){throw'renderer invalid'}
 $foregroundBefore=[OslBindingKeys]::GetForegroundWindow()
 if($Sequence-ceq'DirectClick'){
  $rect=New-Object OslBindingKeys+RECT;if(-not[OslBindingKeys]::GetWindowRect($p[0].MainWindowHandle,[ref]$rect)){throw'OSL geometry unavailable'}
  if(-not[OslBindingKeys]::Click($r[0],$rect.Left+$WindowX,$rect.Top+$WindowY)){throw'OSL click post rejected'}
  Start-Sleep -Milliseconds 500
  Receipt 'posted'
  exit 0
 }
 if($Sequence-ceq'DirectBindFlow'){
  $rect=New-Object OslBindingKeys+RECT;if(-not[OslBindingKeys]::GetWindowRect($p[0].MainWindowHandle,[ref]$rect)){throw'OSL geometry unavailable'}
  $width=$rect.Right-$rect.Left;$height=$rect.Bottom-$rect.Top
  if($width-lt960-or$height-lt680){throw'OSL geometry rejected'}
  function ClickAt([int]$x,[int]$y,[int]$wait){
   if(-not[OslBindingKeys]::Click($r[0],$rect.Left+$x,$rect.Top+$y)){throw'OSL click post rejected'}
   Start-Sleep -Milliseconds $wait
  }
  ClickAt 620 75 500
  ClickAt ($width-350) 400 3500
  ClickAt ($width-390) 280 400
  ClickAt ($width-350) 350 3500
  if([OslBindingKeys]::GetForegroundWindow()-ne$foregroundBefore){throw'foreground changed during OSL-only binding flow'}
  Receipt 'posted'
  exit 0
 }
 function Key([int]$vk,[int]$scan){if(-not[OslBindingKeys]::Key($r[0],$vk,$scan)){throw'OSL key rejected'};Start-Sleep -Milliseconds 250}
 if($Sequence-ceq'OpenAndBind'){Key 0x0D 0x1C}
 Key 0x0D 0x1C
 Start-Sleep -Seconds 2
 Key 0x20 0x39
 Key 0x09 0x0F
 Key 0x0D 0x1C
 Start-Sleep -Seconds 2
 Receipt 'posted'
}catch{Receipt 'failedClosed';exit 1}
'@
if(Test-Path -LiteralPath $resultPath){Get-Content -LiteralPath $resultPath -Raw;exit 0}
$leaf|Set-Content -LiteralPath $leafPath -Encoding UTF8
$args="-NoProfile -NonInteractive -ExecutionPolicy Bypass -File `"$leafPath`" -OslPath `"$oslPath`" -OslExeSha256 $OslExeSha256 -SessionId $SessionId -ClientNumber $ClientNumber -InvocationId $InvocationId -ResultPath `"$resultPath`" -Sequence $Sequence -WindowX $WindowX -WindowY $WindowY"
$action=New-ScheduledTaskAction -Execute 'powershell.exe' -Argument $args
$principal=New-ScheduledTaskPrincipal -UserId $interactiveUser -LogonType Interactive -RunLevel Limited
Register-ScheduledTask -TaskName $taskName -Action $action -Principal $principal -Settings (New-ScheduledTaskSettingsSet -ExecutionTimeLimit([TimeSpan]::FromSeconds(30))) -Force|Out-Null
try{
 Start-ScheduledTask -TaskName $taskName;$deadline=[DateTime]::UtcNow.AddSeconds(15)
 do{if(Test-Path -LiteralPath $resultPath){$raw=Get-Content -LiteralPath $resultPath -Raw;$raw;exit 0};Start-Sleep -Milliseconds 100}while([DateTime]::UtcNow-lt$deadline)
 throw'OSL binding key task timed out'
}finally{Unregister-ScheduledTask -TaskName $taskName -Confirm:$false -ErrorAction SilentlyContinue}
