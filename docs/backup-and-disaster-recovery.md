# Backup and disaster-recovery boundary

This is the measured current boundary, not a resilience target. On 11 August
2026 the checked-in Cloudflare configuration, live provider API, account IAM,
Wrangler credential and storage-key graph reconciled to the same result.

## Hosted services

OSL's hosted recovery is Cloudflare D1 Time Travel only. Live D1 and recovery
share provider **Cloudflare**, **WNAM** location with no separately observed
recovery **region**, one Cloudflare **account**, the same dashboard/API **control
plane**, one super **administrator**, shared account **credentials**, and
Cloudflare-managed storage **key authority**. Isolation is **0** for provider,
region, account, control plane, administrator, credential and key authority.

A Cloudflare-wide or WNAM-correlated storage/key incident, account closure or
takeover, control-plane failure, administrator error or lockout, credential
compromise, or storage-key loss can destroy or make unavailable both current D1
state and its Time Travel history. That can lose relay and attachment metadata
and deletion/expiry state; identities, prekeys, wrapped keys, mailbox state,
licences, payment and commerce records; and administrative, licensing, payment,
mail, Telegram and watcher service secrets. Message, attachment and retained-
archive ciphertext in R2 has no observed second copy, so R2 loss can make those
payloads unrecoverable even if D1 returns. Recovery after any shared-domain
failure is not guaranteed.

Genuine disaster isolation is tracked by held task 6582. Owner ruling T7
deferred it on 11 August 2026 because genuine disaster isolation is wanted but
not funded or operated for this release.

## Backup files created on a person's device

An account export is one user-chosen file, not an OSL-operated second copy. Its
provider, region, account, control plane, administrator, credential and key
authority are whatever protects the location the person chooses. OSL observes
0 independent isolation there and guarantees 0 recovery. A shared device,
storage-provider, region, account, control-plane, administrator, credential or
key-authority failure can remove both the active account and the file, losing
identity keys, contacts, settings and message history. Losing or corrupting
either the file or its recovery phrase can also prevent import.

The uninstaller's optional Documents copy has the same boundary: it is one copy
under the Windows user's Documents storage authorities. A shared provider,
region, account, control plane, administrator, credential or key-authority
failure can lose both the installed identity/local messages and that copy.

## Deletion and erasure stay in force

This loss disclosure does not postpone deletion. Serving-bucket deletion,
backup deletion where a recoverable copy exists, and object-specific
cryptographic erasure remain active duties. A Time Travel history can restore
deleted D1 rows; the R2 deletion probe proves only the serving bucket and still
must report every failure. Task 6582 is not a reason to defer any of those
obligations.

The machine-readable measurement and reached-surface list are in
[`contracts/task-6580-backup-failure-domain-oracle.json`](../contracts/task-6580-backup-failure-domain-oracle.json).
