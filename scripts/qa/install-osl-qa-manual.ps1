$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$source = '\\tsclient\C\Users\liamw\Desktop\OSL-QA-Manual\OSL Privacy.exe'
$target = 'C:\Users\osltest\Desktop\OSL Privacy\OSL Privacy.exe'
$expectedSha256 = '2065d3329f9f6999c3e51e149634c6dab660a03a5cda86cfa146465ead28410f'
$expectedSize = 43865117

function Get-Sha256([string]$Path) {
  (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
}

if (-not (Test-Path -LiteralPath $source -PathType Leaf)) {
  throw 'The redirected OSL QA build is unavailable. Reconnect RDP with the C: drive shared.'
}
if ((Get-Item -LiteralPath $source).Length -ne $expectedSize -or
    (Get-Sha256 $source) -cne $expectedSha256) {
  throw 'The redirected OSL QA build does not match the approved hash and size.'
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

$stage = "$target.manual-stage"
if (Test-Path -LiteralPath $stage) {
  Remove-Item -LiteralPath $stage -Force
}
[IO.File]::Copy($source, $stage, $false)
if ((Get-Item -LiteralPath $stage).Length -ne $expectedSize -or
    (Get-Sha256 $stage) -cne $expectedSha256) {
  Remove-Item -LiteralPath $stage -Force -ErrorAction SilentlyContinue
  throw 'The staged OSL QA build did not verify.'
}
[IO.File]::Replace($stage, $target, $null, $true)
if ((Get-Sha256 $target) -cne $expectedSha256) {
  throw 'The installed OSL QA build did not verify.'
}

$process = Start-Process -FilePath $target -WorkingDirectory ([IO.Path]::GetDirectoryName($target)) -PassThru
[pscustomobject]@{
  Status = 'installed-and-launched'
  Sha256 = $expectedSha256
  Size = $expectedSize
  Pid = $process.Id
  Backup = $backup
  DiscordTouched = $false
  ProfilesTouched = $false
} | ConvertTo-Json -Compress
