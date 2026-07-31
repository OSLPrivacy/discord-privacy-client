$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

function disposable_discord_accounts_load_from_key_vault_without_logging_secrets {
  $scriptPath = Join-Path $PSScriptRoot 'osl-vm-unlock-from-key-vault.ps1'
  $output = @(& $scriptPath `
    -SelfTest 'disposable_discord_accounts_load_from_key_vault_without_logging_secrets')
  if (($output -join "`n") -cne 'ok - disposable_discord_accounts_load_from_key_vault_without_logging_secrets') {
    throw 'self-test did not prove disposable Discord Key Vault loading'
  }

  $receiptText = @(& $scriptPath -RunInternalSelfTest) -join "`n"
  $receipt = $receiptText | ConvertFrom-Json -ErrorAction Stop
  if ($receipt.Status -cne 'passed' -or
      $receipt.Test -cne 'disposable_discord_accounts_load_from_key_vault_without_logging_secrets') {
    throw 'internal self-test did not return the disposable Discord Key Vault proof receipt'
  }
  foreach ($secretFragment in @(
      'fixture-discord-credential-not-real',
      'fixture-user@example.invalid',
      'osl-test-discord-01',
      'osl-test-discord-02',
      'osl-test-discord-03'
    )) {
    if ($receiptText -cmatch [regex]::Escape($secretFragment)) {
      throw 'disposable Discord Key Vault test emitted secret-bearing material'
    }
  }
}

disposable_discord_accounts_load_from_key_vault_without_logging_secrets
Write-Output 'ok - disposable_discord_accounts_load_from_key_vault_without_logging_secrets'
