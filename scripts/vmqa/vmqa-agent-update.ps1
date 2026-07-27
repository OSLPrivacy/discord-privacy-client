<#
Updates the two hash-bound VMQA runtime scripts through Azure Run Command.

This runs as SYSTEM in session 0 and performs no GUI work. The replacement
bytes come from the Entra-authenticated VMQA blob container, are verified
against host-computed SHA-256 values, and the interactive task is restarted
only after both files are atomically in place.
#>

param(
    [Parameter(Mandatory)][string]$AgentBlob,
    [Parameter(Mandatory)][string]$Win32Blob,
    [Parameter(Mandatory)][string]$ExpectedAgentSha256,
    [Parameter(Mandatory)][string]$ExpectedWin32Sha256
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$allowedVms = @('OSL-Azure-Client-1', 'OSL-Azure-Client-2')
$imdsHeaders = @{ Metadata = 'true' }
$vmName = ([string](Invoke-RestMethod -Method Get -TimeoutSec 5 `
    -Uri 'http://169.254.169.254/metadata/instance/compute/name?api-version=2021-02-01&format=text' `
    -Headers $imdsHeaders)).Trim()
if ($allowedVms -notcontains $vmName) {
    throw "VMQA_UPDATE_WRONG_MACHINE: IMDS reports '$vmName'"
}
if ($ExpectedAgentSha256 -cnotmatch '^[0-9a-f]{64}$' -or
    $ExpectedWin32Sha256 -cnotmatch '^[0-9a-f]{64}$') {
    throw 'VMQA_UPDATE_BAD_EXPECTED_HASH'
}

$tokenUri = 'http://169.254.169.254/metadata/identity/oauth2/token' +
    '?api-version=2019-08-01&resource=https%3A%2F%2Fstorage.azure.com%2F'
$token = Invoke-RestMethod -Method Get -Uri $tokenUri -Headers $imdsHeaders -TimeoutSec 10
$blobHeaders = @{
    Authorization = "Bearer $($token.access_token)"
    'x-ms-version' = '2021-08-06'
}
$container = 'https://osltestartifactsa7d5.blob.core.windows.net/vmqa'
$root = 'C:\ProgramData\OSL-VMQA'
$taskName = 'OSL-VMQA-Agent'
$staging = Join-Path $root ('update-' + [guid]::NewGuid().ToString('n'))
New-Item -ItemType Directory -Force -Path $staging | Out-Null

function Get-BlobFile {
    param(
        [Parameter(Mandatory)][string]$Blob,
        [Parameter(Mandatory)][string]$Path
    )
    $escaped = (($Blob -split '/') | ForEach-Object { [uri]::EscapeDataString($_) }) -join '/'
    Invoke-WebRequest -Method Get -Uri "$container/$escaped" -Headers $blobHeaders `
        -OutFile $Path -TimeoutSec 60 -UseBasicParsing -ErrorAction Stop
}

function Assert-Hash {
    param(
        [Parameter(Mandatory)][string]$Path,
        [Parameter(Mandatory)][string]$Expected
    )
    $actual = (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($actual -cne $Expected) {
        throw "VMQA_UPDATE_HASH_MISMATCH: path=$Path expected=$Expected actual=$actual"
    }
}

try {
    $agentStaged = Join-Path $staging 'vmqa-agent.ps1'
    $win32Staged = Join-Path $staging 'vmqa-win32.ps1'
    Get-BlobFile -Blob $AgentBlob -Path $agentStaged
    Get-BlobFile -Blob $Win32Blob -Path $win32Staged
    Assert-Hash -Path $agentStaged -Expected $ExpectedAgentSha256
    Assert-Hash -Path $win32Staged -Expected $ExpectedWin32Sha256

    Stop-ScheduledTask -TaskName $taskName -ErrorAction SilentlyContinue
    Start-Sleep -Seconds 2

    $agentTarget = Join-Path $root 'vmqa-agent.ps1'
    $win32Target = Join-Path $root 'vmqa-win32.ps1'
    Move-Item -LiteralPath $agentStaged -Destination ($agentTarget + '.new') -Force
    Move-Item -LiteralPath $win32Staged -Destination ($win32Target + '.new') -Force
    Move-Item -LiteralPath ($agentTarget + '.new') -Destination $agentTarget -Force
    Move-Item -LiteralPath ($win32Target + '.new') -Destination $win32Target -Force
    Assert-Hash -Path $agentTarget -Expected $ExpectedAgentSha256
    Assert-Hash -Path $win32Target -Expected $ExpectedWin32Sha256

    Start-ScheduledTask -TaskName $taskName
    Write-Output ("VMQA_UPDATE_OK vm={0} agentSha256={1} win32Sha256={2}" -f `
        $vmName, $ExpectedAgentSha256, $ExpectedWin32Sha256)
} finally {
    Remove-Item -LiteralPath $staging -Recurse -Force -ErrorAction SilentlyContinue
}
