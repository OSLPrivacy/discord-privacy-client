# Verify your Windows download

Use these steps to check that a Windows installer was signed by OSL's updater
key and that its bytes match the release's signed checksum list. A successful
check proves the downloaded file was signed by this key; it does not make a
general claim about the safety of the source code or replace Windows' publisher
warning. OSL Privacy is not currently Authenticode-signed.

## Download one complete release

On the [OSL Privacy releases page](https://github.com/OSLPrivacy/discord-privacy-client/releases),
open one published `hub-v...` release and download these four assets into the
same empty folder:

- the one `.exe` installer;
- that installer's matching `.exe.sig` file;
- `SHA256SUMS.txt`; and
- `SHA256SUMS.txt.minisig`.

Do not mix assets from different releases, and do not substitute the old MSI
installer. The commands below deliberately stop if the folder has anything
other than one of each required file.

## Check the installer signature

Install [minisign](https://jedisct1.github.io/minisign/) if it is not already
available, then open PowerShell in the download folder and run this block.
`RWTU6FiYc+RqO/PyvPdhpLjb+ip8852T2ITDQWbcJGfSA9kDzedzIRPZ` is OSL's
public updater key (key ID `3B6AE4739858E8D4`), compiled into the app.

```powershell
$ErrorActionPreference = "Stop"
$installer = @(Get-ChildItem -File -Filter "*.exe")
$signature = @(Get-ChildItem -File -Filter "*.exe.sig")
if ($installer.Count -ne 1 -or $signature.Count -ne 1) {
  throw "Expected exactly one installer and its .exe.sig signature"
}
if ($signature[0].Name -ne "$($installer[0].Name).sig") {
  throw "The signature does not belong to this installer filename"
}
Move-Item -LiteralPath $signature[0].FullName -Destination "$($installer[0].FullName).minisig"
minisign -Vm $installer[0].Name -P RWTU6FiYc+RqO/PyvPdhpLjb+ip8852T2ITDQWbcJGfSA9kDzedzIRPZ
```

The final command must report that verification succeeded. If it fails, do not
run the installer: download all four files again from the same release page.

The signature's authenticated `trusted comment` can name the installer before
its release-asset rename. That is expected: Minisign verifies bytes, not that
comment's filename. Do not reject a successful verification just because those
two filenames differ.

## Check the signed SHA-256 list

The release also signs its checksum list with the same public key. Verify that
list first, then compare the installer's hash with its entry:

```powershell
$checksum = Get-Item -LiteralPath "SHA256SUMS.txt"
$checksumSignature = Get-Item -LiteralPath "SHA256SUMS.txt.minisig"
minisign -Vm $checksum.Name -P RWTU6FiYc+RqO/PyvPdhpLjb+ip8852T2ITDQWbcJGfSA9kDzedzIRPZ

$matches = @(Get-Content -LiteralPath $checksum.FullName | Where-Object {
  $_ -match "^[0-9a-f]{64}  $([regex]::Escape($installer[0].Name))$"
})
if ($matches.Count -ne 1) {
  throw "The signed checksum list has no unique entry for this installer"
}
$expected = ($matches[0] -split "  ")[0].ToLowerInvariant()
$actual = (Get-FileHash -LiteralPath $installer[0].FullName -Algorithm SHA256).Hash.ToLowerInvariant()
if ($actual -cne $expected) {
  throw "FAIL: installer SHA-256 does not match the signed checksum list"
}
Write-Host "PASS: installer SHA-256 matches the signed checksum list"
```

Both Minisign commands and the final SHA-256 comparison must succeed. A
modified installer fails the installer-signature check and the SHA-256
comparison. A checksum file that has been changed after signing fails its
Minisign check.

## Build provenance

GitHub build provenance attestation is not published for this release path
yet, so there is no `gh attestation verify` command to run today. When a future
release publishes one, its release notes will name the exact attestation asset
and verification command. Do not treat the absence of that future artifact as
a successful attestation check.
