Add-Type @"
using System;
using System.Runtime.InteropServices;
public static class OslWindowCapture {
  [StructLayout(LayoutKind.Sequential)]
  public struct RECT { public int Left, Top, Right, Bottom; }
  [DllImport("user32.dll")]
  public static extern bool GetWindowRect(IntPtr hWnd, out RECT rect);
  [DllImport("user32.dll")]
  public static extern bool ShowWindow(IntPtr hWnd, int nCmdShow);
  [DllImport("user32.dll")]
  public static extern bool PrintWindow(IntPtr hWnd, IntPtr hdcBlt, uint flags);
}
"@

$process = Get-Process "OSL Privacy" -ErrorAction Stop |
  Where-Object { $_.MainWindowHandle -ne 0 } |
  Select-Object -First 1
if (-not $process) { throw "OSL Privacy window not found" }
[OslWindowCapture]::ShowWindow($process.MainWindowHandle, 9) | Out-Null
Start-Sleep -Milliseconds 500
$rect = New-Object OslWindowCapture+RECT
if (-not [OslWindowCapture]::GetWindowRect($process.MainWindowHandle, [ref]$rect)) {
  throw "Could not read OSL Privacy bounds"
}
$width = $rect.Right - $rect.Left
$height = $rect.Bottom - $rect.Top
if ($width -le 0 -or $height -le 0) { throw "Invalid OSL Privacy bounds" }

Add-Type -AssemblyName System.Drawing
$bitmap = New-Object System.Drawing.Bitmap($width, $height)
$graphics = [System.Drawing.Graphics]::FromImage($bitmap)
$hdc = $graphics.GetHdc()
$printed = [OslWindowCapture]::PrintWindow($process.MainWindowHandle, $hdc, 2)
$graphics.ReleaseHdc($hdc)
if (-not $printed) { throw "PrintWindow failed" }
$target = Join-Path $env:TEMP "osl-privacy.png"
$bitmap.Save($target, [System.Drawing.Imaging.ImageFormat]::Png)
$graphics.Dispose()
$bitmap.Dispose()
Write-Output $target
