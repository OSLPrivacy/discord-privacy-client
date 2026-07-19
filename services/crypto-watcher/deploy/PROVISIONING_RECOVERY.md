# Watch-only provisioning recovery

The supported flow has two boundaries:

1. `generate-offline-merchant-wallets.sh` creates and proves encrypted spending
   wallets on a permanently offline machine.
2. `provision-watch-only-wallets.sh` imports only the completed Bitcoin public
   descriptor and Monero address/private view key into the online VPS.

Never copy an xprv, Monero spend key, recovery words, encrypted spending-wallet
backup, or wallet passphrase to the VPS or an online workstation.

## Interrupted offline ceremony

Both output directories begin with `CEREMONY-INCOMPLETE`. If the ceremony exits
before success:

- keep the machine offline;
- do not fund, import, merge, or reuse either partial output;
- quarantine both directories as sensitive material;
- preserve them until an operator has accounted for every backup copy; and
- rerun the ceremony from the beginning with two new empty directories.

Only a transfer directory with no incomplete marker and an exact
`CEREMONY-COMPLETE` receipt bound to its verified `SHA256SUMS` is importable.

## Interrupted online import

Leave public BTC/XMR flags disabled. Keep `osl-crypto-watcher.service` stopped
and disabled until the entire state is revalidated.

The importer is designed to be rerun with the same completed bundle:

- A successful Bitcoin descriptor import is intentionally not rolled back.
  A rerun continues only if `osl-watch` contains that one exact public ranged
  descriptor at index zero and still has private keys disabled.
- Monero files created by a failed invocation are removed by its failure trap.
  Existing Monero files are accepted only with a matching root-owned creation
  receipt binding the address, restore height, view material, and BTC descriptor.
- The importer restores the prior service enable/active state after a failure.
- `watcher.env` is committed last and is never overwritten when its contents
  differ from the expected fail-closed configuration.

Before retrying, use read-only checks to verify:

- Bitcoin Core is mainnet, fully synchronized, and `blocks == headers`;
- `osl-watch` has `private_keys_enabled=false`, descriptors enabled, and no
  unexpected descriptor or transaction;
- Monero is mainnet and synchronized;
- the transfer bundle still passes its four-file manifest and completion receipt;
- the watcher and Monero wallet RPC are not publicly bound; and
- no invoice database exists unless a prior canary was intentionally started.

If any descriptor, address, receipt hash, network, or ownership check differs,
do not delete or overwrite it. Preserve the affected watch-state backups and
keep checkout disabled until an operator proves which offline wallet controls
the address.

## After a successful import

Keep the watcher disabled while verifying the pinned primary address and exact
Bitcoin descriptor. Run paid BTC and XMR canaries, replay, underpayment, expiry,
delivery acknowledgement, and callback-idempotency checks before enabling either
public asset flag. Narrow the temporary provisioning sudo grant after launch.
