$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

function disposable_discord_accounts_load_from_key_vault_without_logging_secrets {
  $scriptPath = Join-Path $PSScriptRoot 'osl-vm-unlock-from-key-vault.ps1'
  $output = @(& $scriptPath `
    -SelfTest 'disposable_discord_accounts_load_from_key_vault_without_logging_secrets')
  if (($output -join "`n") -cne 'ok - disposable_discord_accounts_load_from_key_vault_without_logging_secrets') {
    throw 'self-test did not prove disposable Discord Key Vault loading'
  }
}

disposable_discord_accounts_load_from_key_vault_without_logging_secrets
Write-Output 'ok - disposable_discord_accounts_load_from_key_vault_without_logging_secrets'
