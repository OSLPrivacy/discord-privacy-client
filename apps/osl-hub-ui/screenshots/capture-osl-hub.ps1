param(
  [string]$Target = (Join-Path $env:TEMP "osl-privacy.png")
)

Add-Type @"
using System;
using System.Runtime.InteropServices;
public static class OslWinRect {
  [StructLayout(LayoutKind.Sequential)]
  public struct RECT { public int Left, Top, Right, Bottom; }
  [DllImport("user32.dll")]
  public static extern bool GetWindowRect(IntPtr hWnd, out RECT rect);
  [DllImport("user32.dll")]
  public static extern bool ShowWindow(IntPtr hWnd, int nCmdShow);
  [DllImport("user32.dll")]
  public static extern bool SetForegroundWindow(IntPtr hWnd);
  [DllImport("user32.dll")]
  public static extern bool PrintWindow(IntPtr hWnd, IntPtr hdcBlt, uint flags);
}
"@

$proc = Get-Process "OSL Privacy" -ErrorAction Stop |
  Where-Object { $_.MainWindowHandle -ne 0 } |
  Select-Object -First 1
if (-not $proc) { throw "OSL Privacy window not found" }
[OslWinRect]::ShowWindow($proc.MainWindowHandle, 9) | Out-Null
[OslWinRect]::SetForegroundWindow($proc.MainWindowHandle) | Out-Null
Start-Sleep -Milliseconds 500
$rect = New-Object OslWinRect+RECT
if (-not [OslWinRect]::GetWindowRect($proc.MainWindowHandle, [ref]$rect)) {
  throw "Could not read OSL Privacy bounds"
}
$width = $rect.Right - $rect.Left
$height = $rect.Bottom - $rect.Top
if ($width -le 0 -or $height -le 0) { throw "Invalid OSL Privacy bounds" }

Add-Type -AssemblyName System.Drawing
$bmp = New-Object System.Drawing.Bitmap($width, $height)
$graphics = [System.Drawing.Graphics]::FromImage($bmp)
$hdc = $graphics.GetHdc()
$printed = [OslWinRect]::PrintWindow($proc.MainWindowHandle, $hdc, 2)
$graphics.ReleaseHdc($hdc)
if (-not $printed) {
  $graphics.CopyFromScreen($rect.Left, $rect.Top, 0, 0, $bmp.Size)
}
$bmp.Save($target, [System.Drawing.Imaging.ImageFormat]::Png)
$graphics.Dispose()
$bmp.Dispose()
Write-Output $target
