# OSL-QA-Capability: signal-unlock-from-key-vault/v1
param(
  [Parameter(Mandatory)][ValidateRange(1,2)][int]$ClientNumber,
  [Parameter(Mandatory)][ValidatePattern('^[a-zA-Z0-9-]{3,24}$')][string]$VaultName,
  [Parameter(Mandatory)][ValidatePattern('^[a-zA-Z0-9-]{1,127}$')][string]$SecretName,
  [Parameter(Mandatory)][ValidateRange(1,128)][int]$SessionId,
  [Parameter(Mandatory)][ValidateNotNullOrEmpty()][string]$WindowsUser,
  [Parameter(Mandatory)][ValidateNotNullOrEmpty()][string]$ProfileRoot,
  [Parameter(Mandatory)][ValidateNotNullOrEmpty()][string]$OslExePath,
  [Parameter(Mandatory)][ValidatePattern('^[0-9a-fA-F]{64}$')][string]$OslExeSha256
)
$ErrorActionPreference='Stop'
Set-StrictMode -Version Latest
$path=[IO.Path]::GetFullPath($OslExePath);$profile=[IO.Path]::GetFullPath($ProfileRoot);$expected=$OslExeSha256.ToLowerInvariant()
if(-not(Test-Path -LiteralPath $profile -PathType Container)){throw 'configured profile root is missing'}
if(-not(Test-Path -LiteralPath $path -PathType Leaf) -or (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant() -cne $expected){throw 'exact OSL executable is missing or hash-mismatched'}
$processes=@(Get-CimInstance Win32_Process -Filter "Name = 'OSL Privacy.exe'"|Where-Object{[int]$_.SessionId -eq $SessionId -and $_.ExecutablePath -and [string]::Equals([IO.Path]::GetFullPath([string]$_.ExecutablePath),$path,[StringComparison]::OrdinalIgnoreCase) -and ([string]$_.CommandLine -notmatch '(?:^|\s)--osl-signal-window-guardian-v1(?:\s|$)')})
if($processes.Count -ne 1){throw 'exact running OSL process is unavailable or ambiguous'}
$explorers=@(Get-CimInstance Win32_Process -Filter "Name = 'explorer.exe'"|Where-Object{[int]$_.SessionId -eq $SessionId})
if($explorers.Count -ne 1){throw 'interactive Explorer session is unavailable or ambiguous'}
$owner=Invoke-CimMethod -InputObject $explorers[0] -MethodName GetOwner
if($owner.ReturnValue -ne 0 -or $owner.User -cne $WindowsUser -or -not $owner.Domain){throw 'interactive session owner mismatch'}
$interactiveUser="$($owner.Domain)\$($owner.User)"
$tokenUri='http://169.254.169.254/metadata/identity/oauth2/token?api-version=2019-08-01&resource=https%3A%2F%2Fvault.azure.net'
$token=Invoke-RestMethod -Method Get -Uri $tokenUri -Headers @{Metadata='true'} -TimeoutSec 10
if(-not $token.access_token){throw 'managed identity unavailable'}
$record=Invoke-RestMethod -Method Get -Uri "https://$VaultName.vault.azure.net/secrets/$SecretName`?api-version=7.4" -Headers @{Authorization="Bearer $($token.access_token)"} -TimeoutSec 15
$token=$null;$credential=[string]$record.value;$record=$null
if([string]::IsNullOrWhiteSpace($credential) -or $credential.Length -gt 1024){throw 'unlock credential is unavailable or invalid'}
$root='C:\ProgramData\OSL-QA\signal\unlock';[void](New-Item -ItemType Directory -Path $root -Force)
$nonce=[Guid]::NewGuid().ToString('N');$pipeName="OSL-QA-Signal-Unlock-$nonce";$taskName="OSL-QA-Signal-Unlock-$nonce";$scriptPath=Join-Path $root "$nonce.ps1";$resultPath=Join-Path $root "$nonce.json"
$request=[pscustomobject]@{PipeName=$pipeName;ResultPath=$resultPath;SessionId=$SessionId;WindowsUser=$WindowsUser;OslExePath=$path;OslExeSha256=$expected}
$requestPath=Join-Path $root "$nonce.clixml";$request|Export-Clixml -LiteralPath $requestPath
$wrapper=@'
param([Parameter(Mandatory)][string]$RequestPath)
$ErrorActionPreference='Stop';Set-StrictMode -Version Latest
Add-Type -AssemblyName UIAutomationClient;Add-Type -AssemblyName UIAutomationTypes
if(-not ('OslQaNativeWindows' -as [type])){
  Add-Type @"
using System;
using System.Runtime.InteropServices;
public static class OslQaNativeWindows {
  public delegate bool EnumWindowsProc(IntPtr hwnd, IntPtr state);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumWindowsProc callback, IntPtr state);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hwnd);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint processId);
}
"@
}
function Get-VisibleProcessRoots([int]$ProcessId){
  $handles=[Collections.Generic.List[IntPtr]]::new()
  $callback=[OslQaNativeWindows+EnumWindowsProc]{param([IntPtr]$hwnd,[IntPtr]$state)
    [uint32]$candidatePid=0
    [void][OslQaNativeWindows]::GetWindowThreadProcessId($hwnd,[ref]$candidatePid)
    if($candidatePid -eq $ProcessId -and [OslQaNativeWindows]::IsWindowVisible($hwnd)){
      $handles.Add($hwnd)
      if($handles.Count -gt 32){return $false}
    }
    return $true
  }
  [void][OslQaNativeWindows]::EnumWindows($callback,[IntPtr]::Zero)
  if($handles.Count -gt 32){throw 'visible OSL window set is unexpectedly large'}
  $roots=@()
  foreach($handle in $handles){
    try{$element=[Windows.Automation.AutomationElement]::FromHandle($handle);if($element){$roots+=$element}}catch{}
  }
  return @($roots)
}
function Get-VerifiedUnlockState([int]$ProcessId){
  $surfaces=@()
  $workspaceSurfaces=@();$roots=@(Get-VisibleProcessRoots $ProcessId)
  foreach($candidate in $roots){
    $candidateEdits=@($candidate.FindAll([Windows.Automation.TreeScope]::Descendants,[Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::AutomationIdProperty,'identity-password'))|Where-Object{-not $_.Current.IsOffscreen})
    $candidateButtons=@($candidate.FindAll([Windows.Automation.TreeScope]::Descendants,[Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::AutomationIdProperty,'identity-password-submit'))|Where-Object{-not $_.Current.IsOffscreen})
    $candidateWorkspaces=@($candidate.FindAll([Windows.Automation.TreeScope]::Descendants,[Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::AutomationIdProperty,'workspace-render-surface'))|Where-Object{-not $_.Current.IsOffscreen})
    if($candidateEdits.Count -gt 0 -or $candidateButtons.Count -gt 0){
      $surfaces+=,[pscustomobject]@{Root=$candidate;Edits=$candidateEdits;Buttons=$candidateButtons}
    }
    if($candidateWorkspaces.Count -gt 0){$workspaceSurfaces+=,[pscustomobject]@{Root=$candidate;Workspaces=$candidateWorkspaces}}
  }
  if($surfaces.Count -eq 1 -and $surfaces[0].Edits.Count -eq 1 -and $surfaces[0].Buttons.Count -eq 1){return [pscustomobject]@{Status='locked';Surface=$surfaces[0]}}
  if($surfaces.Count -eq 0 -and $roots.Count -gt 0 -and $workspaceSurfaces.Count -eq 1 -and $workspaceSurfaces[0].Workspaces.Count -eq 1){return [pscustomobject]@{Status='alreadyUnlocked';Surface=$null}}
  throw 'exact unlock controls are unavailable or ambiguous'
}
$r=Import-Clixml -LiteralPath $RequestPath;$bytes=$null;$secret=$null
try{
  $pipe=[IO.Pipes.NamedPipeClientStream]::new('.',$r.PipeName,[IO.Pipes.PipeDirection]::In)
  try{$pipe.Connect(15000);$reader=[IO.BinaryReader]::new($pipe,[Text.Encoding]::UTF8,$true);$length=$reader.ReadInt32();if($length -lt 1 -or $length -gt 4096){throw 'credential handoff rejected'};$bytes=$reader.ReadBytes($length);if($bytes.Length -ne $length){throw 'credential handoff incomplete'};$secret=[Text.Encoding]::UTF8.GetString($bytes);$reader.Dispose()}finally{$pipe.Dispose()}
  $matches=@(Get-CimInstance Win32_Process -Filter "Name = 'OSL Privacy.exe'"|Where-Object{[int]$_.SessionId -eq $r.SessionId -and $_.ExecutablePath -and [string]::Equals([IO.Path]::GetFullPath([string]$_.ExecutablePath),$r.OslExePath,[StringComparison]::OrdinalIgnoreCase) -and ([string]$_.CommandLine -notmatch '(?:^|\s)--osl-signal-window-guardian-v1(?:\s|$)')})
  if($matches.Count -ne 1){throw 'exact interactive OSL process is unavailable or ambiguous'}
  if((Get-FileHash -LiteralPath $matches[0].ExecutablePath -Algorithm SHA256).Hash.ToLowerInvariant() -cne $r.OslExeSha256){throw 'interactive OSL hash mismatch'}
  $unlockState=Get-VerifiedUnlockState ([int]$matches[0].ProcessId)
  if($unlockState.Status -ceq 'alreadyUnlocked'){
    $secret=$null;[Array]::Clear($bytes,0,$bytes.Length);$bytes=$null
    @{Status='alreadyUnlocked'}|ConvertTo-Json -Compress|Set-Content -LiteralPath $r.ResultPath -Encoding UTF8
    return
  }
  $surface=$unlockState.Surface;$edits=$surface.Edits;$buttons=$surface.Buttons
  $edits[0].GetCurrentPattern([Windows.Automation.ValuePattern]::Pattern).SetValue($secret)
  $secret=$null;[Array]::Clear($bytes,0,$bytes.Length);$bytes=$null
  $deadline=[DateTime]::UtcNow.AddSeconds(5);do{Start-Sleep -Milliseconds 50;if($buttons[0].Current.IsEnabled){break}}while([DateTime]::UtcNow -lt $deadline)
  if(-not $buttons[0].Current.IsEnabled){throw 'unlock action did not enable'}
  $buttons[0].GetCurrentPattern([Windows.Automation.InvokePattern]::Pattern).Invoke()
  $deadline=[DateTime]::UtcNow.AddSeconds(20);$unlocked=$false
  do{
    Start-Sleep -Milliseconds 100
    $freshRoots=@(Get-VisibleProcessRoots ([int]$matches[0].ProcessId));$errors=@()
    foreach($fresh in $freshRoots){
      $errors+=@($fresh.FindAll([Windows.Automation.TreeScope]::Descendants,[Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::AutomationIdProperty,'password-error'))|Where-Object{-not $_.Current.IsOffscreen -and $_.Current.Name})
    }
    if($errors.Count){throw 'unlock credential was rejected'}
    try{$postUnlockState=Get-VerifiedUnlockState ([int]$matches[0].ProcessId);if($postUnlockState.Status -ceq 'alreadyUnlocked'){$unlocked=$true;break}}catch{}
  }while([DateTime]::UtcNow -lt $deadline)
  if(-not $unlocked){throw 'unlock postcondition timed out'}
  @{Status='unlocked'}|ConvertTo-Json -Compress|Set-Content -LiteralPath $r.ResultPath -Encoding UTF8
}catch{@{Status='failed';Detail=if($_.Exception.Message.Length -le 120){$_.Exception.Message}else{$_.Exception.Message.Substring(0,120)}}|ConvertTo-Json -Compress|Set-Content -LiteralPath $r.ResultPath -Encoding UTF8}finally{$secret=$null;if($bytes){[Array]::Clear($bytes,0,$bytes.Length)}}
'@
[IO.File]::WriteAllText($scriptPath,$wrapper,[Text.UTF8Encoding]::new($false))
$server=[IO.Pipes.NamedPipeServerStream]::new($pipeName,[IO.Pipes.PipeDirection]::Out,1,[IO.Pipes.PipeTransmissionMode]::Byte,[IO.Pipes.PipeOptions]::Asynchronous)
$registered=$false
try{
  $action=New-ScheduledTaskAction -Execute 'powershell.exe' -Argument ('-NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "{0}" -RequestPath "{1}"' -f $scriptPath,$requestPath)
  $principal=New-ScheduledTaskPrincipal -UserId $interactiveUser -LogonType Interactive -RunLevel Limited
  Register-ScheduledTask -TaskName $taskName -Action $action -Principal $principal -Settings (New-ScheduledTaskSettingsSet -ExecutionTimeLimit ([TimeSpan]::FromMinutes(1)))|Out-Null;$registered=$true;Start-ScheduledTask $taskName
  $wait=$server.WaitForConnectionAsync();if(-not $wait.Wait(20000)){throw 'credential pipe connection timed out'}
  $bytes=[Text.Encoding]::UTF8.GetBytes($credential);$credential=$null;$writer=[IO.BinaryWriter]::new($server,[Text.Encoding]::UTF8,$true);$writer.Write([int]$bytes.Length);$writer.Write($bytes);$writer.Flush();[Array]::Clear($bytes,0,$bytes.Length);$writer.Dispose();$server.Dispose()
  $deadline=[DateTime]::UtcNow.AddSeconds(25);while(-not(Test-Path -LiteralPath $resultPath) -and [DateTime]::UtcNow -lt $deadline){Start-Sleep -Milliseconds 100}
  if(-not(Test-Path -LiteralPath $resultPath)){throw 'unlock result timed out'}
  $result=Get-Content -LiteralPath $resultPath -Raw|ConvertFrom-Json;if($result.Status -cne 'unlocked' -and $result.Status -cne 'alreadyUnlocked'){throw "interactive unlock failed: $($result.Detail)"}
  [pscustomobject]@{Status=$result.Status;ClientNumber=$ClientNumber;SessionId=$SessionId;SecretSource='managed-identity-key-vault'}|ConvertTo-Json -Compress
}finally{$credential=$null;if($server){$server.Dispose()};if($registered){Unregister-ScheduledTask $taskName -Confirm:$false -ErrorAction SilentlyContinue};Remove-Item -LiteralPath $scriptPath,$requestPath,$resultPath -Force -ErrorAction SilentlyContinue}
