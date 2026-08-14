<#
  TASK 4089 test setup -- NOT the shipping reader.

  Sends a fixed number of unique marker messages into the signed-in account's
  own "Note to Self" conversation (Signal shows this using the account's own
  display name, e.g. "Chat with <name>") so the shipping read-only route in
  apps/osl-hub/src/native_signal_row_words.rs has real newly-sent rows to
  read. This script is deliberately outside the read-only contract under
  test: it clicks, focuses, types and presses Enter. It never touches any
  other conversation, and it re-selects Note to Self before every single
  message in case another concurrently running lane's own automation moves
  focus elsewhere on this shared Windows session.

  Text is delivered by clipboard paste rather than literal SendKeys text so
  no per-character SendKeys escaping is needed.

  The composer's pre-existing draft (belonging to unrelated in-flight work)
  is captured before any change and pasted back into the composer as a draft
  (not sent) once the marker messages are in.
#>
[CmdletBinding()]
param(
  [Parameter(Mandatory)] [string[]] $Messages
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName System.Windows.Forms

function Get-ExactSignalProcess {
  $all = @(Get-Process -Name Signal -ErrorAction SilentlyContinue |
    Where-Object { $_.MainWindowHandle -ne [IntPtr]::Zero -and $_.MainWindowTitle -ceq 'Signal' })
  if ($all.Count -ne 1) { throw "exact visible Signal window count=$($all.Count)" }
  return $all[0]
}

function Select-NoteToSelf {
  param([Windows.Automation.AutomationElement] $Root)
  $all = @($Root.FindAll([Windows.Automation.TreeScope]::Descendants, [Windows.Automation.Condition]::TrueCondition))
  $btn = $all | Where-Object {
    $_.Current.ControlType -eq [Windows.Automation.ControlType]::Button -and $_.Current.Name -like 'Chat with *'
  } | Select-Object -First 1
  if (-not $btn) { throw 'Note to Self chat entry not found' }
  $pattern = $null
  if (-not $btn.TryGetCurrentPattern([Windows.Automation.InvokePattern]::Pattern, [ref]$pattern)) {
    throw 'Note to Self chat entry has no InvokePattern'
  }
  ([Windows.Automation.InvokePattern]$pattern).Invoke()
  Start-Sleep -Milliseconds 500
}

function Get-ComposerEdit {
  param([Windows.Automation.AutomationElement] $Root)
  for ($attempt = 0; $attempt -lt 20; $attempt++) {
    $all = @($Root.FindAll([Windows.Automation.TreeScope]::Descendants, [Windows.Automation.Condition]::TrueCondition))
    # Signal Desktop's composer is the Quill.js contenteditable region; its
    # UIA-projected class name is the one stable, unambiguous handle to it.
    $edit = $all | Where-Object {
      $_.Current.IsKeyboardFocusable -and $_.Current.ClassName -like '*ql-editor*'
    } | Select-Object -First 1
    if ($edit) { return $edit }
    Start-Sleep -Milliseconds 200
  }
  throw 'Signal composer (ql-editor) not found'
}

function Set-ComposerText {
  param([Windows.Automation.AutomationElement] $Root, [string] $Text)
  Select-NoteToSelf -Root $Root
  $composer = Get-ComposerEdit -Root $Root
  $composer.SetFocus()
  Start-Sleep -Milliseconds 150
  [Windows.Forms.SendKeys]::SendWait('^a')
  Start-Sleep -Milliseconds 80
  [Windows.Forms.SendKeys]::SendWait('{DELETE}')
  Start-Sleep -Milliseconds 80
  if ($Text) {
    [Windows.Forms.Clipboard]::SetText($Text)
    Start-Sleep -Milliseconds 80
    [Windows.Forms.SendKeys]::SendWait('^v')
    Start-Sleep -Milliseconds 150
  }
  return $composer
}

$process = Get-ExactSignalProcess
$root = [Windows.Automation.AutomationElement]::FromHandle($process.MainWindowHandle)
Select-NoteToSelf -Root $root
$composer = Get-ComposerEdit -Root $root
$composer.SetFocus()
Start-Sleep -Milliseconds 200

# Capture the pre-existing draft with copy, not by reading provider text, so
# no assumption is made about which pattern the composer supports.
[Windows.Forms.SendKeys]::SendWait('^a')
Start-Sleep -Milliseconds 100
[Windows.Forms.SendKeys]::SendWait('^c')
Start-Sleep -Milliseconds 200
$originalDraft = [Windows.Forms.Clipboard]::GetText()
Write-Output "ORIGINAL_DRAFT_CAPTURED_LENGTH=$($originalDraft.Length)"

foreach ($message in $Messages) {
  Set-ComposerText -Root $root -Text $message | Out-Null
  Start-Sleep -Milliseconds 150
  [Windows.Forms.SendKeys]::SendWait('{ENTER}')
  Start-Sleep -Milliseconds 400
}

Start-Sleep -Milliseconds 400
Set-ComposerText -Root $root -Text $originalDraft | Out-Null
Start-Sleep -Milliseconds 200
Write-Output "SENT_COUNT=$($Messages.Count)"
Write-Output 'DRAFT_RESTORED=true'
