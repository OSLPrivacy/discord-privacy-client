# Phase 9-C1: permissive decrypt + tri-state whitelist

## Why

Pre-C1, OSL gated incoming decrypts behind an in-band handshake:
the sender sent a `MSG_TYPE_WHITELIST_INVITATION` (0x02), the
recipient accepted/declined via a banner, and only after acceptance
did content from that scope decrypt. The handshake was a usability
tax (banners stacked up, peers had to know they'd been invited) and
duplicated trust signal that Discord's own block-feature already
provides. C1 removes the gate.

The trust model now: **if we have the keys to decrypt, we
decrypt**. The user-facing "is this person someone I'm willing to
hear from?" decision lives in Discord's block list, not in an OSL-
internal accept queue.

## What changed

### Removed

- All `WhitelistInvitation` / `WhitelistResponse` control messages
  (was wire-v=2 msg_type 0x02 / 0x03).
- `pending_invitations.json` persistence + accept/decline banners.
- `should_decrypt_from` gate function + `incoming_decrypt_accepted`
  field on `PeerEntry`.
- The Tauri commands `osl_send_whitelist_invitation`,
  `osl_send_whitelist_response`, `osl_accept_invitation`,
  `osl_decline_invitation`, `osl_list_pending_invitations`.

### Added

- Permissive decrypt across v=2 CONTENT / v=3 / v=4 / v=5: any wire
  the receiver can decrypt with their long-term keys + ratchet state
  surfaces as plaintext.
- Legacy 0x02 / 0x03 frames are swallowed via the
  `OSL_RESULT_LEGACY_HANDSHAKE_IGNORED` sentinel so pre-C1 peers'
  un-acked invitations don't render as garbled ciphertext placeholders.
- `cmd_osl_get_scope_whitelist_summary` — returns
  `{ encrypt_toggle, whitelisted_count, total_members, state }` where
  `state` is one of `"all"` / `"some"` / `"none"` / `"unknown"`.
- `cmd_osl_bulk_set_whitelist` / `cmd_osl_bulk_unwhitelist_scope`:
  promote / demote N peers in one IPC round-trip.
- Gateway WebSocket tap in `boot.js`: maintains
  `window.__OSL_CHANNEL_MEMBERS__` (channel-id → discord-id array)
  from `READY` / `CHANNEL_CREATE` / `CHANNEL_UPDATE` /
  `GUILD_CREATE` frames, throttled to one push to Rust per channel
  per 2 s.
- Tri-state header lock icon: closed-green (all whitelisted),
  partial-yellow (some), open-gray (none), question-marked (roster
  unknown). Clicking it triggers a bulk set or unset; >25 affected
  peers requires a confirm-modal first.

### Migration

`whitelist_state.json` is migrated lossless and one-shot at boot.
Legacy `members` / `whitelisted_users` arrays on each scope are
projected into per-peer `outgoing_whitelists` on `peer_map.json`,
and the simplified `whitelist_state.json` is stamped with
`migrated_c1: true` in its envelope. Boot is idempotent — second
boot sees the marker and skips the projection step.

The migration helper lives at
[`ipc::migration::migrate_whitelist_state_in_place`](../crates/ipc/src/migration.rs).
Tests in `crates/ipc/tests/phase_c1_migration.rs`.

## Trust boundary

OSL guarantees: keys-bound recipients can decrypt; no one else can.
That's it.

OSL does NOT decide who you want to hear from. If a problem peer
gets your public key (your fault for accepting on your end, or
theirs for joining a server you're in), they can encrypt at you and
their messages will decrypt locally. Use Discord's block feature,
or use the per-peer **unwhitelist** action from the profile popout,
to stop hearing from them entirely.

The header icon's tri-state surface gives you the current overlap:
glance at it before sending to see who you'll be encrypted-to.

## Files

- `crates/ipc/src/migration.rs` — lossless migration entry point
- `crates/ipc/src/commands.rs` — bulk commands + summary + permissive recv
- `crates/ipc/src/whitelist_state.rs` — envelope shape with `migrated_c1`
- `src-tauri/src/injection/boot.js` — gateway tap + tri-state icon
- `src-tauri/src/main.rs` — Tauri wrappers + handler registration
- `crates/ipc/tests/phase_c1_*.rs` — handshake-removal / migration /
  summary / bulk test suites (18 tests total)
