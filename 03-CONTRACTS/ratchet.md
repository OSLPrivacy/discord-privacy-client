# OSL-RN ratchet integration contract

**Status:** frozen by T19-A4. This is the implementation contract for the pairwise OSL-RN (`0x10`) integration. It implements D41's functional bar: a ratchet desync must be detected, shown, recovered, and must not lose a message. It does not authorize making OSL-RN the default for all traffic; that remains a separate go/no-go decision.

## Machine-readable surface

The following data is normative. Consumers may parse it to keep the frozen names, thresholds, and invariants aligned; the explanatory sections define their operational meaning.

```ratchet-contract
{
  "health": {
    "states": ["Healthy", "Degraded", "Desynced", "Unrecoverable"],
    "auth_failures_to_desynced": 3,
    "desync_triggers": ["three_consecutive_auth_failed_from_pinned_peer", "no_session_on_file_for_pinned_peer", "skip_bound_refusal"],
    "only_successful_decrypt_clears_desynced": true,
    "durable": true
  },
  "selection": {
    "live_capability": "RN_CAP_WIRE_RN_LIVE",
    "live_capability_bit": 1,
    "unpinned_requires_verified_live_capability_for_rn": true,
    "pinned_never_selects_v3": true,
    "pin_lowering": "explicit_both_sides_confirmed_out_of_band_unpin_only"
  },
  "storage": {
    "exported_session_directory_constant": "RN_SESSION_DIR",
    "all_production_stores_use_for_config_dir": true,
    "one_writer_per_peer": true,
    "lock_loser": "named_error_no_stale_retry",
    "unique_atomic_write_temp_suffix": true
  },
  "recovery": {
    "reset_wire": "v3",
    "control_types": ["SKDM_REQUEST", "SESSION_RESET", "BOOTSTRAP_PING", "REVOCATION"],
    "throttle_arms": "confirmed_delivery",
    "session_delete_keeps_pin": true,
    "bootstrap_ping_heals_one_directional_desync": true
  },
  "outbox": {
    "encrypt_once": "at_enqueue",
    "retry": "reuse_stored_rn_wire",
    "after_heal": "reseal_queued_plaintext_against_new_rn_session_in_order",
    "storage": "sealed_local_store",
    "version_fallback": "forbidden"
  },
  "error_mapping": {
    "WireInDisabled": "RnUnavailable",
    "PinnedToRn": "RnPinnedPeerUnavailable",
    "RnRequiredButUnsupported": "RnPeerNotReady",
    "PlaintextSealerRefused": "RnSecureStorageRequired",
    "BadPeerKemKey": "RnPeerKeyInvalid",
    "PrekeyAdapter": "RnPeerKeyInvalid",
    "Protocol": "RnRecoveryRequired",
    "StateTooLarge": "RnSecureStorageRequired",
    "StoreFull": "RnStoreCapacityReached",
    "SkippedCacheTooLarge": "RnRecoveryRequired",
    "Storage": "RnSecureStorageRequired"
  }
}
```

## Health and recovery state machine

`RnSessionHealth` is durable per peer in sealed local storage. Its states are `Healthy`, `Degraded`, `Desynced`, and `Unrecoverable`.

- A successful RN decrypt sets the peer to `Healthy` and resets its consecutive-auth-failure counter.
- One or two consecutive `AuthFailed` results from a peer already pinned to RN set or retain `Degraded`; a single tampered packet must not reset a conversation.
- Exactly three consecutive such failures enter `Desynced`. A pinned peer with no session on file, or any refusal of `max_skip_per_message`, enters `Desynced` immediately.
- `Desynced` is a local, durable fact. It is never inferred from a single message and it is never cleared by anything but a successful decrypt.
- A non-retryable local condition (for example, sealed-state rollback or a storage/sealer refusal) enters `Unrecoverable`. It remains surfaced until the user completes the named remediation; it must not silently become `Healthy` or select v3.

**DETECT** is the transition set above. **SURFACE** requires `Desynced`, `Degraded`, and `Unrecoverable` to be visible before a message is discarded, with the same pending—not-green-tick—posture as D28's “Not acknowledged.” **EMIT** sends `SESSION_RESET` on v3 and only arms its throttle after confirmed delivery, never when constructing the wire. **HEAL** deletes the session while keeping the pin, re-handshakes, and uses a bootstrap ping so a one-directional desync heals even when the quiet side has no symptom.

Recovery control traffic (`SKDM_REQUEST`, `SESSION_RESET`, `BOOTSTRAP_PING`, and `REVOCATION`) **never rides the ratchet**. It always uses the stateless v3 path; placing recovery on `0x10` would make recovery depend on the broken session.

## Version selection and pinning

`RN_CAP_WIRE_RN_LIVE` is capability bit 1. It is advertised only by a build with both RN send and receive fuses open. Bit 0 remains an authenticated protocol-capability signal, but it is not sufficient to select `0x10`.

For an unpinned peer, an opportunistic send selects RN only when the peer has a verified live (bit-1) capability; otherwise it selects v3. Required policy without verified bit 1 reports `RnPeerNotReady`. A pin is raised only by a successful authenticated RN bootstrap/handshake; it is monotone thereafter. For a pinned peer, selection returns RN or a named failure—never v3, including after a peer rollback or a stripped capability.

The sole lowering path is D78's explicit, both-sides-confirmed, out-of-band unpin ceremony. Session deletion, recovery, transport errors, and elapsed time cannot lower a pin. Existing v3 conversations are not upgraded in place; a fresh bootstrap creates RN. In-flight v3 remains decryptable. D70 permits a declared fresh start for disposable existing install data, but never a silent state reset that lowers a live pin.

## Session persistence and writer exclusion

`wire_rn::RN_SESSION_DIR` is the single exported directory name. Every production send, receive, and initiation path constructs its `RnSessionStore` through `RnSessionStore::for_config_dir`; no production helper names a second session-directory literal.

Exactly one writer may load, advance, and save a peer's RN session at a time, across threads and processes. The exclusion is a per-peer lock held over the entire load → encrypt/decrypt → durable save operation. A losing writer gets a named `RnError` and must not queue or retry from stale state. Atomic writes use a unique temporary suffix as well as atomic replacement. This invariant is required because a repeated message key repeats the deterministic body nonce.

## Errors are product state, never silence

Every `RnError` variant maps to the non-empty user-visible state in the normative table. The mapping is exhaustive and new variants must add a row. **No error may map to silence.** Error handling may retain plaintext only in the sealed outbox described below; it may not fall back to v3, drop the message, or report success.

`Protocol` errors that represent authentication failure feed the health detector rather than being shown as a generic successful/no-op result. A skip-bound refusal is `Degraded`/`Desynced` with a recovery offer, not a generic authentication warning. Storage, capacity, sealer, capability, and pin failures likewise show their named state and preserve unsent content.

## Outbox and message fallback

An RN message is encrypted exactly once, at enqueue. Delivery retries reuse the stored RN wire; they never call `encrypt_rn` again. The outbox is sealed local storage and does not evict a live message.

If the stored wire cannot be delivered or the session must heal, its queued plaintext remains only in that sealed local store. After a successful re-handshake it is re-sealed against the new RN session, in original order, and then sent. This is a message fallback: a desync costs a round trip, never content. A version fallback or retry-on-v3 is forbidden.
