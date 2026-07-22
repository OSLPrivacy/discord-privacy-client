$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class OslWhatsAppRdpWindow {
  [DllImport("user32.dll")]
  public static extern bool ShowWindowAsync(IntPtr window, int command);

  [DllImport("user32.dll")]
  public static extern bool IsIconic(IntPtr window);
}
'@

$matches = @(Get-Process mstsc -ErrorAction SilentlyContinue | Where-Object {
  $_.MainWindowHandle -ne 0 -and $_.MainWindowTitle -match '^OSL-WhatsApp-Client-[12] -'
})
if ($matches.Count -ne 2) {
  throw 'exact dedicated WhatsApp RDP window pair is unavailable or ambiguous'
}
foreach ($process in $matches) {
  [void][OslWhatsAppRdpWindow]::ShowWindowAsync($process.MainWindowHandle, 6)
}
Start-Sleep -Milliseconds 300
if (@($matches | Where-Object { -not [OslWhatsAppRdpWindow]::IsIconic($_.MainWindowHandle) }).Count -ne 0) {
  throw 'one or more WhatsApp RDP windows did not minimize'
}
[pscustomobject]@{ Status = 'whatsapp-rdp-windows-minimized'; Count = 2 } | ConvertTo-Json -Compress
