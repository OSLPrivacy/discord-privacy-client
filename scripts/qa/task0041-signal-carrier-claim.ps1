param(
    [Parameter(Mandatory = $true)] [string]$AccountId,
    [Parameter(Mandatory = $true)] [string]$DataDirectory,
    [Parameter(Mandatory = $true)] [string]$EvidenceDirectory
)

$ErrorActionPreference = 'Stop'

# This verifier intentionally reads only process metadata and the pixels of the
# visible Signal window.  It never opens Signal's database or other profile data.
$expectedExe = Join-Path $env:LOCALAPPDATA 'Programs\signal-desktop\Signal.exe'
$expectedDirectory = [IO.Path]::GetFullPath($DataDirectory).TrimEnd('\\')
$allProcesses = @(Get-CimInstance Win32_Process -Filter "Name = 'Signal.exe'" | Where-Object { $_.ExecutablePath -eq $expectedExe })
$processes = @($allProcesses | Where-Object { $_.CommandLine -notmatch '(?i)(?:^|\s)--type=' })
if ($processes.Count -ne 1) { throw "TASK0041_FAIL expected exactly one primary Signal.exe; observed $($processes.Count)" }
$process = $processes[0]
$profileDirectories = @($allProcesses | ForEach-Object {
    $match = [regex]::Match($_.CommandLine, '(?i)--user-data-dir(?:=|\s+)(?:"(?<path>[^"]+)"|(?<path>\S+))')
    if ($match.Success) { [IO.Path]::GetFullPath($match.Groups['path'].Value).TrimEnd('\\') }
} | Sort-Object -Unique)
if ($profileDirectories.Count -ne 1) { throw "TASK0041_FAIL expected one Signal data directory; observed $($profileDirectories.Count)" }
$actualDirectory = $profileDirectories[0]
if (-not [string]::Equals($actualDirectory, $expectedDirectory, [StringComparison]::OrdinalIgnoreCase)) { throw "TASK0041_FAIL process directory '$actualDirectory' does not equal roster directory '$expectedDirectory'" }

$db = Join-Path $actualDirectory 'sql\db.sqlite'
if (-not (Test-Path -LiteralPath $db -PathType Leaf)) { throw "TASK0041_FAIL missing $db" }

Add-Type -AssemblyName System.Drawing
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class Task0041Native {
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT r);
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr hWnd, IntPtr hdc, uint flags);
  public struct RECT { public int Left, Top, Right, Bottom; }
}
'@
$window = Get-Process -Id $process.ProcessId | Where-Object { $_.MainWindowHandle -ne 0 } | Select-Object -First 1
if (-not $window) { throw "TASK0041_FAIL Signal PID $($process.ProcessId) has no visible top-level window" }
$rect = New-Object Task0041Native+RECT
if (-not [Task0041Native]::GetWindowRect($window.MainWindowHandle, [ref]$rect)) { throw 'TASK0041_FAIL unable to measure Signal window' }
$width = $rect.Right - $rect.Left; $height = $rect.Bottom - $rect.Top
if ($width -lt 200 -or $height -lt 200) { throw "TASK0041_FAIL implausible Signal window ${width}x${height}" }
New-Item -ItemType Directory -Force -Path $EvidenceDirectory | Out-Null
$bitmap = New-Object Drawing.Bitmap $width, $height
$graphics = [Drawing.Graphics]::FromImage($bitmap)
$graphics.CopyFromScreen($rect.Left, $rect.Top, 0, 0, (New-Object Drawing.Size($width, $height)))
$graphics.Dispose()
$png = Join-Path $EvidenceDirectory 'signal-signed-in-account.png'
$bitmap.Save($png, [Drawing.Imaging.ImageFormat]::Png); $bitmap.Dispose()
if ((Get-Item -LiteralPath $png).Length -lt 4096) { throw 'TASK0041_FAIL Signal capture is too small' }

$record = [ordered]@{
    schema = 'osl.task0041.signal-carrier-claim.v1'
    machine = $env:COMPUTERNAME
    carrier = 'Signal'
    account_id = $AccountId
    process_id = $process.ProcessId
    process_path = $process.ExecutablePath
    process_command_line = $process.CommandLine
    data_directory = $actualDirectory
    db_sqlite = $db
    db_sqlite_bytes = (Get-Item -LiteralPath $db).Length
    capture = $png
    captured_at_utc = [DateTime]::UtcNow.ToString('o')
}
$json = Join-Path $EvidenceDirectory 'roster.json'
$record | ConvertTo-Json -Depth 3 | Set-Content -LiteralPath $json -Encoding utf8
Write-Output "TASK0041_OK machine=$($record.machine) account_id=$AccountId data_directory=$actualDirectory db_sqlite_bytes=$($record.db_sqlite_bytes) capture=$png"
