# Disposable service accounts for OSL QA

OSL's production client must never read these credentials. They belong only to a
controlled local QA runner used to exercise two disposable accounts against a
service. The runner should retrieve each value just before a test, keep it only
in process memory, redact logs and screenshots, and discard it when the test
ends.

## Store the accounts

Open **PowerShell** on the test computer. Do not paste passwords into a command,
chat, `.env` file, or shell variable.

```powershell
Set-ExecutionPolicy -Scope Process Bypass
Install-Module Az.Accounts, Az.KeyVault -Scope CurrentUser
Connect-AzAccount
Set-AzContext -Subscription "<subscription name or ID>"
cd "<path-to-discord-privacy-client>"
./scripts/set-osl-test-account-secrets.ps1 `
  -VaultName "<vault-name>" `
  -Services discord,telegram,signal,instagram,snapchat,x,messenger,whatsapp,gmail,outlook,proton,yahoo,aol,gmx,maildotcom `
  -ExpiresInDays 30
```

The script prompts invisibly for OSL Test 1 and OSL Test 2. It creates names in
this form:

```text
osl-test-1-discord-login
osl-test-1-discord-password
osl-test-2-discord-login
osl-test-2-discord-password
```

Run it again with `-IncludeTotpSeed` only for disposable accounts whose TOTP
seed is dedicated to QA. Prefer entering ordinary one-time codes interactively
during a test. Never store recovery codes, browser cookies, session tokens, or
personal accounts in this vault.

The signed-in operator needs the Azure **Key Vault Secrets Officer** role on the
test vault. The later test runner should receive only the narrower **Key Vault
Secrets User** role and should be denied list, write, delete, and purge access.

## Confirm metadata without revealing values

```powershell
Get-AzKeyVaultSecret -VaultName "<vault-name>" |
  Where-Object Name -Like "osl-test-*-*" |
  Sort-Object Name |
  Select-Object Name, Enabled, Expires
```

## Remove the QA credentials

After account testing, remove every OSL QA secret. This creates recoverable,
soft-deleted entries under normal Key Vault retention settings:

```powershell
Get-AzKeyVaultSecret -VaultName "<vault-name>" |
  Where-Object Name -Like "osl-test-*-*" |
  ForEach-Object {
    Remove-AzKeyVaultSecret -VaultName "<vault-name>" -Name $_.Name -Force
  }
```

Permanent purge is deliberately not automated. Use the Azure portal only after
confirming that the QA run and any authorized incident review are complete.
