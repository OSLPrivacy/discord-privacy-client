param(
  [ValidateRange(64,4096)][int]$MaxNodes=2048,
  [ValidateRange(1,128)][int]$SessionId=2,
  [string]$ResultPath=''
)
$ErrorActionPreference='Stop';Set-StrictMode -Version Latest
if(-not $ResultPath){
  $root='C:\ProgramData\OSL-QA\whatsapp-msaa-probe-v1';[void](New-Item -ItemType Directory -Path $root -Force)
  $staged=Join-Path $root 'probe.ps1';$result=Join-Path $root 'result.json';$phase="$result.phase";Copy-Item -LiteralPath $PSCommandPath -Destination $staged -Force;Remove-Item -LiteralPath $result,$phase -Force -ErrorAction SilentlyContinue
  $explorer=@(Get-CimInstance Win32_Process -Filter "Name = 'explorer.exe'"|Where-Object{[int]$_.SessionId -eq $SessionId});if($explorer.Count -ne 1){throw 'interactive session is unavailable or ambiguous'}
  $owner=Invoke-CimMethod -InputObject $explorer[0] -MethodName GetOwner;if($owner.ReturnValue -ne 0 -or $owner.User -cne 'osltest' -or -not $owner.Domain){throw 'interactive owner identity changed'}
  $task='OSL-QA-WhatsApp-MSAA-Probe';$arguments="-NoProfile -NonInteractive -WindowStyle Hidden -ExecutionPolicy Bypass -File `"$staged`" -MaxNodes $MaxNodes -SessionId $SessionId -ResultPath `"$result`""
  $action=New-ScheduledTaskAction -Execute 'powershell.exe' -Argument $arguments;$principal=New-ScheduledTaskPrincipal -UserId "$($owner.Domain)\$($owner.User)" -LogonType Interactive -RunLevel Limited;$settings=New-ScheduledTaskSettingsSet -ExecutionTimeLimit ([TimeSpan]::FromMinutes(2)) -MultipleInstances IgnoreNew
  Register-ScheduledTask -TaskName $task -Action $action -Principal $principal -Settings $settings -Force|Out-Null
  try{Start-ScheduledTask -TaskName $task;$deadline=[DateTime]::UtcNow.AddSeconds(60);while(-not(Test-Path -LiteralPath $result -PathType Leaf) -and [DateTime]::UtcNow -lt $deadline){Start-Sleep -Milliseconds 250};if(-not(Test-Path -LiteralPath $result -PathType Leaf)){$last=if(Test-Path -LiteralPath $phase){[string](Get-Content -LiteralPath $phase -Raw)}else{'notStarted'};[pscustomobject]@{Schema='whatsapp-msaa-metadata-probe/v1';Status='timedOut';LastPhase=$last;NamesReturned=$false;ValuesReturned=$false;ProviderContentReturned=$false;InputSent=$false;WindowForegrounded=$false;WhatsAppPrivateStorageRead=$false}|ConvertTo-Json -Compress}else{Get-Content -LiteralPath $result -Raw}}finally{Unregister-ScheduledTask -TaskName $task -Confirm:$false -ErrorAction SilentlyContinue;Remove-Item -LiteralPath $staged,$result,$phase -Force -ErrorAction SilentlyContinue}
  exit 0
}
Add-Type -AssemblyName Accessibility
$phasePath="$ResultPath.phase";[IO.File]::WriteAllText($phasePath,'compiling')
$source=@'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;
using Accessibility;
public static class OslWhatsAppMsaaProbe {
  public delegate bool EnumWindowsProc(IntPtr hwnd, IntPtr lparam);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumWindowsProc callback, IntPtr lparam);
  [DllImport("user32.dll")] public static extern bool EnumChildWindows(IntPtr parent, EnumWindowsProc callback, IntPtr lparam);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint pid);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetClassName(IntPtr hwnd, StringBuilder value, int count);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr hwnd, StringBuilder value, int count);
  [DllImport("oleacc.dll")] public static extern int AccessibleObjectFromWindow(IntPtr hwnd, uint objectId, ref Guid iid, [MarshalAs(UnmanagedType.Interface)] out object result);
  [DllImport("oleacc.dll")] public static extern int AccessibleChildren(IAccessible container, int start, int count, [Out, MarshalAs(UnmanagedType.LPArray, SizeParamIndex=2)] object[] children, out int obtained);
  public static List<IntPtr> WindowsFor(HashSet<uint> pids) {
    var result=new List<IntPtr>();EnumWindowsProc add=(h,l)=>{uint p;GetWindowThreadProcessId(h,out p);if(pids.Contains(p))result.Add(h);EnumChildWindows(h,(c,x)=>{uint q;GetWindowThreadProcessId(c,out q);if(pids.Contains(q))result.Add(c);return true;},IntPtr.Zero);return true;};EnumWindows(add,IntPtr.Zero);return result;
  }
  public static List<IntPtr> AllWindows() { var result=new List<IntPtr>();EnumWindowsProc add=(h,l)=>{result.Add(h);EnumChildWindows(h,(c,x)=>{result.Add(c);return true;},IntPtr.Zero);return true;};EnumWindows(add,IntPtr.Zero);return result; }
  public static string ClassName(IntPtr hwnd){var b=new StringBuilder(256);GetClassName(hwnd,b,b.Capacity);return b.ToString();}
  public static string Title(IntPtr hwnd){var b=new StringBuilder(256);GetWindowText(hwnd,b,b.Capacity);return b.ToString();}
  public static uint Pid(IntPtr hwnd){uint p;GetWindowThreadProcessId(hwnd,out p);return p;}
  public static IAccessible Root(IntPtr hwnd){object value;var iid=new Guid("618736E0-3C3D-11CF-810C-00AA00389B71");return AccessibleObjectFromWindow(hwnd,0xFFFFFFFC,ref iid,out value)>=0?value as IAccessible:null;}
}
'@
Add-Type -TypeDefinition $source -ReferencedAssemblies Accessibility
[IO.File]::WriteAllText($phasePath,'findingWindow')
$processes=@(Get-CimInstance Win32_Process|Where-Object{$_.ExecutablePath -and ([string]$_.ExecutablePath).StartsWith('C:\Program Files\WindowsApps\5319275A.WhatsAppDesktop_',[StringComparison]::OrdinalIgnoreCase)})
if($processes.Count -eq 0){throw 'official WhatsApp process evidence is unavailable'}
$pids=[Collections.Generic.HashSet[uint32]]::new();foreach($process in $processes){[void]$pids.Add([uint32]$process.ProcessId)}
$windows=@([OslWhatsAppMsaaProbe]::AllWindows()|Where-Object{[OslWhatsAppMsaaProbe]::ClassName($_) -ceq 'WinUIDesktopWin32WindowClass' -and [OslWhatsAppMsaaProbe]::Title($_) -ceq 'WhatsApp'})
if($windows.Count -ne 1){throw 'exact WhatsApp claimed window is unavailable or ambiguous'}
$windowPid=[OslWhatsAppMsaaProbe]::Pid($windows[0]);$windowProcess=Get-CimInstance Win32_Process -Filter "ProcessId = $windowPid"
if(-not $windowProcess.ExecutablePath -or -not ([string]$windowProcess.ExecutablePath).StartsWith('C:\Program Files\WindowsApps\5319275A.WhatsAppDesktop_',[StringComparison]::OrdinalIgnoreCase)){throw 'exact WhatsApp window process identity changed'}
[IO.File]::WriteAllText($phasePath,'acquiringRoot')
$results=@();$total=0
foreach($hwnd in $windows){
  $root=[OslWhatsAppMsaaProbe]::Root($hwnd);if(-not $root){continue}
  [IO.File]::WriteAllText($phasePath,'traversingRoot')
  $roles=@{};$named=0;$bounded=0;$queue=[Collections.Generic.Queue[Accessibility.IAccessible]]::new();$queue.Enqueue($root);$nodes=0
  while($queue.Count -gt 0 -and $total -lt $MaxNodes){
    $current=$queue.Dequeue();$nodes++;$total++
    [IO.File]::WriteAllText($phasePath,'readingRole')
    try{$role=[int]$current.accRole(0);$key=[string]$role;if($roles.ContainsKey($key)){$roles[$key]++}else{$roles[$key]=1}}catch{}
    [IO.File]::WriteAllText($phasePath,'readingNamePresence')
    try{if(-not[string]::IsNullOrWhiteSpace([string]$current.accName(0))){$named++}}catch{}
    [IO.File]::WriteAllText($phasePath,'readingBounds')
    try{$x=0;$y=0;$w=0;$h=0;$current.accLocation([ref]$x,[ref]$y,[ref]$w,[ref]$h,0);if($w -gt 0 -and $h -gt 0){$bounded++}}catch{}
    [IO.File]::WriteAllText($phasePath,'readingChildren')
    try{$count=[int]$current.accChildCount;if($count -gt 0){$children=New-Object object[] $count;$obtained=0;if([OslWhatsAppMsaaProbe]::AccessibleChildren($current,0,$count,$children,[ref]$obtained) -ge 0){for($i=0;$i-lt $obtained;$i++){if($children[$i] -is [Accessibility.IAccessible]){$queue.Enqueue($children[$i])}}}}}catch{}
  }
  $results+=,[pscustomobject]@{Class=[OslWhatsAppMsaaProbe]::ClassName($hwnd);Provider=$true;Nodes=$nodes;NamedNodes=$named;BoundedNodes=$bounded;Roles=$roles}
}
$json=[pscustomobject]@{Schema='whatsapp-msaa-metadata-probe/v1';OfficialProcessCount=$processes.Count;WindowCount=$windows.Count;ProviderWindowCount=$results.Count;TotalNodes=$total;Windows=$results;NamesReturned=$false;ValuesReturned=$false;ProviderContentReturned=$false;InputSent=$false;WindowForegrounded=$false;WhatsAppPrivateStorageRead=$false}|ConvertTo-Json -Compress -Depth 8
if($ResultPath){[IO.File]::WriteAllText($phasePath,'complete');$temporary="$ResultPath.tmp";[IO.File]::WriteAllText($temporary,$json,[Text.UTF8Encoding]::new($false));Move-Item -LiteralPath $temporary -Destination $ResultPath -Force}else{$json}
