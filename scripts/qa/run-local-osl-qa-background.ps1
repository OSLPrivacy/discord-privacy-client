param(
  [Parameter(Mandatory = $true)]
  [string]$ExePath
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

Add-Type @"
using System;
using System.Runtime.InteropServices;
public static class OslQaBackgroundWindow {
  [DllImport("user32.dll")]
  public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")]
  public static extern bool ShowWindowAsync(IntPtr hwnd, int command);
  [DllImport("user32.dll")]
  public static extern bool SetWindowPos(
    IntPtr hwnd, IntPtr after, int x, int y, int width, int height, uint flags
  );
}
"@

$exactPath = [IO.Path]::GetFullPath($ExePath)
if (-not (Test-Path -LiteralPath $exactPath -PathType Leaf)) {
  throw 'The standalone QA executable is unavailable.'
}

$before = [OslQaBackgroundWindow]::GetForegroundWindow()
$process = Start-Process -FilePath $exactPath `
  -WorkingDirectory ([IO.Path]::GetDirectoryName($exactPath)) `
  -WindowStyle Minimized -PassThru

$deadline = [DateTime]::UtcNow.AddSeconds(15)
$hwnd = [IntPtr]::Zero
while ([DateTime]::UtcNow -lt $deadline -and $hwnd -eq [IntPtr]::Zero) {
  Start-Sleep -Milliseconds 100
  $process.Refresh()
  if ($process.HasExited) {
    throw "The standalone QA process exited before creating a window: $($process.ExitCode)"
  }
  $candidate = $process.MainWindowHandle
  if ($null -ne $candidate) {
    $hwnd = [IntPtr]$candidate
  }
}
if ($hwnd -eq [IntPtr]::Zero) {
  throw 'The standalone QA main window was not created.'
}

# Restore real geometry without activating or foregrounding the QA window.
[void][OslQaBackgroundWindow]::ShowWindowAsync($hwnd, 4)
$noActivateNoMoveNoSizeShow = [uint32](0x0010 -bor 0x0002 -bor 0x0001 -bor 0x0040)
[void][OslQaBackgroundWindow]::SetWindowPos(
  $hwnd, [IntPtr]1, 0, 0, 0, 0, $noActivateNoMoveNoSizeShow
)
Start-Sleep -Milliseconds 250
$after = [OslQaBackgroundWindow]::GetForegroundWindow()

[pscustomobject]@{
  Pid = $process.Id
  Hwnd = $hwnd.ToInt64()
  ForegroundUnchanged = $before -eq $after
  Path = $exactPath
} | ConvertTo-Json -Compress
