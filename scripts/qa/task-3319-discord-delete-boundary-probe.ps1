param(
  [Parameter(Mandatory = $true)]
  [string]$ReceiptPath,
  [int]$MinimumDistinctRgb = 32
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

if ($MinimumDistinctRgb -lt 32) { throw 'TASK3319 refuses a colour floor below 32' }
Add-Type -AssemblyName System.Drawing
Add-Type -AssemblyName System.Windows.Forms

# This is a Windows observation, never a Linux process-list substitute.
$tasklist = & tasklist.exe /FO CSV /NH
if ($LASTEXITCODE -ne 0) { throw 'TASK3319 tasklist.exe observation failed' }
$discord = @(Get-Process -Name Discord -ErrorAction SilentlyContinue)
if ($discord.Count -ne 1) { throw "TASK3319 requires exactly one Discord process; found $($discord.Count)" }
if ($tasklist -notmatch ('"Discord.exe"')) { throw 'TASK3319 tasklist.exe did not observe Discord.exe' }
if ($discord[0].MainWindowHandle -eq [IntPtr]::Zero) { throw 'TASK3319 Discord has no interactive window' }

$bounds = [Windows.Forms.Screen]::PrimaryScreen.Bounds
$bitmap = [Drawing.Bitmap]::new($bounds.Width, $bounds.Height)
$graphics = [Drawing.Graphics]::FromImage($bitmap)
try {
  $graphics.CopyFromScreen($bounds.Location, [Drawing.Point]::Empty, $bounds.Size)
  $colours = [Collections.Generic.HashSet[string]]::new()
  for ($y = 0; $y -lt $bitmap.Height; $y += [Math]::Max(1, [int]($bitmap.Height / 200))) {
    for ($x = 0; $x -lt $bitmap.Width; $x += [Math]::Max(1, [int]($bitmap.Width / 200))) {
      $pixel = $bitmap.GetPixel($x, $y)
      [void]$colours.Add(('{0},{1},{2}' -f $pixel.R, $pixel.G, $pixel.B))
    }
  }
  if ($colours.Count -lt $MinimumDistinctRgb) { throw "TASK3319 CopyFromScreen has $($colours.Count) distinct RGB colours below $MinimumDistinctRgb" }
  $directory = Split-Path -Parent $ReceiptPath
  [IO.Directory]::CreateDirectory($directory) | Out-Null
  [pscustomobject]@{
    task = 3319
    source = 'windows-powershell-tasklist-copyfromscreen'
    operation = 'discord-single-message-delete'
    finiteDeadline = $null
    eligibility = 'no-finite-limit'
    discordPid = $discord[0].Id
    tasklistObserved = $true
    copyFromScreenDistinctRgb = $colours.Count
    observationTools = @('tasklist.exe', 'Windows PowerShell', 'CopyFromScreen')
  } | ConvertTo-Json -Depth 3 | Set-Content -LiteralPath $ReceiptPath -Encoding utf8
} finally {
  $graphics.Dispose()
  $bitmap.Dispose()
}
