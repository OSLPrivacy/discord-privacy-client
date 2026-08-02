<#
.SYNOPSIS
  T5-T18: verifies the two UIA-visible verification codes and completes both
  sides of the ceremony without using screenshots.

.DESCRIPTION
  Run once on each isolated QA VM.  The coordinator transports the peer value
  over its protected test channel and supplies it through stdin (never a
  command line, log, screenshot, or receipt).  The script reads the displayed
  code from the live accessibility tree, compares it byte-for-byte with the
  peer's UIA observation, enters the peer value, and invokes the confirmation
  button.  Its receipt contains SHA-256 digests only.
#>
[CmdletBinding()]
param(
  [Parameter(Mandatory)][ValidateSet('A','B')][string]$Side,
  [Parameter(Mandatory)][int]$ProcessId,
  [Parameter(Mandatory)][string]$ReceiptPath,
  [switch]$ReadPeerCodeFromStandardInput
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

if (-not $ReadPeerCodeFromStandardInput) {
  throw 'T5_PEER_CODE_REQUIRED: pass -ReadPeerCodeFromStandardInput; codes must not appear in command arguments.'
}

# UIA/MSAA stays available while capture protection correctly blocks screenshots.
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes

function Get-VisibleById([Windows.Automation.AutomationElement]$Root, [string]$Id) {
  $condition = [Windows.Automation.PropertyCondition]::new(
    [Windows.Automation.AutomationElement]::AutomationIdProperty, $Id)
  $found = @($Root.FindAll([Windows.Automation.TreeScope]::Descendants, $condition) |
    Where-Object { -not $_.Current.IsOffscreen })
  if ($found.Count -ne 1) { throw "T5_UIA_AMBIGUOUS: expected one visible '$Id', found $($found.Count)." }
  return $found[0]
}

function Get-VisibleByName([Windows.Automation.AutomationElement]$Root, [string]$Name) {
  $condition = [Windows.Automation.PropertyCondition]::new(
    [Windows.Automation.AutomationElement]::NameProperty, $Name)
  $found = @($Root.FindAll([Windows.Automation.TreeScope]::Descendants, $condition) |
    Where-Object { -not $_.Current.IsOffscreen })
  if ($found.Count -ne 1) { throw "T5_UIA_AMBIGUOUS: expected one visible '$Name', found $($found.Count)." }
  return $found[0]
}

function Get-UiaText([Windows.Automation.AutomationElement]$Element) {
  $value = $null
  if ($Element.TryGetCurrentPattern([Windows.Automation.ValuePattern]::Pattern, [ref]$value)) {
    return ([Windows.Automation.ValuePattern]$value).Current.Value
  }
  return $Element.Current.Name
}

function Set-UiaText([Windows.Automation.AutomationElement]$Element, [string]$Value) {
  $pattern = $null
  if (-not $Element.TryGetCurrentPattern([Windows.Automation.ValuePattern]::Pattern, [ref]$pattern)) {
    throw 'T5_UIA_NO_VALUE_PATTERN: verification input is not writable through UIA.'
  }
  ([Windows.Automation.ValuePattern]$pattern).SetValue($Value)
}

function Invoke-Uia([Windows.Automation.AutomationElement]$Element) {
  $pattern = $null
  if (-not $Element.TryGetCurrentPattern([Windows.Automation.InvokePattern]::Pattern, [ref]$pattern)) {
    throw 'T5_UIA_NO_INVOKE_PATTERN: verification confirmation is not invokable through UIA.'
  }
  ([Windows.Automation.InvokePattern]$pattern).Invoke()
}

function Get-Digest([string]$Value) {
  $bytes = [Text.UTF8Encoding]::new($false).GetBytes($Value)
  return ([Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($bytes))).ToLowerInvariant()
}

$root = [Windows.Automation.AutomationElement]::RootElement
$processCondition = [Windows.Automation.PropertyCondition]::new(
  [Windows.Automation.AutomationElement]::ProcessIdProperty, $ProcessId)
$windows = @($root.FindAll([Windows.Automation.TreeScope]::Children, $processCondition) |
  Where-Object { -not $_.Current.IsOffscreen })
if ($windows.Count -ne 1) { throw "T5_UIA_ROOT_AMBIGUOUS: expected one visible app window for pid $ProcessId, found $($windows.Count)." }

$app = $windows[0]
$display = Get-VisibleByName $app 'Shared verification code for this friend'
$input = Get-VisibleById $app 'friend-verification-input'
$confirm = Get-VisibleById $app 'owned-confirmation-submit'
$localCode = Get-UiaText $display
$peerCode = [Console]::In.ReadToEnd().TrimEnd("`r", "`n")
if ([string]::IsNullOrWhiteSpace($localCode) -or [string]::IsNullOrWhiteSpace($peerCode)) {
  throw 'T5_EMPTY_CODE: the UIA-visible local or peer verification code was blank.'
}

# This is deliberately exact.  Normalising whitespace/digits would reintroduce
# the self-referential proof loophole by making different rendered strings pass.
if ($localCode -cne $peerCode) {
  throw "T5_SAFETY_NUMBER_MISMATCH: $Side displayed a different verification code than its peer."
}

Set-UiaText $input $peerCode
Invoke-Uia $confirm

$receipt = [ordered]@{
  task = 'T5-T18'; side = $Side; status = 'pass'; comparedByteForByte = $true
  localCodeSha256 = Get-Digest $localCode; peerCodeSha256 = Get-Digest $peerCode
  completedUtc = [DateTime]::UtcNow.ToString('o')
}
[IO.File]::WriteAllText($ReceiptPath, ($receipt | ConvertTo-Json -Compress), [Text.UTF8Encoding]::new($false))
