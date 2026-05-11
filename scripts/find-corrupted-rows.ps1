# find-corrupted-rows.ps1
#
# Lists every row in %APPDATA%\osl\store\messages.sqlite, sorted by
# decrypted_at DESC, and flags rows whose ciphertext is suspiciously
# large.
#
# Background: a broken Phase 6a edit-tab fix could double-encrypt a
# mangled ciphertext as if it were plaintext, leaving the original
# DPC0:: cover string stored as `plaintext` (encrypted under the row
# AEAD). Normal short test messages produce ct_len around 20; values
# above ~100 are almost always corrupted rows.
#
# Run from a PowerShell prompt (no admin needed):
#     pwsh -File scripts\find-corrupted-rows.ps1
#
# Requires sqlite3.exe on PATH. The script prints a download pointer
# if it can't find one.

$ErrorActionPreference = "Stop"

$db = Join-Path $env:APPDATA "osl\store\messages.sqlite"
if (-not (Test-Path $db)) {
    Write-Error "messages.sqlite not found at: $db"
    exit 1
}

$sqlite = Get-Command sqlite3 -ErrorAction SilentlyContinue
if (-not $sqlite) {
    Write-Host ""
    Write-Host "sqlite3.exe not found on PATH." -ForegroundColor Red
    Write-Host "Download the precompiled Windows tools bundle from:" -ForegroundColor Yellow
    Write-Host "    https://sqlite.org/download.html" -ForegroundColor Yellow
    Write-Host "Look for 'sqlite-tools-win-x64-*.zip', extract sqlite3.exe,"
    Write-Host "and either add its folder to PATH or copy it next to this script."
    exit 1
}

# Checkpoint the WAL into the main DB file so the SELECT below sees
# rows that were written but not yet checkpointed. Tauri may still
# have the file open; WAL checkpoints are concurrent-safe.
& sqlite3 $db "PRAGMA wal_checkpoint(FULL);" | Out-Null

# Pull rows. `datetime(decrypted_at, 'unixepoch', 'localtime')` is
# SQLite's stock helper for formatting unix-seconds in local time.
# `-separator '|'` keeps the parse simple even though message_id /
# channel_id are pure digits.
$rows = & sqlite3 -separator '|' $db @"
SELECT discord_message_id,
       channel_id,
       datetime(decrypted_at, 'unixepoch', 'localtime') AS decrypted_local,
       length(ciphertext) AS ct_len
  FROM messages
 ORDER BY decrypted_at DESC;
"@

if (-not $rows) {
    Write-Host "No rows in messages table." -ForegroundColor Green
    exit 0
}

# Header.
$fmt = "{0,-22} {1,-22} {2,-20} {3,8} {4}"
Write-Host ($fmt -f "discord_message_id", "channel_id", "decrypted_at", "ct_len", "flag")
Write-Host ($fmt -f ("-" * 22), ("-" * 22), ("-" * 20), ("-" * 8), "----")

$suspicious = 0
foreach ($line in $rows) {
    $parts = $line -split '\|'
    if ($parts.Count -ne 4) { continue }
    $mid = $parts[0]
    $ch = $parts[1]
    $dt = $parts[2]
    $ctLen = [int]$parts[3]
    $flag = ""
    $color = "Gray"
    if ($ctLen -gt 100) {
        $flag = "SUSPICIOUS"
        $color = "Yellow"
        $suspicious++
    }
    Write-Host ($fmt -f $mid, $ch, $dt, $ctLen, $flag) -ForegroundColor $color
}

Write-Host ""
if ($suspicious -gt 0) {
    Write-Host ("{0} suspicious row(s) (ct_len > 100). To delete:" -f $suspicious) -ForegroundColor Yellow
    Write-Host "    pwsh -File scripts\delete-rows.ps1 <id1> <id2> ..."
} else {
    Write-Host "No suspicious rows." -ForegroundColor Green
}
