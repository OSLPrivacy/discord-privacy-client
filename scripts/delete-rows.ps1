# delete-rows.ps1
#
# Deletes rows from %APPDATA%\osl\store\messages.sqlite by
# discord_message_id. Use after `find-corrupted-rows.ps1` flags
# SUSPICIOUS rows.
#
# Usage:
#     pwsh -File scripts\delete-rows.ps1 <id1> <id2> ...
#
# Confirms before deleting. Requires sqlite3.exe on PATH.

$ErrorActionPreference = "Stop"

if ($args.Count -eq 0) {
    Write-Host "Usage: pwsh -File scripts\delete-rows.ps1 <id1> <id2> ..."
    exit 1
}

# Validate ids look like Discord snowflakes (15-22 digits) before
# we ever touch the DB. Keeps a stray flag like '-Verbose' from
# slipping into the IN(...) clause.
foreach ($id in $args) {
    if ($id -notmatch '^\d{15,22}$') {
        Write-Error "Refusing to delete: '$id' is not a valid discord_message_id (expected 15-22 digits)."
        exit 1
    }
}

$db = Join-Path $env:APPDATA "osl\store\messages.sqlite"
if (-not (Test-Path $db)) {
    Write-Error "messages.sqlite not found at: $db"
    exit 1
}

$sqlite = Get-Command sqlite3 -ErrorAction SilentlyContinue
if (-not $sqlite) {
    Write-Host "sqlite3.exe not found on PATH." -ForegroundColor Red
    Write-Host "See scripts\find-corrupted-rows.ps1 for download pointer."
    exit 1
}

Write-Host "About to DELETE the following rows from $db" -ForegroundColor Yellow
foreach ($id in $args) {
    Write-Host "  $id"
}
$resp = Read-Host "Continue? (y/n)"
if ($resp -ne 'y' -and $resp -ne 'Y') {
    Write-Host "Aborted."
    exit 0
}

# Build the IN(...) list. Ids are validated digit-only above, so
# this is safe to interpolate; we still single-quote each one as a
# defence-in-depth.
$inList = ($args | ForEach-Object { "'$_'" }) -join ","
$sql = @"
BEGIN;
DELETE FROM messages WHERE discord_message_id IN ($inList);
SELECT changes();
COMMIT;
"@

$result = & sqlite3 $db $sql
$deleted = ($result | Select-Object -Last 1).Trim()

Write-Host ""
Write-Host ("Deleted {0} row(s)." -f $deleted) -ForegroundColor Green
