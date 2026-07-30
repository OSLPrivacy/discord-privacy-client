param(
  [Parameter(Mandatory)][ValidateSet('Inventory','ClaimExactWindow','AlreadyRunningAccessibilityBench','CaptureSafeChrome')][string]$Action,
  [Parameter(Mandatory)][string]$OslExePath,
  [Parameter(Mandatory)][ValidatePattern('^[0-9a-fA-F]{64}$')][string]$OslExeSha256,
  [Parameter(Mandatory)][ValidateRange(1,128)][int]$SessionId,
  [Parameter(Mandatory)][string]$SignalExePath,
  [Parameter(Mandatory)][ValidatePattern('^[A-Za-z0-9._-]{1,48}$')][string]$CaseId,
  [uri]$UploadUri,
  [ValidateSet('On','Off')][string]$Visibility='On',
  [ValidateRange(10,180)][int]$TimeoutSeconds=60
)
$ErrorActionPreference='Stop';Set-StrictMode -Version Latest;Add-Type -AssemblyName UIAutomationClient;Add-Type -AssemblyName UIAutomationTypes;Add-Type -AssemblyName System.Drawing;Add-Type -AssemblyName System.Net.Http
if(-not ('OslQaSignalWindows' -as [type])){Add-Type @"
using System;using System.Collections.Generic;using System.Runtime.InteropServices;using System.Text;
public static class OslQaSignalWindows { public delegate bool EnumWindowsProc(IntPtr h,IntPtr s);[StructLayout(LayoutKind.Sequential)]public struct RECT{public int Left,Top,Right,Bottom;}[DllImport("user32.dll")]public static extern bool EnumWindows(EnumWindowsProc c,IntPtr s);[DllImport("user32.dll")]public static extern bool IsWindowVisible(IntPtr h);[DllImport("user32.dll")]public static extern uint GetWindowThreadProcessId(IntPtr h,out uint p);[DllImport("user32.dll",CharSet=CharSet.Unicode)]public static extern int GetWindowText(IntPtr h,StringBuilder t,int n);[DllImport("user32.dll",CharSet=CharSet.Unicode)]public static extern int GetClassName(IntPtr h,StringBuilder t,int n);[DllImport("user32.dll")]public static extern bool GetWindowRect(IntPtr h,out RECT r);[DllImport("user32.dll")]public static extern bool PrintWindow(IntPtr h,IntPtr dc,uint flags); }
"@}
function Get-ExactProcesses([string]$name,[string]$path,[switch]$Primary){@(Get-CimInstance Win32_Process -Filter "Name = '$name'"|Where-Object{[int]$_.SessionId -eq $SessionId -and $_.ExecutablePath -and [string]::Equals([IO.Path]::GetFullPath([string]$_.ExecutablePath),$path,[StringComparison]::OrdinalIgnoreCase) -and (-not $Primary -or ([string]$_.CommandLine -notmatch '(?:^|\s)--osl-signal-window-guardian-v1(?:\s|$)'))})}
function Get-Windows([Collections.Generic.HashSet[uint32]]$pids){$found=@();$callback=[OslQaSignalWindows+EnumWindowsProc]{param($h,$s)[uint32]$candidatePid=0;[void][OslQaSignalWindows]::GetWindowThreadProcessId($h,[ref]$candidatePid);if($pids.Contains($candidatePid) -and [OslQaSignalWindows]::IsWindowVisible($h)){$title=[Text.StringBuilder]::new(512);$class=[Text.StringBuilder]::new(256);[void][OslQaSignalWindows]::GetWindowText($h,$title,$title.Capacity);[void][OslQaSignalWindows]::GetClassName($h,$class,$class.Capacity);$r=New-Object OslQaSignalWindows+RECT;if([OslQaSignalWindows]::GetWindowRect($h,[ref]$r)){$script:windowRecords+=,[pscustomobject]@{Handle=$h;Pid=$candidatePid;Title=$title.ToString();Class=$class.ToString();X=$r.Left;Y=$r.Top;Width=$r.Right-$r.Left;Height=$r.Bottom-$r.Top}}};return $true};$script:windowRecords=@();[void][OslQaSignalWindows]::EnumWindows($callback,[IntPtr]::Zero);return @($script:windowRecords)}
$oslPath=[IO.Path]::GetFullPath($OslExePath);$signalPath=[IO.Path]::GetFullPath($SignalExePath)
if(-not(Test-Path $oslPath -PathType Leaf) -or (Get-FileHash $oslPath -Algorithm SHA256).Hash.ToLowerInvariant() -cne $OslExeSha256.ToLowerInvariant()){throw 'exact OSL executable is missing or hash-mismatched'}
$osl=@(Get-ExactProcesses 'OSL Privacy.exe' $oslPath -Primary);if($osl.Count -ne 1){throw 'exact OSL primary process is unavailable or ambiguous'};$signal=@(Get-ExactProcesses 'Signal.exe' $signalPath);if($signal.Count -lt 1){throw 'exact Signal process set is empty'}
$signalPids=[Collections.Generic.HashSet[uint32]]::new();$signal|ForEach-Object{[void]$signalPids.Add([uint32]$_.ProcessId)};$allSignalWindows=@(Get-Windows $signalPids)
$oslPids=[Collections.Generic.HashSet[uint32]]::new();[void]$oslPids.Add([uint32]$osl[0].ProcessId);$allOslWindows=@(Get-Windows $oslPids)
if($Action -ceq 'Inventory'){
  $safeSignalWindows=@($allSignalWindows|ForEach-Object{[pscustomobject]@{Class=$_.Class;TitleLength=$_.Title.Length;TitleIsExactSignal=($_.Title -ceq 'Signal');Width=[int]$_.Width;Height=[int]$_.Height}})
  $safeOslWindows=@($allOslWindows|ForEach-Object{[pscustomobject]@{Class=$_.Class;TitleLength=$_.Title.Length;TitleIsExactQa=($_.Title -ceq 'OSL Signal QA');Width=[int]$_.Width;Height=[int]$_.Height}})
  [pscustomobject]@{Ok=$true;Action=$Action;CaseId=$CaseId;ExactOslProcessCount=1;VisibleOslWindowCount=$allOslWindows.Count;OslWindows=$safeOslWindows;ExactSignalProcessCount=$signal.Count;VisibleSignalWindowCount=$allSignalWindows.Count;SignalWindows=$safeSignalWindows;ContentInspected=$false;ForegroundChanged=$false}|ConvertTo-Json -Depth 5 -Compress
  exit 0
}
$signalWindows=@($allSignalWindows|Where-Object{$_.Class -ceq 'Chrome_WidgetWin_1' -and $_.Title -ceq 'Signal'})
if($signalWindows.Count -ne 1){throw 'exact visible Signal Desktop window is unavailable or ambiguous'};$window=$signalWindows[0];if($window.Width -lt 320 -or $window.Height -lt 240){throw 'exact Signal window geometry is invalid'}
$oslWindows=@($allOslWindows|Where-Object{$_.Class -ceq 'Tauri Window' -and $_.Title -ceq 'OSL Signal QA' -and $_.Width -ge 320 -and $_.Height -ge 240});if($oslWindows.Count -ne 1){throw 'exact visible OSL window is unavailable or ambiguous'}
if($Action -ceq 'CaptureSafeChrome'){
  if(-not $UploadUri -or $UploadUri.Scheme -cne 'https' -or $UploadUri.Query -or $UploadUri.Fragment){throw 'safe screenshot upload URI must be query-free HTTPS'}
  $width=[int]$oslWindows[0].Width;$height=[int]$oslWindows[0].Height;if($width -gt 3840 -or $height -gt 2160){throw 'OSL window geometry is outside safe bounds'}
  $safe=$null;$bytes=$null;$full=[Drawing.Bitmap]::new($width,$height,[Drawing.Imaging.PixelFormat]::Format32bppArgb);$graphics=[Drawing.Graphics]::FromImage($full);$dc=$graphics.GetHdc();try{if(-not[OslQaSignalWindows]::PrintWindow($oslWindows[0].Handle,$dc,2)){throw 'exact OSL window render failed'}}finally{$graphics.ReleaseHdc($dc);$graphics.Dispose()}
  $safeHeight=48;$safe=[Drawing.Bitmap]::new($width,$safeHeight,[Drawing.Imaging.PixelFormat]::Format32bppArgb);$safeGraphics=[Drawing.Graphics]::FromImage($safe);$safeGraphics.DrawImage($full,[Drawing.Rectangle]::new(0,0,$width,$safeHeight),[Drawing.Rectangle]::new(0,0,$width,$safeHeight),[Drawing.GraphicsUnit]::Pixel);$safeGraphics.Dispose();$full.Dispose()
  $root='C:\ProgramData\OSL-QA\signal\screenshots';[void](New-Item -ItemType Directory -Path $root -Force);$png=Join-Path $root "$CaseId.png"
  try{
    $safe.Save($png,[Drawing.Imaging.ImageFormat]::Png);$safe.Dispose();$hash=(Get-FileHash $png -Algorithm SHA256).Hash.ToLowerInvariant();$bytes=[IO.File]::ReadAllBytes($png)
    $token=Invoke-RestMethod -Method Get -Uri 'http://169.254.169.254/metadata/identity/oauth2/token?api-version=2018-02-01&resource=https%3A%2F%2Fstorage.azure.com%2F' -Headers @{Metadata='true'} -TimeoutSec 10;if(-not $token.access_token){throw 'managed identity storage token unavailable'}
    $client=[Net.Http.HttpClient]::new();try{$client.Timeout=[TimeSpan]::FromSeconds(30);$client.DefaultRequestHeaders.Authorization=[Net.Http.Headers.AuthenticationHeaderValue]::new('Bearer',[string]$token.access_token);$client.DefaultRequestHeaders.Add('x-ms-version','2023-11-03');$client.DefaultRequestHeaders.Add('x-ms-blob-type','BlockBlob');$content=[Net.Http.ByteArrayContent]::new($bytes);$content.Headers.ContentType=[Net.Http.Headers.MediaTypeHeaderValue]::new('image/png');$response=$client.PutAsync($UploadUri,$content).GetAwaiter().GetResult();if(-not $response.IsSuccessStatusCode){throw 'safe screenshot upload failed'}}finally{$client.Dispose();$token=$null}
    [pscustomobject]@{Ok=$true;Action=$Action;CaseId=$CaseId;Region='osl-titlebar-only';Width=$width;Height=$safeHeight;Sha256=$hash;SignalPixelsIncluded=$false;MessagePixelsIncluded=$false;ContentInspected=$false;ForegroundChanged=$false}|ConvertTo-Json -Compress
    exit 0
  }finally{$bytes=$null;if($safe){$safe.Dispose()};Remove-Item $png -Force -ErrorAction SilentlyContinue}
}
if($Action -ceq 'AlreadyRunningAccessibilityBench'){
  $deadline=[DateTime]::UtcNow.AddSeconds([Math]::Min($TimeoutSeconds,30));$started=[Diagnostics.Stopwatch]::StartNew()
  $root=[Windows.Automation.AutomationElement]::FromHandle($window.Handle);if(-not $root){throw 'exact Signal accessibility root is unavailable'}
  $walker=[Windows.Automation.TreeWalker]::RawViewWalker;$queue=New-Object 'System.Collections.Queue';$queue.Enqueue(@($root,0))
  $nodeCount=0;$maxDepth=0;$truncated=$false;$roleCounts=@{}
  while($queue.Count -gt 0){
    if([DateTime]::UtcNow -ge $deadline){$truncated=$true;break}
    $entry=$queue.Dequeue();$node=$entry[0];$depth=[int]$entry[1];$nodeCount++;if($depth -gt $maxDepth){$maxDepth=$depth}
    $role=[string]$node.Current.ControlType.ProgrammaticName;if(-not $role){$role='Unknown'};$roleCounts[$role]=1+[int]($roleCounts[$role])
    if($nodeCount -ge 384){$truncated=$true;break}
    $child=$walker.GetFirstChild($node)
    while($child){
      if($queue.Count -ge 384){$truncated=$true;break}
      $queue.Enqueue(@($child,$depth+1));$child=$walker.GetNextSibling($child)
    }
    if($truncated){break}
  }
  $started.Stop()
  [pscustomobject]@{Ok=$true;Action=$Action;CaseId=$CaseId;ExactSignalWindowCount=1;NodeCount=[int]$nodeCount;MaxDepth=[int]$maxDepth;Truncated=[bool]$truncated;ElapsedMs=[int][Math]::Min($started.ElapsedMilliseconds,[int]::MaxValue);ControlTypeCount=[int]$roleCounts.Count;ContentInspected=$false;ForegroundChanged=$false;SignalAlreadyRunning=$true}|ConvertTo-Json -Compress
  exit 0
}
if($Action -ceq 'ClaimExactWindow'){
  $deadline=[DateTime]::UtcNow.AddSeconds($TimeoutSeconds);$claimed=$false
  do{$root=[Windows.Automation.AutomationElement]::FromHandle($oslWindows[0].Handle);$statuses=@($root.FindAll([Windows.Automation.TreeScope]::Descendants,[Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::ControlTypeProperty,[Windows.Automation.ControlType]::Text))|Where-Object{-not $_.Current.IsOffscreen -and $_.Current.Name -ceq 'Claimed'});if($statuses.Count -eq 1){$claimed=$true;break};Start-Sleep -Milliseconds 200}while([DateTime]::UtcNow -lt $deadline)
  if(-not $claimed){throw 'OSL did not attest the exact Signal window claim'}
}
# Geometry and counts are semantic only. Never return titles, UIA names,
# contact data, messages, phone numbers, or provider accessibility trees.
[pscustomobject]@{Ok=$true;Action=$Action;CaseId=$CaseId;ExactOslProcessCount=1;ExactSignalWindowCount=1;SignalWindowWidth=[int]$window.Width;SignalWindowHeight=[int]$window.Height;VisibilityExpected=($Visibility -ceq 'On');ClaimAttested=($Action -ceq 'ClaimExactWindow');ContentInspected=$false;ForegroundChanged=$false}|ConvertTo-Json -Compress
