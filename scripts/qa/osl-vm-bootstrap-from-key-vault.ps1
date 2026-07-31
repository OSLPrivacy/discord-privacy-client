# OSL-QA-Capability: signal-bootstrap-from-key-vault/v1
param(
  [Parameter(Mandatory)][ValidateRange(1,2)][int]$ClientNumber,
  [Parameter(Mandatory)][ValidatePattern('^[a-zA-Z0-9-]{3,24}$')][string]$PasswordVaultName,
  [Parameter(Mandatory)][ValidatePattern('^[a-zA-Z0-9-]{1,127}$')][string]$PasswordSecretName,
  [Parameter(Mandatory)][ValidateRange(1,128)][int]$SessionId,
  [Parameter(Mandatory)][ValidateNotNullOrEmpty()][string]$WindowsUser,
  [Parameter(Mandatory)][ValidateNotNullOrEmpty()][string]$ProfileRoot,
  [Parameter(Mandatory)][ValidateNotNullOrEmpty()][string]$OslExePath,
  [Parameter(Mandatory)][ValidatePattern('^[0-9a-fA-F]{64}$')][string]$OslExeSha256
)
$ErrorActionPreference='Stop';Set-StrictMode -Version Latest
$path=[IO.Path]::GetFullPath($OslExePath);$profile=[IO.Path]::GetFullPath($ProfileRoot);$expected=$OslExeSha256.ToLowerInvariant()
if(-not(Test-Path -LiteralPath $profile -PathType Container)){throw 'configured profile root is missing'}
if(-not(Test-Path -LiteralPath $path -PathType Leaf) -or (Get-FileHash $path -Algorithm SHA256).Hash.ToLowerInvariant() -cne $expected){throw 'exact OSL executable is missing or hash-mismatched'}
$primary=@(Get-CimInstance Win32_Process -Filter "Name = 'OSL Privacy.exe'"|Where-Object{[int]$_.SessionId -eq $SessionId -and $_.ExecutablePath -and [string]::Equals([IO.Path]::GetFullPath([string]$_.ExecutablePath),$path,[StringComparison]::OrdinalIgnoreCase) -and ([string]$_.CommandLine -notmatch '(?:^|\s)--osl-signal-window-guardian-v1(?:\s|$)')})
if($primary.Count -ne 1){throw 'exact running OSL primary process is unavailable or ambiguous'}
$explorers=@(Get-CimInstance Win32_Process -Filter "Name = 'explorer.exe'"|Where-Object{[int]$_.SessionId -eq $SessionId});if($explorers.Count -ne 1){throw 'interactive Explorer session is unavailable or ambiguous'}
$owner=Invoke-CimMethod -InputObject $explorers[0] -MethodName GetOwner;if($owner.ReturnValue -ne 0 -or $owner.User -cne $WindowsUser -or -not $owner.Domain){throw 'interactive session owner mismatch'};$interactiveUser="$($owner.Domain)\$($owner.User)"
function Get-VaultSecret([string]$vault,[string]$name){
  $token=Invoke-RestMethod -Method Get -Uri 'http://169.254.169.254/metadata/identity/oauth2/token?api-version=2019-08-01&resource=https%3A%2F%2Fvault.azure.net' -Headers @{Metadata='true'} -TimeoutSec 10
  if(-not $token.access_token){throw 'managed identity unavailable'}
  try{$record=Invoke-RestMethod -Method Get -Uri "https://$vault.vault.azure.net/secrets/$name`?api-version=7.4" -Headers @{Authorization="Bearer $($token.access_token)"} -TimeoutSec 15;return [string]$record.value}finally{$token=$null;$record=$null}
}
$password=Get-VaultSecret $PasswordVaultName $PasswordSecretName
if([string]::IsNullOrWhiteSpace($password) -or $password.Length -gt 128){throw 'password secret is unavailable or invalid'}
$root='C:\ProgramData\OSL-QA\signal\bootstrap';[void](New-Item -ItemType Directory -Path $root -Force)
$nonce=[Guid]::NewGuid().ToString('N');$pipeName="OSL-QA-Signal-Bootstrap-$nonce";$taskName="OSL-QA-Signal-Bootstrap-$nonce";$requestPath=Join-Path $root "$nonce.clixml";$scriptPath=Join-Path $root "$nonce.ps1";$resultPath=Join-Path $root "$nonce.json"
[pscustomobject]@{PipeName=$pipeName;ResultPath=$resultPath;SessionId=$SessionId;OslExePath=$path;OslExeSha256=$expected}|Export-Clixml -LiteralPath $requestPath
$wrapper=@'
param([Parameter(Mandatory)][string]$RequestPath)
$ErrorActionPreference='Stop';Set-StrictMode -Version Latest;Add-Type -AssemblyName UIAutomationClient;Add-Type -AssemblyName UIAutomationTypes
if(-not ('OslQaBootstrapWindows' -as [type])){Add-Type @"
using System;using System.Collections.Generic;using System.Runtime.InteropServices;
public static class OslQaBootstrapWindows { public delegate bool EnumWindowsProc(IntPtr h,IntPtr s);[DllImport("user32.dll")]public static extern bool EnumWindows(EnumWindowsProc c,IntPtr s);[DllImport("user32.dll")]public static extern bool IsWindowVisible(IntPtr h);[DllImport("user32.dll")]public static extern uint GetWindowThreadProcessId(IntPtr h,out uint p); }
"@}
function Get-Root([int]$pid){$handles=[Collections.Generic.List[IntPtr]]::new();$cb=[OslQaBootstrapWindows+EnumWindowsProc]{param($h,$s)[uint32]$p=0;[void][OslQaBootstrapWindows]::GetWindowThreadProcessId($h,[ref]$p);if($p -eq $pid -and [OslQaBootstrapWindows]::IsWindowVisible($h)){$handles.Add($h)};return $true};[void][OslQaBootstrapWindows]::EnumWindows($cb,[IntPtr]::Zero);$roots=@($handles|ForEach-Object{try{[Windows.Automation.AutomationElement]::FromHandle($_)}catch{}}|Where-Object{$_});if($roots.Count -ne 1){throw 'exact visible OSL root is unavailable or ambiguous'};return $roots[0]}
function Find-Id($root,[string]$id){@($root.FindAll([Windows.Automation.TreeScope]::Descendants,[Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::AutomationIdProperty,$id))|Where-Object{-not $_.Current.IsOffscreen})}
function Set-Value($element,[string]$value){$element.GetCurrentPattern([Windows.Automation.ValuePattern]::Pattern).SetValue($value)}
$r=Import-Clixml $RequestPath;$password=$null;$passwordBytes=$null
try{
  $matches=@(Get-CimInstance Win32_Process -Filter "Name = 'OSL Privacy.exe'"|Where-Object{[int]$_.SessionId -eq $r.SessionId -and $_.ExecutablePath -and [string]::Equals([IO.Path]::GetFullPath([string]$_.ExecutablePath),$r.OslExePath,[StringComparison]::OrdinalIgnoreCase) -and ([string]$_.CommandLine -notmatch '(?:^|\s)--osl-signal-window-guardian-v1(?:\s|$)')});if($matches.Count -ne 1){throw 'exact OSL process changed'}
  if((Get-FileHash $matches[0].ExecutablePath -Algorithm SHA256).Hash.ToLowerInvariant() -cne $r.OslExeSha256){throw 'interactive OSL hash mismatch'}
  $root=Get-Root ([int]$matches[0].ProcessId)
  $reader=$null;$pipe=[IO.Pipes.NamedPipeClientStream]::new('.',$r.PipeName,[IO.Pipes.PipeDirection]::In);try{$pipe.Connect(15000);$reader=[IO.BinaryReader]::new($pipe,[Text.Encoding]::UTF8,$true);$passwordLength=$reader.ReadInt32();if($passwordLength -lt 6 -or $passwordLength -gt 256){throw 'secret handoff rejected'};$passwordBytes=$reader.ReadBytes($passwordLength);if($passwordBytes.Length -ne $passwordLength){throw 'secret handoff incomplete'};$password=[Text.Encoding]::UTF8.GetString($passwordBytes)}finally{if($reader){$reader.Dispose()};$pipe.Dispose()}
  $workspaceCount=(Find-Id $root 'workspace-render-surface').Count;$passwordCount=(Find-Id $root 'identity-password').Count;$confirmCount=(Find-Id $root 'identity-password-confirm').Count
  if($workspaceCount -eq 1 -or ($passwordCount -eq 1 -and $confirmCount -eq 0)){$password=$null;[Array]::Clear($passwordBytes,0,$passwordBytes.Length);$passwordBytes=$null;@{Status='profileAlreadyPresent'}|ConvertTo-Json -Compress|Set-Content $r.ResultPath -Encoding UTF8;return}
  if($passwordCount -gt 0 -or $confirmCount -gt 0){throw 'partial OSL profile requires manual recovery'}
  $unlockButtons=@($root.FindAll([Windows.Automation.TreeScope]::Descendants,[Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::NameProperty,'Unlock this device'))|Where-Object{-not $_.Current.IsOffscreen -and $_.Current.ControlType -eq [Windows.Automation.ControlType]::Button})
  if($unlockButtons.Count -eq 1){$password=$null;[Array]::Clear($passwordBytes,0,$passwordBytes.Length);$passwordBytes=$null;@{Status='profileAlreadyPresent'}|ConvertTo-Json -Compress|Set-Content $r.ResultPath -Encoding UTF8;return}
  $finishButtons=@($root.FindAll([Windows.Automation.TreeScope]::Descendants,[Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::NameProperty,'Finish setup'))|Where-Object{-not $_.Current.IsOffscreen -and $_.Current.ControlType -eq [Windows.Automation.ControlType]::Button})
  if($finishButtons.Count -gt 0){throw 'partial OSL profile requires manual recovery'}
  $createButtons=@($root.FindAll([Windows.Automation.TreeScope]::Descendants,[Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::NameProperty,'Create account'))|Where-Object{-not $_.Current.IsOffscreen -and $_.Current.ControlType -eq [Windows.Automation.ControlType]::Button})
  if($createButtons.Count -ne 1){throw 'exact account-create entry point is unavailable or ambiguous'};$createButtons[0].GetCurrentPattern([Windows.Automation.InvokePattern]::Pattern).Invoke()
  $deadline=[DateTime]::UtcNow.AddSeconds(10);do{Start-Sleep -Milliseconds 100;$root=Get-Root ([int]$matches[0].ProcessId);$passwordInputs=Find-Id $root 'identity-password';$confirms=Find-Id $root 'identity-password-confirm';$submits=Find-Id $root 'identity-password-submit'}while(($passwordInputs.Count -ne 1 -or $confirms.Count -ne 1 -or $submits.Count -ne 1) -and [DateTime]::UtcNow -lt $deadline)
  if($passwordInputs.Count -ne 1 -or $confirms.Count -ne 1 -or $submits.Count -ne 1){throw 'exact account-create password controls are unavailable or ambiguous'}
  Set-Value $passwordInputs[0] $password;Set-Value $confirms[0] $password;$password=$null;[Array]::Clear($passwordBytes,0,$passwordBytes.Length);$passwordBytes=$null
  $deadline=[DateTime]::UtcNow.AddSeconds(5);while(-not $submits[0].Current.IsEnabled -and [DateTime]::UtcNow -lt $deadline){Start-Sleep -Milliseconds 50};if(-not $submits[0].Current.IsEnabled){throw 'account-create action did not enable'};$submits[0].GetCurrentPattern([Windows.Automation.InvokePattern]::Pattern).Invoke()
  # Treat only the fixed continuation AutomationId as completion. Never read,
  # enumerate, serialize, or log any recovery-page names, values, or text.
  $deadline=[DateTime]::UtcNow.AddSeconds(45);do{Start-Sleep -Milliseconds 150;$root=Get-Root ([int]$matches[0].ProcessId);$continues=Find-Id $root 'recovery-continue'}while($continues.Count -ne 1 -and [DateTime]::UtcNow -lt $deadline)
  if($continues.Count -ne 1){throw 'account creation postcondition timed out'}
  @{Status='profileCreated';SecretSource='managed-identity-key-vault'}|ConvertTo-Json -Compress|Set-Content $r.ResultPath -Encoding UTF8
}catch{@{Status='failed'}|ConvertTo-Json -Compress|Set-Content $r.ResultPath -Encoding UTF8}finally{$password=$null;if($passwordBytes){[Array]::Clear($passwordBytes,0,$passwordBytes.Length)}}
'@
[IO.File]::WriteAllText($scriptPath,$wrapper,[Text.UTF8Encoding]::new($false));$server=[IO.Pipes.NamedPipeServerStream]::new($pipeName,[IO.Pipes.PipeDirection]::Out,1,[IO.Pipes.PipeTransmissionMode]::Byte,[IO.Pipes.PipeOptions]::Asynchronous);$registered=$false
try{
  $action=New-ScheduledTaskAction -Execute 'powershell.exe' -Argument ('-NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "{0}" -RequestPath "{1}"'-f$scriptPath,$requestPath);$principal=New-ScheduledTaskPrincipal -UserId $interactiveUser -LogonType Interactive -RunLevel Limited;Register-ScheduledTask -TaskName $taskName -Action $action -Principal $principal -Settings (New-ScheduledTaskSettingsSet -ExecutionTimeLimit ([TimeSpan]::FromMinutes(2)))|Out-Null;$registered=$true;Start-ScheduledTask $taskName
  $wait=$server.WaitForConnectionAsync();if(-not $wait.Wait(20000)){throw 'secret pipe connection timed out'};$passwordBytes=[Text.Encoding]::UTF8.GetBytes($password);$password=$null;$writer=[IO.BinaryWriter]::new($server,[Text.Encoding]::UTF8,$true);$writer.Write([int]$passwordBytes.Length);$writer.Write($passwordBytes);$writer.Flush();[Array]::Clear($passwordBytes,0,$passwordBytes.Length);$writer.Dispose();$server.Dispose()
  $deadline=[DateTime]::UtcNow.AddSeconds(60);while(-not(Test-Path $resultPath) -and [DateTime]::UtcNow -lt $deadline){Start-Sleep -Milliseconds 100};if(-not(Test-Path $resultPath)){throw 'profile bootstrap result timed out'};$result=Get-Content $resultPath -Raw|ConvertFrom-Json;if($result.Status -cne 'profileCreated' -and $result.Status -cne 'profileAlreadyPresent'){throw 'profile bootstrap failed'};[pscustomobject]@{Status=$result.Status;ClientNumber=$ClientNumber;SecretSource='managed-identity-key-vault';RecoveryMaterialRead=$false;RecoveryMaterialLogged=$false}|ConvertTo-Json -Compress
}finally{$password=$null;if($server){$server.Dispose()};if($registered){Unregister-ScheduledTask $taskName -Confirm:$false -ErrorAction SilentlyContinue};Remove-Item -LiteralPath $scriptPath,$requestPath,$resultPath -Force -ErrorAction SilentlyContinue}
