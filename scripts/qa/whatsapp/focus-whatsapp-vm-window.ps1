param(
 [Parameter(Mandatory)][ValidateSet(1,2)][int]$ClientNumber,
 [Parameter(Mandatory)][ValidatePattern('^[a-z0-9-]{8,80}$')][string]$InvocationId,
 [Parameter(Mandatory)][ValidateRange(1,128)][int]$SessionId
)
$ErrorActionPreference='Stop';Set-StrictMode -Version Latest
$explorer=@(Get-CimInstance Win32_Process -Filter "Name = 'explorer.exe'"|Where-Object{[int]$_.SessionId-eq$SessionId});if($explorer.Count-ne1){throw'interactive session ambiguous'}
$owner=Invoke-CimMethod -InputObject $explorer[0] -MethodName GetOwner;if($owner.ReturnValue-ne0-or$owner.User-cne'osltest'-or-not$owner.Domain){throw'interactive owner invalid'};$user="$($owner.Domain)\$($owner.User)"
$result="C:\Users\osltest\AppData\Local\Temp\$InvocationId.result.json";$leafPath="C:\Users\osltest\AppData\Local\Temp\$InvocationId.leaf.ps1";$task="OSL-WA-Focus-$InvocationId"
$leaf=@'
param([int]$ClientNumber,[string]$InvocationId,[int]$SessionId,[string]$ResultPath)
$ErrorActionPreference='Stop';Set-StrictMode -Version Latest
Add-Type @"
using System;using System.Collections.Generic;using System.Runtime.InteropServices;using System.Text;
public static class OslWaFocus{
 public delegate bool EnumProc(IntPtr h,IntPtr s);
 [DllImport("user32.dll")]static extern bool EnumWindows(EnumProc c,IntPtr s);
 [DllImport("user32.dll",CharSet=CharSet.Unicode)]static extern int GetClassName(IntPtr h,StringBuilder b,int n);
 [DllImport("user32.dll",CharSet=CharSet.Unicode)]static extern int GetWindowText(IntPtr h,StringBuilder b,int n);
 [DllImport("user32.dll")]public static extern uint GetWindowThreadProcessId(IntPtr h,out uint p);
 [DllImport("user32.dll")]public static extern bool SetForegroundWindow(IntPtr h);
 public static IntPtr[] Candidates(){var r=new List<IntPtr>();EnumWindows((h,s)=>{var c=new StringBuilder(128);var t=new StringBuilder(128);GetClassName(h,c,c.Capacity);GetWindowText(h,t,t.Capacity);if(c.ToString()=="WinUIDesktopWin32WindowClass"&&t.ToString()=="WhatsApp")r.Add(h);return true;},IntPtr.Zero);return r.ToArray();}
}
"@
$value=@{Schema='whatsapp-vm-focus/v1';Status='failedClosed';Terminal=$true;ClientNumber=$ClientNumber;InvocationId=$InvocationId;ProviderContentRead=$false;ProviderInputSent=$false;ProviderStorageRead=$false;LocalRdpForegrounded=$false}
try{
 $package=@(Get-AppxPackage -Name '5319275A.WhatsAppDesktop');if($package.Count-ne1){throw'package ambiguous'};$prefix=[IO.Path]::GetFullPath([string]$package[0].InstallLocation).TrimEnd('\')+'\'
 $valid=@();foreach($h in[OslWaFocus]::Candidates()){[uint32]$pid=0;[void][OslWaFocus]::GetWindowThreadProcessId($h,[ref]$pid);$p=Get-Process -Id $pid -ErrorAction Stop;if($p.SessionId-eq$SessionId-and$p.Path-and[IO.Path]::GetFullPath($p.Path).StartsWith($prefix,[StringComparison]::OrdinalIgnoreCase)){$valid+=$h}}
 if($valid.Count-ne1){throw'exact official WhatsApp window ambiguous'}
 if(-not[OslWaFocus]::SetForegroundWindow($valid[0])){throw'VM rejected WhatsApp foreground'}
 $value.Status='focused';$value.VmWhatsAppForegrounded=$true
}catch{$value.Status='failedClosed'}
$tmp="$ResultPath.tmp";$value|ConvertTo-Json -Compress|Set-Content -LiteralPath $tmp -Encoding UTF8;if(Test-Path $ResultPath){Remove-Item $ResultPath -Force};[IO.File]::Move($tmp,$ResultPath);if($value.Status-cne'focused'){exit 1}
'@
$leaf|Set-Content -LiteralPath $leafPath -Encoding UTF8
$action=New-ScheduledTaskAction -Execute 'powershell.exe' -Argument "-NoProfile -NonInteractive -ExecutionPolicy Bypass -File `"$leafPath`" -ClientNumber $ClientNumber -InvocationId $InvocationId -SessionId $SessionId -ResultPath `"$result`""
$principal=New-ScheduledTaskPrincipal -UserId $user -LogonType Interactive -RunLevel Limited
Register-ScheduledTask -TaskName $task -Action $action -Principal $principal -Settings(New-ScheduledTaskSettingsSet -ExecutionTimeLimit([TimeSpan]::FromSeconds(30)))-Force|Out-Null
try{Start-ScheduledTask -TaskName $task;$deadline=[DateTime]::UtcNow.AddSeconds(15);do{if(Test-Path $result){$raw=Get-Content $result -Raw;$raw;exit 0};Start-Sleep -Milliseconds 100}while([DateTime]::UtcNow-lt$deadline);throw'focus task timed out'}finally{Unregister-ScheduledTask -TaskName $task -Confirm:$false -ErrorAction SilentlyContinue}
