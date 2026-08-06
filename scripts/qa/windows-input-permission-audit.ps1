<#
Reads Windows process integrity levels for OSL and native messaging apps.

UIPI blocks synthetic input from a lower-integrity process to a higher-
integrity target without a useful SendInput error. This probe reads the live
process token, not installation metadata, and reports whether OSL's current
integrity level is high enough to send input to each app.
#>
[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

if (-not ('OslAudit.TokenNative' -as [type])) {
  Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;

namespace OslAudit {
  public static class TokenNative {
    public const UInt32 PROCESS_QUERY_LIMITED_INFORMATION = 0x1000;
    public const UInt32 TOKEN_QUERY = 0x0008;
    public const Int32 TokenIntegrityLevel = 25;

    [DllImport("kernel32.dll", SetLastError = true)]
    public static extern IntPtr OpenProcess(UInt32 desiredAccess, bool inheritHandle, UInt32 processId);

    [DllImport("advapi32.dll", SetLastError = true)]
    public static extern bool OpenProcessToken(IntPtr processHandle, UInt32 desiredAccess, out IntPtr tokenHandle);

    [DllImport("advapi32.dll", SetLastError = true)]
    public static extern bool GetTokenInformation(
      IntPtr tokenHandle,
      Int32 tokenInformationClass,
      IntPtr tokenInformation,
      UInt32 tokenInformationLength,
      out UInt32 returnLength);

    [DllImport("advapi32.dll", SetLastError = true)]
    public static extern UInt32 GetLengthSid(IntPtr sid);

    [DllImport("advapi32.dll", SetLastError = true)]
    public static extern IntPtr GetSidSubAuthorityCount(IntPtr sid);

    [DllImport("advapi32.dll", SetLastError = true)]
    public static extern IntPtr GetSidSubAuthority(IntPtr sid, UInt32 subAuthority);

    [DllImport("kernel32.dll", SetLastError = true)]
    public static extern bool CloseHandle(IntPtr handle);
  }
}
'@
}

function Get-IntegrityName {
  param([Parameter(Mandatory)] [uint32]$Rid)
  if ($Rid -ge 0x5000) { return 'Protected' }
  if ($Rid -ge 0x4000) { return 'System' }
  if ($Rid -ge 0x3000) { return 'High' }
  if ($Rid -ge 0x2100) { return 'MediumPlus' }
  if ($Rid -ge 0x2000) { return 'Medium' }
  if ($Rid -ge 0x1000) { return 'Low' }
  return 'Untrusted'
}

function Get-ProcessIntegrity {
  param([Parameter(Mandatory)] [System.Diagnostics.Process]$Process)

  $processHandle = [OslAudit.TokenNative]::OpenProcess(
    [OslAudit.TokenNative]::PROCESS_QUERY_LIMITED_INFORMATION,
    $false,
    [uint32]$Process.Id)
  if ($processHandle -eq [IntPtr]::Zero) {
    throw "OpenProcess failed for pid $($Process.Id): $([Runtime.InteropServices.Marshal]::GetLastWin32Error())"
  }

  $tokenHandle = [IntPtr]::Zero
  try {
    if (-not [OslAudit.TokenNative]::OpenProcessToken(
      $processHandle,
      [OslAudit.TokenNative]::TOKEN_QUERY,
      [ref]$tokenHandle)) {
      throw "OpenProcessToken failed for pid $($Process.Id): $([Runtime.InteropServices.Marshal]::GetLastWin32Error())"
    }

    $required = [uint32]0
    [void][OslAudit.TokenNative]::GetTokenInformation(
      $tokenHandle,
      [OslAudit.TokenNative]::TokenIntegrityLevel,
      [IntPtr]::Zero,
      0,
      [ref]$required)
    if ($required -eq 0) {
      throw "GetTokenInformation size query failed for pid $($Process.Id): $([Runtime.InteropServices.Marshal]::GetLastWin32Error())"
    }

    $buffer = [Runtime.InteropServices.Marshal]::AllocHGlobal([int]$required)
    try {
      if (-not [OslAudit.TokenNative]::GetTokenInformation(
        $tokenHandle,
        [OslAudit.TokenNative]::TokenIntegrityLevel,
        $buffer,
        $required,
        [ref]$required)) {
        throw "GetTokenInformation failed for pid $($Process.Id): $([Runtime.InteropServices.Marshal]::GetLastWin32Error())"
      }

      $sid = [Runtime.InteropServices.Marshal]::ReadIntPtr($buffer)
      $subAuthorityCountPointer = [OslAudit.TokenNative]::GetSidSubAuthorityCount($sid)
      if ($subAuthorityCountPointer -eq [IntPtr]::Zero) {
        throw "GetSidSubAuthorityCount failed for pid $($Process.Id): $([Runtime.InteropServices.Marshal]::GetLastWin32Error())"
      }
      $subAuthorityCount = [Runtime.InteropServices.Marshal]::ReadByte($subAuthorityCountPointer)
      if ($subAuthorityCount -lt 1) {
        throw "integrity SID for pid $($Process.Id) has no subauthority"
      }
      $ridPointer = [OslAudit.TokenNative]::GetSidSubAuthority($sid, [uint32]($subAuthorityCount - 1))
      if ($ridPointer -eq [IntPtr]::Zero) {
        throw "GetSidSubAuthority failed for pid $($Process.Id): $([Runtime.InteropServices.Marshal]::GetLastWin32Error())"
      }
      $rid = [uint32][Runtime.InteropServices.Marshal]::ReadInt32($ridPointer)

      return [pscustomobject]@{
        Id = [int]$Process.Id
        ProcessName = [string]$Process.ProcessName
        Path = [string]$Process.Path
        MainWindowHandle = [string]$Process.MainWindowHandle
        IntegrityRid = [uint32]$rid
        IntegrityLevel = Get-IntegrityName -Rid $rid
      }
    } finally {
      if ($null -ne $buffer) { [Runtime.InteropServices.Marshal]::FreeHGlobal($buffer) }
    }
  } finally {
    if ($tokenHandle -ne [IntPtr]::Zero) { [void][OslAudit.TokenNative]::CloseHandle($tokenHandle) }
    if ($processHandle -ne [IntPtr]::Zero) { [void][OslAudit.TokenNative]::CloseHandle($processHandle) }
  }
}

function Get-AppProcesses {
  param([Parameter(Mandatory)] [string[]]$Names)
  $items = New-Object System.Collections.Generic.List[System.Diagnostics.Process]
  foreach ($name in $Names) {
    foreach ($process in @(Get-Process -Name $name -ErrorAction SilentlyContinue)) {
      [void]$items.Add($process)
    }
  }
  return @($items | Sort-Object Id)
}

function New-AppReport {
  param(
    [Parameter(Mandatory)] [string]$App,
    [Parameter(Mandatory)] [string[]]$Names,
    [AllowNull()] [Nullable[uint32]]$OslIntegrityRid
  )

  $processes = @(Get-AppProcesses -Names $Names)
  if ($processes.Count -eq 0) {
    return [pscustomobject]@{
      App = $App
      Status = 'not running'
      PermissionLevel = 'not running'
      IntegrityRid = $null
      ProcessIds = @()
      ProcessNames = @()
      OslMaySendItInput = 'no'
      Reason = 'app is not running'
      Processes = @()
    }
  }

  $processReports = @($processes | ForEach-Object { Get-ProcessIntegrity -Process $_ })
  $uniqueRids = @($processReports | Select-Object -ExpandProperty IntegrityRid -Unique | Sort-Object)
  $uniqueNames = @($processReports | Select-Object -ExpandProperty IntegrityLevel -Unique | Sort-Object)
  $maxRid = [uint32](@($uniqueRids | Measure-Object -Maximum).Maximum)
  $permission = if ($uniqueNames.Count -eq 1) { $uniqueNames[0] } else { 'mixed:' + ($uniqueNames -join ',') }
  $maySend = if ($null -ne $OslIntegrityRid -and [uint32]$OslIntegrityRid -ge $maxRid) { 'yes' } else { 'no' }
  $reason = if ($null -eq $OslIntegrityRid) {
    'OSL is not running'
  } elseif ($maySend -eq 'yes') {
    "OSL integrity RID $OslIntegrityRid is >= target max RID $maxRid"
  } else {
    "OSL integrity RID $OslIntegrityRid is < target max RID $maxRid"
  }

  return [pscustomobject]@{
    App = $App
    Status = 'running'
    PermissionLevel = $permission
    IntegrityRid = if ($uniqueRids.Count -eq 1) { [uint32]$uniqueRids[0] } else { @($uniqueRids) }
    ProcessIds = @($processReports | Select-Object -ExpandProperty Id)
    ProcessNames = @($processReports | Select-Object -ExpandProperty ProcessName -Unique)
    OslMaySendItInput = $maySend
    Reason = $reason
    Processes = $processReports
  }
}

$targets = @(
  [pscustomobject]@{ App = 'Discord'; Names = @('Discord', 'DiscordPTB', 'DiscordCanary') }
  [pscustomobject]@{ App = 'Telegram'; Names = @('Telegram') }
  [pscustomobject]@{ App = 'Signal'; Names = @('Signal') }
  [pscustomobject]@{ App = 'WhatsApp'; Names = @('WhatsApp', 'WhatsApp.Root') }
  [pscustomobject]@{ App = 'OSL'; Names = @('OSL Privacy', 'osl-privacy-hub', 'discord-privacy-client') }
)

$oslProcesses = @(Get-AppProcesses -Names @('OSL Privacy', 'osl-privacy-hub', 'discord-privacy-client'))
$oslReport = $null
$oslIntegrityRid = $null
if ($oslProcesses.Count -eq 0) {
  $oslReport = [pscustomobject]@{
    App = 'OSL'
    Status = 'not running'
    PermissionLevel = 'not running'
    IntegrityRid = $null
    ProcessIds = @()
    ProcessNames = @()
    OslMaySendItInput = 'n/a'
    Reason = 'OSL is not running'
    Processes = @()
  }
} else {
  $oslProcessReports = @($oslProcesses | ForEach-Object { Get-ProcessIntegrity -Process $_ })
  $oslRids = @($oslProcessReports | Select-Object -ExpandProperty IntegrityRid -Unique | Sort-Object)
  $oslNames = @($oslProcessReports | Select-Object -ExpandProperty IntegrityLevel -Unique | Sort-Object)
  $oslIntegrityRid = [uint32](@($oslRids | Measure-Object -Maximum).Maximum)
  $oslReport = [pscustomobject]@{
    App = 'OSL'
    Status = 'running'
    PermissionLevel = if ($oslNames.Count -eq 1) { $oslNames[0] } else { 'mixed:' + ($oslNames -join ',') }
    IntegrityRid = if ($oslRids.Count -eq 1) { [uint32]$oslRids[0] } else { @($oslRids) }
    ProcessIds = @($oslProcessReports | Select-Object -ExpandProperty Id)
    ProcessNames = @($oslProcessReports | Select-Object -ExpandProperty ProcessName -Unique)
    OslMaySendItInput = 'n/a'
    Reason = 'sender baseline'
    Processes = $oslProcessReports
  }
}

$reports = New-Object System.Collections.Generic.List[object]
foreach ($target in $targets) {
  if ($target.App -eq 'OSL') {
    [void]$reports.Add($oslReport)
  } else {
    [void]$reports.Add((New-AppReport -App $target.App -Names $target.Names -OslIntegrityRid $oslIntegrityRid))
  }
}

[pscustomobject]@{
  Schema = 'osl-windows-input-permission-audit/v1'
  MeasuredAt = (Get-Date).ToString('o')
  PermissionSource = 'GetTokenInformation(TokenIntegrityLevel) on running process tokens'
  Rule = 'OSL may send input when OSL integrity RID is greater than or equal to the target app integrity RID'
  Apps = @($reports.ToArray())
} | ConvertTo-Json -Depth 6
