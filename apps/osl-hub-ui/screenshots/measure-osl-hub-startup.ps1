param(
  [string]$Exe = (Join-Path ([Environment]::GetFolderPath("Desktop")) (([string][char]0xD83D + [char]0xDD12) + " Privacy\OSL Privacy\OSL Privacy.exe"))
)

$exe = $Exe
if (-not (Test-Path -LiteralPath $exe -PathType Leaf)) {
  throw "OSL Privacy executable not found: $exe"
}
$results = 1..3 | ForEach-Object {
  Get-Process "OSL Privacy" -ErrorAction SilentlyContinue | Stop-Process -Force
  Start-Sleep -Milliseconds 300
  $stopwatch = [Diagnostics.Stopwatch]::StartNew()
  $process = Start-Process -FilePath $exe -PassThru -ErrorAction Stop
  while ($stopwatch.ElapsedMilliseconds -lt 15000) {
    $process.Refresh()
    if ($process.MainWindowHandle -ne 0 -and $process.Responding) { break }
    Start-Sleep -Milliseconds 25
  }
  [pscustomobject]@{
    Run = $_
    ReadyMs = $stopwatch.ElapsedMilliseconds
    Responding = $process.Responding
    WorkingSetMB = [math]::Round($process.WorkingSet64 / 1MB, 1)
  }
}
$results | ConvertTo-Json -Compress
