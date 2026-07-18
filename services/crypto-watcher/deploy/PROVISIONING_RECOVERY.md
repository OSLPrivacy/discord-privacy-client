# Merchant wallet provisioning recovery

`provision-new-merchant-wallets.sh` is intentionally an initial-only,
fail-closed ceremony. It never deletes a spending wallet or rolls back to an
older address index automatically.

If the script exits after either wallet is created:

1. Do not rerun it and do not delete any local or VPS wallet file.
2. Keep `osl-crypto-watcher.service` stopped, disabled, and runtime-masked.
   Public crypto checkout must continue returning `503`.
3. Preserve these local directories as one incident set:
   - `~/.local/share/osl-crypto/merchant-backups`
   - `~/.local/share/osl-crypto/merchant-receipts`
   - `~/.local/share/osl-crypto/wallets/osl-merchant-spend-v1*`
4. Preserve, without starting the watcher:
   - `/var/lib/bitcoind/wallets/osl-watch`
   - `/var/lib/bitcoind/watch-wallet-backups`
   - `/var/lib/osl-crypto/wallets/osl-view-only*`
   - `/var/lib/osl-crypto/watch-wallet-backups`
   - `/etc/osl-crypto/watcher.env.new`, if present
   - `/etc/osl-crypto/monero-view-only-creation.receipt`, if present
5. Inventory the stage using read-only wallet calls. Compare the external BTC
   descriptor and index-0 derived address with the local public receipt. Compare
   the Monero primary address with the local public receipt. Never export a
   private Bitcoin descriptor, Monero spend key, or seed during recovery.
6. Resume only the missing forward steps after an operator review. If the
   remote watch/view material already matches the local receipts, preserve it
   and continue with address-pin validation, atomic configuration commit, and
   new coordinated backups. Do not re-import at index zero and do not replace a
   matching wallet with a blank wallet.
7. If any descriptor, primary address, network, or receipt hash differs, leave
   checkout disabled and treat both wallet sets as quarantined until a manual
   recovery proves which local spending wallet controls the remote addresses.

Before activation, copy the encrypted local spending-wallet backups and the
coordinated VPS watch-state backup set to separate encrypted offline media.
Restore both spending wallets in a clean environment and verify their first
addresses. Then run tiny BTC and XMR payment, replay, underpayment, expiry, and
spend-back canaries. A same-disk copy is not a disaster-recovery backup.
