param(
    [string]$CoverText = "OSL-TASK-3410-COVER-HISTORY",
    [string]$PersonText = "OSL-TASK-3410-PERSON-CONTENT",
    [switch]$SkipHistoryClear
)

$ErrorActionPreference = "Stop"

Add-Type -AssemblyName System.Runtime.WindowsRuntime
[Windows.ApplicationModel.DataTransfer.Clipboard, Windows.ApplicationModel.DataTransfer, ContentType = WindowsRuntime] > $null
[Windows.ApplicationModel.DataTransfer.ClipboardHistoryItemsResult, Windows.ApplicationModel.DataTransfer, ContentType = WindowsRuntime] > $null
[Windows.ApplicationModel.DataTransfer.DataPackage, Windows.ApplicationModel.DataTransfer, ContentType = WindowsRuntime] > $null
[Windows.ApplicationModel.DataTransfer.StandardDataFormats, Windows.ApplicationModel.DataTransfer, ContentType = WindowsRuntime] > $null

function Await-WinRt {
    param(
        [Parameter(Mandatory = $true)]$Operation,
        [Parameter(Mandatory = $true)][Type]$ResultType
    )

    $method = [System.WindowsRuntimeSystemExtensions].GetMethods() |
        Where-Object {
            $_.Name -eq "AsTask" -and
            $_.IsGenericMethodDefinition -and
            $_.GetParameters().Count -eq 1 -and
            $_.GetParameters()[0].ParameterType.Name -eq "IAsyncOperation``1"
        } |
        Select-Object -First 1
    $task = $method.MakeGenericMethod($ResultType).Invoke($null, @($Operation))
    $task.GetAwaiter().GetResult()
}

function Set-ClipboardTextWinRt {
    param([Parameter(Mandatory = $true)][string]$Text)

    Set-Clipboard -Value $Text
}

function Get-ClipboardTextWinRt {
    $content = [Windows.ApplicationModel.DataTransfer.Clipboard]::GetContent()
    $textFormat = [Windows.ApplicationModel.DataTransfer.StandardDataFormats]::Text
    if (-not $content.Contains($textFormat)) {
        return ""
    }
    Await-WinRt $content.GetTextAsync() ([string])
}

function Get-ClipboardHistoryItems {
    $result = Await-WinRt ([Windows.ApplicationModel.DataTransfer.Clipboard]::GetHistoryItemsAsync()) ([Windows.ApplicationModel.DataTransfer.ClipboardHistoryItemsResult])
    if ($result.Status -ne "Success") {
        throw "clipboard history read status was $($result.Status)"
    }
    $result.Items
}

function Count-CoverHistoryMatches {
    param([Parameter(Mandatory = $true)][string]$Needle)

    $items = @(Get-ClipboardHistoryItems)
    $textFormat = [Windows.ApplicationModel.DataTransfer.StandardDataFormats]::Text
    $count = 0
    foreach ($item in $items) {
        $content = $item.Content
        if ($content.Contains($textFormat)) {
            $text = Await-WinRt $content.GetTextAsync() ([string])
            if ($text.Contains($Needle)) {
                $count++
            }
        }
    }
    $count
}

function Remove-CoverHistoryMatches {
    param([Parameter(Mandatory = $true)][string]$Needle)

    $items = @(Get-ClipboardHistoryItems)
    $textFormat = [Windows.ApplicationModel.DataTransfer.StandardDataFormats]::Text
    $deleted = 0
    foreach ($item in $items) {
        $content = $item.Content
        if ($content.Contains($textFormat)) {
            $text = Await-WinRt $content.GetTextAsync() ([string])
            if ($text.Contains($Needle)) {
                if ([Windows.ApplicationModel.DataTransfer.Clipboard]::DeleteItemFromHistory($item)) {
                    $deleted++
                }
            }
        }
    }
    $deleted
}

function Wait-CoverHistoryMatchesAtLeast {
    param(
        [Parameter(Mandatory = $true)][string]$Needle,
        [Parameter(Mandatory = $true)][int]$Minimum
    )

    $count = 0
    for ($attempt = 0; $attempt -lt 30; $attempt++) {
        $count = Count-CoverHistoryMatches $Needle
        if ($count -ge $Minimum) {
            return $count
        }
        Start-Sleep -Milliseconds 100
    }
    $count
}

$historyEnabled = [Windows.ApplicationModel.DataTransfer.Clipboard]::IsHistoryEnabled()
Write-Output "history_enabled=$historyEnabled"
if (-not $historyEnabled) {
    Write-Output "task_3410_exit_code=2"
    exit 2
}

$preexisting = Remove-CoverHistoryMatches $CoverText
Start-Sleep -Milliseconds 150

Set-ClipboardTextWinRt $PersonText
Start-Sleep -Milliseconds 150
$saved = Get-ClipboardTextWinRt

Set-ClipboardTextWinRt $CoverText
Start-Sleep -Milliseconds 150
$staged = Get-ClipboardTextWinRt
$beforeClear = Wait-CoverHistoryMatchesAtLeast $CoverText 1

Set-ClipboardTextWinRt $saved
Start-Sleep -Milliseconds 150
$final = Get-ClipboardTextWinRt

$deleted = 0
if (-not $SkipHistoryClear) {
    for ($attempt = 0; $attempt -lt 10; $attempt++) {
        $deleted += Remove-CoverHistoryMatches $CoverText
        Start-Sleep -Milliseconds 100
        if ((Count-CoverHistoryMatches $CoverText) -eq 0) {
            break
        }
    }
}
$afterClear = Count-CoverHistoryMatches $CoverText

Write-Output "cover_text=$CoverText"
Write-Output "person_text=$PersonText"
Write-Output "preexisting_cover_history_deleted=$preexisting"
Write-Output "saved_clipboard=$saved"
Write-Output "staged_clipboard=$staged"
Write-Output "final_clipboard=$final"
Write-Output "skip_history_clear=$($SkipHistoryClear.IsPresent)"
Write-Output "cover_history_matches_before_clear=$beforeClear"
Write-Output "cover_history_deleted=$deleted"
Write-Output "cover_history_matches_after_clear=$afterClear"

if ($staged -ne $CoverText -or $final -ne $PersonText -or $beforeClear -lt 1) {
    Write-Output "task_3410_exit_code=1"
    exit 1
}

if ($SkipHistoryClear) {
    Write-Output "task_3410_exit_code=10"
    exit 10
}

if ($afterClear -ne 0) {
    Write-Output "task_3410_exit_code=1"
    exit 1
}

Write-Output "task_3410_exit_code=0"
