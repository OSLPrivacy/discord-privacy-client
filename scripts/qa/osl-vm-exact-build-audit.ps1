$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$path = 'C:\Users\osltest\Desktop\OSL Privacy\OSL Privacy.exe'
$sha256 = (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant()
$processes = @(Get-CimInstance Win32_Process -Filter "Name = 'OSL Privacy.exe'" | Where-Object {
  [int]$_.SessionId -eq 2 -and $_.ExecutablePath -ceq $path
})

[pscustomobject]@{
  Sha256 = $sha256
  ExactSession2ProcessCount = $processes.Count
  ProcessIds = @($processes.ProcessId)
} | ConvertTo-Json -Compress
