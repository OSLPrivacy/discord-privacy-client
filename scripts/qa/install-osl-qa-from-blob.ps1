$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
Add-Type -AssemblyName System.Net.Http

$storageAccount = 'osltestartifactsa7d5'
$buildBlob = 'builds/qa/a32628344dff9312/OSL-Privacy.exe'
$expectedSha256 = 'a32628344dff9312d7e5767601cbb02a801ec66c8d6c854b82631c752a0020d6'
$expectedSize = 44003107
$target = 'C:\Users\osltest\Desktop\OSL Privacy\OSL Privacy.exe'

$machine = ([string](Invoke-RestMethod -Method Get `
  -Uri 'http://169.254.169.254/metadata/instance/compute/name?api-version=2021-02-01&format=text' `
  -Headers @{ Metadata = 'true' } -TimeoutSec 10)).Trim()
$container = switch -CaseSensitive ($machine) {
  'OSL-Azure-Client-1' { 'osl-discord-qa-client1' }
  'OSL-Azure-Client-2' { 'osl-discord-qa-client2' }
  default { throw 'This installer only supports the two exact OSL Discord QA VMs.' }
}

$tokenUri = 'http://169.254.169.254/metadata/identity/oauth2/token' +
  '?api-version=2018-02-01&resource=https%3A%2F%2Fstorage.azure.com%2F'
$token = Invoke-RestMethod -Method Get -Uri $tokenUri -Headers @{ Metadata = 'true' } -TimeoutSec 10
if ([string]::IsNullOrWhiteSpace([string]$token.access_token)) {
  throw 'The VM managed-identity storage token is unavailable.'
}

$stageRoot = 'C:\ProgramData\OSL-QA\manual-install'
[void](New-Item -ItemType Directory -Path $stageRoot -Force)
$stage = Join-Path $stageRoot "$expectedSha256.exe"
if (Test-Path -LiteralPath $stage) {
  Remove-Item -LiteralPath $stage -Force
}

$client = [Net.Http.HttpClient]::new()
try {
  $client.Timeout = [TimeSpan]::FromSeconds(90)
  $client.DefaultRequestHeaders.Authorization =
    [Net.Http.Headers.AuthenticationHeaderValue]::new('Bearer', [string]$token.access_token)
  $client.DefaultRequestHeaders.Add('x-ms-version', '2023-11-03')
  $uri = "https://$storageAccount.blob.core.windows.net/$container/$buildBlob"
  $response = $client.GetAsync($uri).GetAwaiter().GetResult()
  if (-not $response.IsSuccessStatusCode) {
    throw "The QA build download failed with HTTP $([int]$response.StatusCode)."
  }
  if ($null -eq $response.Content.Headers.ContentLength -or
      [int64]$response.Content.Headers.ContentLength -ne $expectedSize) {
    throw 'The QA build response size does not match.'
  }
  $bytes = $response.Content.ReadAsByteArrayAsync().GetAwaiter().GetResult()
  if ($bytes.LongLength -ne $expectedSize) {
    throw 'The downloaded QA build size does not match.'
  }
  [IO.File]::WriteAllBytes($stage, $bytes)
} finally {
  $client.Dispose()
}

function Get-Sha256([string]$Path) {
  (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
}

if ((Get-Sha256 $stage) -cne $expectedSha256) {
  throw 'The downloaded QA build hash does not match.'
}
if (-not (Test-Path -LiteralPath $target -PathType Leaf)) {
  throw 'The existing OSL QA installation was not found.'
}

$exactProcesses = @(Get-CimInstance Win32_Process -Filter "Name = 'OSL Privacy.exe'" |
  Where-Object {
    $_.ExecutablePath -and
    [string]::Equals(
      [IO.Path]::GetFullPath([string]$_.ExecutablePath),
      $target,
      [StringComparison]::OrdinalIgnoreCase
    )
  })
if ($exactProcesses.Count -gt 1) {
  throw 'More than one exact OSL QA process is running.'
}
if ($exactProcesses.Count -eq 1) {
  Stop-Process -Id ([int]$exactProcesses[0].ProcessId) -Force
  Wait-Process -Id ([int]$exactProcesses[0].ProcessId) -Timeout 15 -ErrorAction SilentlyContinue
}

$oldSha256 = Get-Sha256 $target
$backup = "$target.pre-manual-$($oldSha256.Substring(0, 16))"
if (-not (Test-Path -LiteralPath $backup -PathType Leaf)) {
  [IO.File]::Copy($target, $backup, $false)
}
if ((Get-Sha256 $backup) -cne $oldSha256) {
  throw 'The recoverable backup did not verify.'
}

$replaceStage = "$target.manual-stage"
if (Test-Path -LiteralPath $replaceStage) {
  Remove-Item -LiteralPath $replaceStage -Force
}
[IO.File]::Copy($stage, $replaceStage, $false)
$replaceBackup = "$target.pre-replace-$($oldSha256.Substring(0, 16))"
if (Test-Path -LiteralPath $replaceBackup) {
  Remove-Item -LiteralPath $replaceBackup -Force
}
[IO.File]::Replace($replaceStage, $target, $replaceBackup, $true)
if ((Get-Sha256 $target) -cne $expectedSha256) {
  throw 'The installed OSL QA build did not verify.'
}

$process = Start-Process -FilePath $target `
  -WorkingDirectory ([IO.Path]::GetDirectoryName($target)) -PassThru
[pscustomobject]@{
  Status = 'installed-and-launched'
  Machine = $machine
  Sha256 = $expectedSha256
  Size = $expectedSize
  Pid = $process.Id
  Backup = $backup
  DiscordTouched = $false
  ProfilesTouched = $false
} | ConvertTo-Json -Compress
