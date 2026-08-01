# Offline controls and OSL opened receipts

> **Status: partially implemented, NOT wired end to end (reconciled 2026-07-31).** Local design
> primitives exist: `control_contract.rs` has opaque signed-control and receipt-consent types,
> `message_expiry.rs` has a sealed local expiry ledger, and `receipt-status.ts` has honest
> sender-status formatting. They are not evidence that the feature is usable. In particular, the
> signed receipt transport, server-enforced wrapped-key destruction, blob reservation path, and
> user-facing control wiring required for B–F have not landed. `receipt-status.ts` has no
> production caller. `MessageStore::put` still never populates `wrapped_key`
> (`crates/store/src/lib.rs:194-206`), so there is no per-message key to destroy or withhold.
> Burn remains **state deletion**: local shredding, server-side deletion, and a cooperative peer
> request. The connected-service carrier remains readable to a holder of the recipient's long-term
> key material. Read every unqualified capability below as intended design, not current behaviour;
> never quote it as a shipping capability. See `docs/design/osl-public-claim-allowlist.md` §D.

## T2 reconciliation contract

```json
{
  "version": 1,
  "status": "partial-not-end-to-end",
  "evidence": {
    "localControlPrimitives": true,
    "senderReceiptFormatting": true,
    "receiptTransportWired": false,
    "serverEnforcedBurn": false,
    "blobReservationWired": false,
    "userFacingControlsWired": false
  }
}
```

The intended burn, expiry, and opened-receipt controls use bounded opaque signed
records. They contain commitments and timing/replay metadata only—no message
plaintext, service name, account handle, or chat title. The intended encrypted
local queue retries after reconnect; the receiver journal makes identical
delivery idempotent and rejects control-id or nonce reuse. Neither structure may
silently evict live records when full. These are still required behaviour, not a
claim that the shipping client has the complete queue or journal.

The default timed-message clock begins at the first authenticated local
open/decrypt (designed, not implemented). A fixed absolute expiry is an advanced option and may make an
unread message unrecoverable. Expiry destroys local decrypt keys and plaintext
caches and requests deletion of OSL's encrypted blob (designed, not implemented). Native carrier history,
provider copies, screenshots, exports, and backups remain unchanged.

Opened receipts mean “authenticated local open in OSL,” never native-service
Seen. The requester/sender must have Pro. A Free recipient may participate, but
must explicitly allow receipts for that friend and chat and can revoke the
grant. Group state is tracked separately per recipient. Declined or unknown
permission is unavailable; an offline consenting recipient is pending. OSL
never infers an opened receipt from presence, focus, scrolling, or native UI.

These are domain contracts and tests. Network transport and the end-user controls
are not wired. The standalone sender receipt formatter does not make receipt
state available in the product UI.
