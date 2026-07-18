[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [ValidateNotNullOrEmpty()]
    [string]$VaultName,

    [ValidateRange(1, 90)]
    [int]$ExpiresInDays = 30,

    [ValidateSet(
        "discord", "telegram", "signal", "instagram", "snapchat", "x", "messenger", "whatsapp",
        "gmail", "outlook", "proton", "yahoo", "aol", "gmx", "maildotcom"
    )]
    [string[]]$Services = @(
        "discord", "telegram", "signal", "instagram", "snapchat", "x", "messenger", "whatsapp",
        "gmail", "outlook", "proton", "yahoo", "aol", "gmx", "maildotcom"
    ),

    [switch]$IncludeTotpSeed
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

if (-not (Get-Module -ListAvailable -Name Az.Accounts)) {
    throw "Az.Accounts is missing. Install-Module Az.Accounts -Scope CurrentUser"
}
if (-not (Get-Module -ListAvailable -Name Az.KeyVault)) {
    throw "Az.KeyVault is missing. Install-Module Az.KeyVault -Scope CurrentUser"
}

Import-Module Az.Accounts
Import-Module Az.KeyVault

if (-not (Get-AzContext)) {
    Connect-AzAccount | Out-Null
}

$vault = Get-AzKeyVault -VaultName $VaultName
if (-not $vault) {
    throw "Azure Key Vault '$VaultName' was not found in the active subscription."
}

$expires = (Get-Date).ToUniversalTime().AddDays($ExpiresInDays)

function Set-OslPromptedSecret {
    param(
        [Parameter(Mandatory = $true)][string]$Name,
        [Parameter(Mandatory = $true)][string]$Prompt,
        [Parameter(Mandatory = $true)][string]$Service,
        [Parameter(Mandatory = $true)][string]$Account,
        [Parameter(Mandatory = $true)][string]$Field
    )

    $secret = Read-Host $Prompt -AsSecureString
    try {
        if ($secret.Length -eq 0) {
            throw "No value was entered for '$Name'."
        }
        $tags = @{
            purpose = "osl-e2e-qa"
            service = $Service
            account = $Account
            field = $Field
        }
        Set-AzKeyVaultSecret `
            -VaultName $VaultName `
            -Name $Name `
            -SecretValue $secret `
            -Expires $expires `
            -ContentType "OSL disposable QA credential" `
            -Tag $tags | Out-Null
    }
    finally {
        if ($null -ne $secret) {
            $secret.Dispose()
        }
    }
}

foreach ($service in ($Services | Sort-Object -Unique)) {
    foreach ($accountNumber in 1..2) {
        $account = "test-$accountNumber"
        $prefix = "osl-$account-$service"
        Set-OslPromptedSecret `
            -Name "$prefix-login" `
            -Prompt "[$service / OSL Test $accountNumber] login, email, username, or phone" `
            -Service $service `
            -Account $account `
            -Field "login"
        Set-OslPromptedSecret `
            -Name "$prefix-password" `
            -Prompt "[$service / OSL Test $accountNumber] password" `
            -Service $service `
            -Account $account `
            -Field "password"
        if ($IncludeTotpSeed) {
            Set-OslPromptedSecret `
                -Name "$prefix-totp-seed" `
                -Prompt "[$service / OSL Test $accountNumber] TOTP seed (not a one-time code)" `
                -Service $service `
                -Account $account `
                -Field "totp-seed"
        }
    }
}

Write-Host "Stored OSL disposable QA secret metadata in '$VaultName'. Secret values were not printed."
Get-AzKeyVaultSecret -VaultName $VaultName |
    Where-Object { $_.Name -like "osl-test-*-*" } |
    Sort-Object Name |
    Select-Object Name, Expires, Enabled
