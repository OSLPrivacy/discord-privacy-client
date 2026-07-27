<#
OSL VMQA bootstrap.

az vm run-command runs as SYSTEM in session 0, where Chromium/Tauri surfaces do not render and
synthetic input cannot prove an overlay over a real Discord window. The agent is therefore installed
as an interactive logon task for osltest so screenshots, window focus, and input happen in a real
rendering session.
#>

param([switch]$Force)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$StorageAccount = 'osltestartifactsa7d5'
$BlobContainer = 'vmqa'
$BlobContainerUri = "https://$StorageAccount.blob.core.windows.net/$BlobContainer"
$InteractiveUser = 'osltest'
$AgentRoot = 'C:\ProgramData\OSL-VMQA'
$TaskName = 'OSL-VMQA-Agent'
$AgentFileName = 'vmqa-agent.ps1'

function Assert-Elevated {
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = [Security.Principal.WindowsPrincipal]::new($identity)
    if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
        throw 'VMQA_BOOTSTRAP_NOT_ELEVATED: run this bootstrap from an elevated PowerShell session inside the disposable OSL QA VM.'
    }
}

function Assert-QaMachine {
    if ($env:COMPUTERNAME -notlike 'OSL-*') {
        throw "VMQA_WRONG_MACHINE: refusing to install the VMQA agent on '$env:COMPUTERNAME'. These scripts synthesize input and are only allowed on disposable OSL-* QA VMs."
    }
}

function Write-Utf8File {
    param(
        [Parameter(Mandatory)][string]$Path,
        [Parameter(Mandatory)][string]$Content
    )
    $directory = Split-Path -Parent $Path
    if ($directory -and -not (Test-Path -LiteralPath $directory)) {
        New-Item -ItemType Directory -Force -Path $directory | Out-Null
    }
    [IO.File]::WriteAllText($Path, $Content, [Text.UTF8Encoding]::new($false))
}

function Test-BlobReachability {
    try {
        $tokenUri = 'http://169.254.169.254/metadata/identity/oauth2/token' +
          '?api-version=2019-08-01&resource=https%3A%2F%2Fstorage.azure.com%2F'
        $token = Invoke-RestMethod -Method Get -Uri $tokenUri -Headers @{ Metadata = 'true' } -TimeoutSec 10
        $headers = @{
            Authorization = "Bearer $($token.access_token)"
            # Entra-authenticated Blob requests fail without an explicit storage service version.
            'x-ms-version' = '2021-08-06'
        }
        [void](Invoke-WebRequest -Method Get -Uri ($BlobContainerUri + '?restype=container&comp=list&maxresults=1') -Headers $headers -TimeoutSec 20 -UseBasicParsing -ErrorAction Stop)
        return 'ok'
    } catch {
        return 'failed'
    }
}

Assert-Elevated
Assert-QaMachine

New-Item -ItemType Directory -Force -Path $AgentRoot | Out-Null

$sourceAgent = Join-Path $PSScriptRoot $AgentFileName
if (-not (Test-Path -LiteralPath $sourceAgent -PathType Leaf)) {
    throw "VMQA_AGENT_SOURCE_MISSING: expected '$sourceAgent' beside the bootstrap."
}

$targetAgent = Join-Path $AgentRoot $AgentFileName
Copy-Item -LiteralPath $sourceAgent -Destination $targetAgent -Force
$agentSha = (Get-FileHash -LiteralPath $targetAgent -Algorithm SHA256).Hash.ToLowerInvariant()
Write-Utf8File -Path (Join-Path $AgentRoot 'agent.sha256') -Content ($agentSha + [Environment]::NewLine)

$blobReachability = Test-BlobReachability

$existingTask = Get-ScheduledTask -TaskName $TaskName -ErrorAction SilentlyContinue
if ($existingTask -and -not $Force) {
    $taskState = 'exists'
    Write-Output 'task=exists'
} else {
    if ($existingTask) {
        Unregister-ScheduledTask -TaskName $TaskName -Confirm:$false
    }

    $action = New-ScheduledTaskAction `
        -Execute 'powershell.exe' `
        -Argument ('-NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File "' + $targetAgent + '"')
    $trigger = New-ScheduledTaskTrigger -AtLogOn -User $InteractiveUser
    # Interactive is mandatory: S4U and service logons cannot render the Chromium overlay or
    # synthesize real input in the logged-in desktop that Discord occupies.
    $principal = New-ScheduledTaskPrincipal `
        -UserId $InteractiveUser `
        -LogonType Interactive `
        -RunLevel Highest
    $settings = New-ScheduledTaskSettingsSet `
        -RestartCount 999 `
        -RestartInterval (New-TimeSpan -Minutes 1) `
        -ExecutionTimeLimit ([TimeSpan]::Zero) `
        -AllowStartIfOnBatteries `
        -DontStopIfGoingOnBatteries `
        -DontStopOnIdleEnd `
        -StartWhenAvailable

    Register-ScheduledTask `
        -TaskName $TaskName `
        -Action $action `
        -Trigger $trigger `
        -Principal $principal `
        -Settings $settings `
        -Description 'OSL VMQA interactive agent for visual Discord overlay testing.' | Out-Null
    $taskState = 'registered'
}

Write-Output ("bootstrap=ok blob={0} task={1} agentSha={2}" -f $blobReachability, $taskState, $agentSha.Substring(0, 12))
