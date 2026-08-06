param(
    [Parameter(Mandatory = $true)]
    [string]$Text,
    [int]$Attempts = 5,
    [int]$WaitMilliseconds = 50,
    [int]$HoldMilliseconds = 0,
    [string]$ReadyPath = "",
    [int]$PreSetDelayMilliseconds = 0,
    [int]$FailSetAttemptsForQa = 0
)

$ErrorActionPreference = "Stop"
$script:QaRemainingSetFailures = $FailSetAttemptsForQa

Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;

public static class OslTask3405Clipboard {
    public const uint CF_UNICODETEXT = 13;
    public const uint GMEM_MOVEABLE = 0x0002;
    public const uint GMEM_ZEROINIT = 0x0040;

    [DllImport("user32.dll", SetLastError = true)]
    public static extern bool OpenClipboard(IntPtr hWndNewOwner);

    [DllImport("user32.dll", SetLastError = true)]
    public static extern bool CloseClipboard();

    [DllImport("user32.dll", SetLastError = true)]
    public static extern bool EmptyClipboard();

    [DllImport("user32.dll", SetLastError = true)]
    public static extern IntPtr GetClipboardData(uint uFormat);

    [DllImport("user32.dll", SetLastError = true)]
    public static extern bool IsClipboardFormatAvailable(uint format);

    [DllImport("user32.dll", SetLastError = true)]
    public static extern IntPtr SetClipboardData(uint uFormat, IntPtr hMem);

    [DllImport("kernel32.dll", SetLastError = true)]
    public static extern IntPtr GlobalAlloc(uint uFlags, UIntPtr dwBytes);

    [DllImport("kernel32.dll", SetLastError = true)]
    public static extern IntPtr GlobalLock(IntPtr hMem);

    [DllImport("kernel32.dll", SetLastError = true)]
    public static extern bool GlobalUnlock(IntPtr hMem);

    [DllImport("kernel32.dll", SetLastError = true)]
    public static extern IntPtr GlobalFree(IntPtr hMem);
}
"@

function Read-ClipboardUnicodeTextOnce {
    if (-not [OslTask3405Clipboard]::OpenClipboard([IntPtr]::Zero)) {
        throw "open clipboard failed"
    }
    try {
        if (-not [OslTask3405Clipboard]::IsClipboardFormatAvailable([OslTask3405Clipboard]::CF_UNICODETEXT)) {
            return ""
        }
        $handle = [OslTask3405Clipboard]::GetClipboardData([OslTask3405Clipboard]::CF_UNICODETEXT)
        if ($handle -eq [IntPtr]::Zero) {
            throw "get clipboard text failed"
        }
        $pointer = [OslTask3405Clipboard]::GlobalLock($handle)
        if ($pointer -eq [IntPtr]::Zero) {
            throw "lock clipboard text failed"
        }
        try {
            return [Runtime.InteropServices.Marshal]::PtrToStringUni($pointer)
        } finally {
            [void][OslTask3405Clipboard]::GlobalUnlock($handle)
        }
    } finally {
        [void][OslTask3405Clipboard]::CloseClipboard()
    }
}

function Read-ClipboardUnicodeTextWithRetries {
    param([int]$ReadAttempts)
    $last = ""
    for ($attempt = 1; $attempt -le $ReadAttempts; $attempt++) {
        try {
            return Read-ClipboardUnicodeTextOnce
        } catch {
            $last = $_.Exception.Message
            if ($attempt -lt $ReadAttempts) {
                Start-Sleep -Milliseconds $WaitMilliseconds
            }
        }
    }
    throw $last
}

function Set-ClipboardUnicodeTextOnce {
    param([string]$Value)

    if ($script:QaRemainingSetFailures -gt 0) {
        $script:QaRemainingSetFailures -= 1
        throw "QA forced set failure"
    }

    $bytes = [Text.Encoding]::Unicode.GetBytes($Value + [char]0)
    $byteCount = [UIntPtr]::new([uint32]$bytes.Length)
    $memory = [OslTask3405Clipboard]::GlobalAlloc(
        [OslTask3405Clipboard]::GMEM_MOVEABLE -bor [OslTask3405Clipboard]::GMEM_ZEROINIT,
        $byteCount
    )
    if ($memory -eq [IntPtr]::Zero) {
        throw "clipboard allocation failed"
    }

    $transferred = $false
    try {
        $destination = [OslTask3405Clipboard]::GlobalLock($memory)
        if ($destination -eq [IntPtr]::Zero) {
            throw "clipboard allocation lock failed"
        }
        try {
            [Runtime.InteropServices.Marshal]::Copy($bytes, 0, $destination, $bytes.Length)
        } finally {
            [void][OslTask3405Clipboard]::GlobalUnlock($memory)
        }

        if (-not [OslTask3405Clipboard]::OpenClipboard([IntPtr]::Zero)) {
            throw "open clipboard failed"
        }
        try {
            if (-not [OslTask3405Clipboard]::EmptyClipboard()) {
                throw "empty clipboard failed"
            }
            if ([OslTask3405Clipboard]::SetClipboardData([OslTask3405Clipboard]::CF_UNICODETEXT, $memory) -eq [IntPtr]::Zero) {
                throw "set clipboard data failed"
            }
            $transferred = $true
        } finally {
            [void][OslTask3405Clipboard]::CloseClipboard()
        }
    } finally {
        if (-not $transferred) {
            [void][OslTask3405Clipboard]::GlobalFree($memory)
        }
    }
}

function Set-ClipboardUnicodeTextWithRetries {
    param([string]$Value)

    $last = ""
    for ($attempt = 1; $attempt -le $Attempts; $attempt++) {
        try {
            Set-ClipboardUnicodeTextOnce -Value $Value
            Write-Output "set_attempt=$attempt status=ok"
            return $attempt
        } catch {
            $last = $_.Exception.Message
            Write-Output "set_attempt=$attempt status=failed reason=$last"
            if ($attempt -lt $Attempts) {
                Start-Sleep -Milliseconds $WaitMilliseconds
            }
        }
    }
    throw "set failed after $Attempts attempts: $last"
}

$saved = Read-ClipboardUnicodeTextWithRetries -ReadAttempts $Attempts
Write-Output "saved_clipboard=$saved"

if ($ReadyPath.Length -gt 0) {
    $readyParent = Split-Path -Parent $ReadyPath
    if ($readyParent.Length -gt 0) {
        New-Item -ItemType Directory -Force -Path $readyParent | Out-Null
    }
    Set-Content -LiteralPath $ReadyPath -NoNewline -Value "saved"
}

if ($PreSetDelayMilliseconds -gt 0) {
    Start-Sleep -Milliseconds $PreSetDelayMilliseconds
}

try {
    $setOutput = @(Set-ClipboardUnicodeTextWithRetries -Value $Text)
    $setAttempts = [int]$setOutput[-1]
    $staged = Read-ClipboardUnicodeTextWithRetries -ReadAttempts $Attempts
    Write-Output "staged_clipboard=$staged"
    if ($HoldMilliseconds -gt 0) {
        Start-Sleep -Milliseconds $HoldMilliseconds
    }
    $restoreOutput = @(Set-ClipboardUnicodeTextWithRetries -Value $saved)
    $restoreAttempts = [int]$restoreOutput[-1]
    $final = Read-ClipboardUnicodeTextWithRetries -ReadAttempts (20 + $Attempts)
    Write-Output "task_3405_set_attempts=$setAttempts"
    Write-Output "task_3405_restore_attempts=$restoreAttempts"
    Write-Output "final_clipboard=$final"
    exit 0
} catch {
    Write-Output "task_3405_error=$($_.Exception.Message)"
    Write-Output "task_3405_exit_code=1"
    try {
        $final = Read-ClipboardUnicodeTextWithRetries -ReadAttempts (20 + $Attempts)
        Write-Output "final_clipboard=$final"
    } catch {
        Write-Output "final_clipboard_unreadable=$($_.Exception.Message)"
    }
    exit 1
}
