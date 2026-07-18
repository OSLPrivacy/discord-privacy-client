# OSL Privacy feature-parity map

Status: implementation map, not a product claim
Source of truth: the 127 commands registered in `src-tauri/src/main.rs`

This document maps the existing Discord OSL implementation into the new
standalone OSL Privacy. It deliberately separates reusable protocol/state code from
Discord-specific observation and delivery. A feature is not considered
present in the app merely because its Rust implementation exists elsewhere in
the repository.

## Broker status legend

| Status | Meaning |
| --- | --- |
| `linked` | The app's trusted local broker exposes the production operation with the intended trust boundary. |
| `bootstrap linked; trusted UI pending` | The app runs the original bootstrap against its own `ipc::AppState`; the operation still needs a narrow trusted-local UI or broker command before it is usable. |
| `needs platform adapter` | The operation also needs authenticated visible context or delivery from a specific service. It must not be callable directly by the remote service document. |
| `deliberately guarded` | Raw, legacy, destructive, diagnostic, or authority-expanding operation that should remain unavailable to service pages and require a narrow trusted-local flow if retained. |

The app now links the original `ipc` core, runs the original production
bootstrap, and owns a context-bound local broker for message
preparation/decryption. On a password-protected install it remains locked and
does not advertise protection until the trusted local gate has installed the
file-storage key and reloaded encrypted state. The Windows prototype currently
reports `passwordRequired`, which is the expected fail-closed state.

## Non-negotiable trust boundaries

1. Remote service pages receive **no Tauri capability**. The existing broad
   `src-tauri/capabilities/main.json` must not be copied into the app.
2. Plaintext enters Rust only from the trusted OSL Privacy composer. Decrypted
   plaintext returns only to a trusted local overlay, never into the remote
   page's JavaScript context.
3. A platform adapter may provide validated visible context and place an
   already-produced capsule. It cannot access identity secrets, ratchet state,
   password APIs, raw crypto primitives, burns, or key-change decisions.
4. Every operation is bound to a local context token containing service,
   platform account, conversation kind, conversation ID, and generation.
   Switching any of those invalidates plaintext drafts and pending capsules.
5. The current process-global active-account directory supports one active
   OSL identity at a time. Multiple linked platform accounts are UI/profile
   records until account state is refactored into explicit per-account
   contexts.
6. Group sender-key wire v5 stays disabled. Its current chain identity omits a
   physical device ID and can desynchronise when one account runs on two
   devices.
7. OSL burn revokes decryptability and clears OSL-managed state. It does not
   mean the native service deleted its own message history. Native deletion is
   a separately reported platform-adapter result.
8. The ciphertext relay cannot decrypt content, but it can observe ciphertext
   size, request time, source network metadata, TTL, and blob access patterns.

## Original feature families

| Feature family | Reusable production source | OSL Privacy broker status | Critical implementation notes |
| --- | --- | --- | --- |
| Cryptographic primitives | `crates/crypto/src/{aead,x25519,ml_kem_768,hkdf,ed25519,padding}.rs` | `bootstrap linked; trusted UI pending` | Never expose primitive oracles to a remote origin. Keep associated data bound to account and conversation context. |
| Hybrid session setup | `crates/crypto/src/pqxdh.rs`, `crates/keystore/src/prekeys.rs` | `bootstrap linked; trusted UI pending` | Custom, unaudited construction; retain the high-risk alpha disclosure. |
| DM forward secrecy | `crates/crypto/src/ratchet.rs`, persisted through `crates/ipc/src/peer_map.rs` | `bootstrap linked; trusted UI pending` | Persist the ratchet atomically after every state transition; key changes require an explicit trusted-local decision. |
| Group encryption | `crates/crypto/src/sender_keys.rs`, `crates/ipc/src/sender_key_state.rs` | `deliberately guarded` | v5 is disabled for the known multi-device desynchronisation problem. Use v3 fan-out until device-bound sender keys exist. |
| Text capsules | `crates/ipc/src/wire_v2.rs`, `crates/ipc/src/commands.rs` | `needs platform adapter` | Current external shape is `DPC0::<base64>`. Validate each platform's text limits before placement. |
| Stego/prose carriers | `crates/stego`, `crates/ipc/src/prose_token.rs` | `deliberately guarded` | Mode 1 is retired/coerced to Mode 0 in the current v2 dispatcher. Do not imply conversation-level cover coherence. |
| Encrypted images/files | `crates/crypto/src/attachment.rs`, `crates/ipc/src/attachment_wire.rs` | `needs platform adapter` | Platform upload/download is separate from sealing/opening; plaintext bytes stay in trusted memory only. |
| Ciphertext relay | `crates/ipc/src/cipher_store_client.rs`, `cipher-store-cf` | `bootstrap linked; trusted UI pending` | Capability-token protected, TTL-bounded ciphertext; metadata caveat above still applies. |
| Local history | `crates/store/src/lib.rs`, `crates/store/SECURITY.md` | `bootstrap linked; trusted UI pending` | Bodies are AEAD-encrypted; IDs, sender IDs, channel IDs and timestamps remain plaintext SQLite metadata. No plaintext FTS. |
| Identity and sealing | `crates/keystore/src/{identity,storage,sealer}.rs` | `bootstrap linked; trusted UI pending` | Prefer TPM, then OS keyring. Refuse silent downgrade in production UI; the fallback posture must be visible. |
| Safety numbers and TOFU | `crates/ipc/src/{peer_map,commands}.rs` | `bootstrap linked; trusted UI pending` | Key changes never silently replace the trusted baseline. Decisions belong to a trusted local window. |
| Scopes and encryption toggles | `crates/ipc/src/{scope,whitelist,whitelist_state}.rs` | `needs platform adapter` | Existing kinds are Discord-shaped (`dm`, `gc`, `server_channel`, `server_full`); introduce a service-neutral scope adapter rather than changing stable storage keys in place. |
| Membership, friends and server defaults | `crates/ipc/src/{membership,whitelist_state,commands}.rs` | `needs platform adapter` | Platform membership is an untrusted observation. The trusted broker validates and accrues it; a service page cannot grant itself recipients. |
| Main password and recovery | `crates/ipc/src/{main_password,state_reload}.rs` | `bootstrap linked; trusted UI pending` | Password is primarily a local UX/at-rest gate, not the identity-key KDF. Post-unlock reload is mandatory or encrypted state appears empty. |
| Stealth, duress and account burn | `crates/ipc/src/main_password.rs`, `crates/keystore/src/duress.rs` | `deliberately guarded` | Destructive and privacy-sensitive. Require local UI, explicit sequencing, journaled outcomes, and honest forensic limitations. |
| Scope/message burn and TTL | `crates/ipc/src/{commands,burned_scopes_file,scope_ttl_file,scope_blobs_file}.rs` | `needs platform adapter` | Burn is cryptographic/local revocation, not native deletion. Some GC-per-user and server-wide local deletion paths remain unimplemented. |
| Multi-account and migration | `src-tauri/src/bootstrap.rs`, `crates/keystore/src/recipients.rs` | `bootstrap linked; trusted UI pending` | The app reuses the canonical bootstrap source without copying it, but account binding is still Discord-snowflake-specific and the active directory is process-global. |
| Import/export | `crates/ipc/src/commands.rs` account export/import paths | `deliberately guarded` | Trusted local flow only. Stage, validate identity binding, install atomically, then reload; never accept a service-origin path. |
| Licensing/tier state | `crates/ipc/src/{license_lifecycle,tier_gate}.rs`, `crates/keystore/src/license_cache.rs` | `bootstrap linked; trusted UI pending` | Keep entitlement separate from cryptographic correctness. Text currently remains unconditional; attachments are tier-gated. |
| Onboarding/preferences | `crates/ipc/src/app_preferences.rs`, OSL Privacy `preferences.rs` | `bootstrap linked; trusted UI pending` | Merge schemas deliberately. The preview's send/placement settings are not a substitute for production OSL preferences. |
| Screenshot protection | `src-tauri/src/screenshot.rs`, `crates/runtime/src/screenshot.rs` | `needs platform adapter` | Windows capture resistance is not screenshot prevention; it cannot stop cameras, malware, or modified recipients. |
| Updates, deep links and lifecycle | `src-tauri/src/main.rs`, `crates/ipc/src/commands.rs` | `deliberately guarded` | Signed updater and local deep-link parsing belong to the trusted shell. Test-only deep-link command should not ship as general authority. |

## Complete 127-command disposition

The names below exactly match `tauri::generate_handler!` in the existing
client. Counts total 127. A grouped status is the minimum work still required
for the app; individual commands may receive an even narrower capability when
implemented.

| # | Existing commands | Count | Reusable source | OSL Privacy broker status | Critical caveat |
| --- | --- | ---: | --- | --- | --- |
| 1 | `generate_identity`, `load_identity`, `save_identity`, `init_keyserver`, `register`, `fetch_pubkeys` | 6 | `crates/ipc/src/commands.rs`, `crates/keystore` | `bootstrap linked; trusted UI pending` | Identity creation/registration must be confirmed locally and bound to the intended OSL account, never inferred from a remote login. |
| 2 | `aead_seal`, `aead_open`, `stego_encode`, `stego_decode`, `status`, `x25519_diffie_hellman` | 6 | `crates/ipc/src/commands.rs`, `crates/crypto`, `crates/stego` | `deliberately guarded` | Do not expose raw cryptographic or decoding oracles to service content. Replace broad diagnostics with a redacted OSL Privacy health DTO. |
| 3 | `set_screenshot_protection` | 1 | `src-tauri/src/screenshot.rs` | `needs platform adapter` | Apply to trusted OSL Privacy windows with explicit user state; document its capture-resistance limits. |
| 4 | `osl_encrypt_message`, `osl_decrypt_message`, `osl_load_channel_history`, `osl_burn_message`, `osl_persist_edit`, `osl_persist_outbound` | 6 | `crates/ipc/src/commands.rs`, `crates/store` | `deliberately guarded` | The first two are legacy/Discord-oriented entry points. History/persistence must be re-exposed through account-bound OSL Privacy DTOs, not copied wholesale. |
| 5 | `osl_encrypt_message_v2` | 1 | `crates/ipc/src/commands.rs` | `needs platform adapter` | Use behind the trusted composer broker after service/account/scope validation; return capsules only to the placement adapter. |
| 6 | `osl_seal_attachment`, `osl_open_attachment`, `osl_seal_attachment_with_cover_v2`, `osl_open_attachment_v2`, `osl_encrypt_attachment_envelope`, `osl_seal_attachment_with_cover_v3`, `osl_fetch_attachment_bytes` | 7 | `crates/ipc/src/attachment_wire.rs`, `crates/ipc/src/commands.rs`, `crates/crypto/src/attachment.rs` | `needs platform adapter` | Never pass plaintext attachment bytes through the remote page. Separate seal/open from visible upload/download. |
| 7 | `osl_send_burn_marker`, `osl_apply_burn`, `osl_unwhitelist_scope`, `osl_local_unwhitelist_scope`, `osl_set_whitelist`, `osl_get_scope_encryption_state`, `osl_get_scope_whitelist_summary`, `osl_bulk_set_whitelist`, `osl_bulk_unwhitelist_scope`, `osl_toggle_scope_encryption`, `osl_set_scope_encrypt` | 11 | `crates/ipc/src/{commands,whitelist,whitelist_state,scope}.rs` | `needs platform adapter` | Marker delivery and recipient resolution require verified platform context. Local-only removal is not retroactive revocation. Burn is not native deletion. |
| 8 | `osl_get_self_user_id`, `osl_register_self_snowflake`, `osl_get_identity_info` | 3 | `crates/ipc/src/commands.rs`, `crates/keystore/src/identity.rs` | `bootstrap linked; trusted UI pending` | Replace Discord snowflake registration with an explicit service-neutral identity-link ceremony; never trust a remote page's claimed self ID. |
| 9 | `osl_validate_license`, `osl_get_license_state`, `osl_clear_license`, `osl_get_tier_gate_status` | 4 | `crates/ipc/src/{commands,license_lifecycle,tier_gate}.rs` | `bootstrap linked; trusted UI pending` | Validation performs network work and belongs in a bounded background task. Clearing entitlement requires local confirmation. |
| 10 | `osl_list_all_whitelists` | 1 | `crates/ipc/src/commands.rs` | `bootstrap linked; trusted UI pending` | Return only the active identity's records; never merge identities merely for UI convenience. |
| 11 | `osl_password_status`, `osl_set_main_password`, `osl_change_main_password`, `osl_remove_main_password`, `osl_view_recovery_phrase`, `osl_view_identity_recovery_phrase`, `osl_recover_identity_from_phrase` | 7 | `crates/ipc/src/{commands,main_password,recovery}.rs` | `deliberately guarded` | Trusted local windows only. Phrase display/recovery must resist shoulder-surfing, logging and replay. Password removal changes at-rest exposure. |
| 12 | `osl_switch_account`, `osl_export_data`, `osl_import_data` | 3 | `src-tauri/src/bootstrap.rs`, `crates/ipc/src/commands.rs` | `bootstrap linked; trusted UI pending` | Current switch is Discord-snowflake-specific and process-global. Import/export must be staged, validated and installed atomically. |
| 13 | `osl_ensure_recovery_phrase`, `osl_verify_main_password`, `osl_verify_recovery_phrase`, `osl_set_main_password_after_recovery`, `osl_lockout_status`, `osl_set_stealth_password`, `osl_remove_stealth_password`, `osl_stealth_password_status`, `osl_set_burn_password`, `osl_remove_burn_password`, `osl_burn_password_status`, `osl_verify_gate_password` | 12 | `crates/ipc/src/{commands,main_password,state_reload}.rs` | `bootstrap linked; trusted UI pending` | Gate success must install the file-storage key and reload encrypted state. Preserve shared lockout semantics across main/stealth/burn roles. |
| 14 | `osl_stealth_mode_engage`, `osl_burn_engage`, `osl_burn_scope_data` | 3 | `crates/ipc/src/commands.rs`, `crates/keystore/src/duress.rs`, `crates/store` | `deliberately guarded` | Destructive/local concealment authority. Journal outcomes and surface partial failure; do not claim forensic erasure or platform deletion. |
| 15 | `osl_control_inbox_post`, `osl_control_inbox_drain`, `osl_attachment_cache_put`, `osl_attachment_cache_get` | 4 | `crates/ipc/src/{commands,control_messages}.rs`, local cache paths | `needs platform adapter` | Control delivery comes from untrusted transport and must be authenticated before mutation. Cache reads must remain account/scope bound. |
| 16 | `osl_mark_scope_burned`, `osl_unburn_scope`, `osl_list_burned_scopes` | 3 | `crates/ipc/src/{commands,burned_scopes_file}.rs` | `bootstrap linked; trusted UI pending` | `unburn` permits future decryption only; it cannot restore destroyed wrapped keys or erase prior disclosure. Parse/load failure must not silently revive burned data. |
| 17 | `osl_membership_update`, `osl_membership_get`, `osl_note_scope_membership`, `osl_get_server_whitelist_state`, `osl_set_server_header_whitelist`, `osl_set_server_lock`, `osl_set_channel_whitelist`, `osl_set_friend_ids`, `osl_get_friend_ids`, `osl_set_guild_list`, `osl_get_guild_list`, `osl_bulk_set_dm_whitelist`, `osl_set_server_default`, `osl_get_server_defaults`, `osl_apply_server_default_to_existing_channels` | 15 | `crates/ipc/src/{commands,membership,whitelist_state}.rs` | `needs platform adapter` | Discord server/guild semantics need service-neutral equivalents. Treat observed rosters as untrusted input and require an OSL-key-capable recipient before encryption. |
| 18 | `osl_get_app_preferences`, `osl_set_app_preferences`, `osl_tour_get_state`, `osl_tour_advance`, `osl_tour_complete`, `osl_tour_skip`, `osl_tour_reset` | 7 | `crates/ipc/src/{commands,app_preferences}.rs` | `bootstrap linked; trusted UI pending` | Device-level preferences must not be accidentally placed under an active account directory. Migrate the app preview schema explicitly. |
| 19 | `osl_take_last_persist_error`, `osl_take_registration_alert`, `osl_list_key_change_alerts`, `osl_accept_key_change`, `osl_decline_key_change` | 5 | `crates/ipc/src/commands.rs`, `crates/ipc/src/state.rs` | `bootstrap linked; trusted UI pending` | Alerts are security state, not disposable toasts. Key-change accept/decline requires trusted local confirmation and safety-number context. |
| 20 | `osl_reset_v4_session`, `osl_reset_v5_sender_key`, `osl_build_skdm_request`, `osl_build_session_reset`, `osl_recover_peer_identity`, `osl_peer_safety_number`, `osl_self_safety_number` | 7 | `crates/ipc/src/commands.rs`, `crates/crypto/src/{ratchet,sender_keys}.rs` | `deliberately guarded` | Session resets are authenticated control operations. v5 reset/build stays unavailable while multi-device sender keys are disabled. |
| 21 | `osl_open_settings_window`, `osl_open_account_burn_window`, `osl_close_settings_window_if_open`, `osl_test_deep_link` | 4 | `src-tauri/src/main.rs`, local window capabilities | `deliberately guarded` | Rebuild as local OSL Privacy routes/dialogs. Do not grant remote pages window creation, burn-window, or arbitrary deep-link authority. Retire test-only command in production. |
| 22 | `osl_check_for_updates`, `osl_install_update`, `osl_get_update_channel`, `osl_set_update_channel` | 4 | `src-tauri/src/main.rs`, updater plugin, `crates/ipc/src/commands.rs` | `bootstrap linked; trusted UI pending` | Check can be background/read-only; install and channel changes require a trusted local action and signature verification. |
| 23 | `osl_prose_token_send`, `osl_prose_token_recv`, `osl_prose_token_burn`, `osl_get_scope_ttl`, `osl_set_scope_ttl`, `osl_scope_burn_blobs`, `osl_is_decommissioned` | 7 | `crates/ipc/src/{prose_token,cipher_store_client,scope_ttl_file,scope_blobs_file}.rs`, `src-tauri/src/main.rs` | `deliberately guarded` | Prose relay is not the primary wire path. TTL/burn must report server and local outcomes separately; decommission is a fail-closed lifecycle gate. |

Total: **127 commands**.

## Smallest safe implementation order

1. **Complete:** the app owns one `ipc::AppState` and reuses the original
   non-Tauri bootstrap/account lifecycle. Move the shared source physically
   into the IPC crate before release; the prototype currently links the one
   canonical source file to avoid a fork.
2. Link trusted-local gate, identity-status, self safety-number, whitelist-list,
   and scope-status commands. Do not add a remote capability.
3. Add a service-neutral `ServiceScope` and short-lived context-token broker.
   Keep conversion to existing `ScopeInput` inside Rust.
4. Link `osl_encrypt_message_v2` and `osl_decrypt_message_v2` behind that
   broker. Test cross-account, cross-service, stale-generation and locked-state
   rejection before enabling placement.
5. Add platform adapters for visible text capsule placement and observation.
   Keep native/send mechanics separate from cryptography.
6. Add attachment sealing/opening, then authenticated control messages and
   local history.
7. Add whitelist mutation, TTL and burn only after delivery outcomes can be
   reported distinctly from local cryptographic outcomes and native-platform
   deletion.
8. Refactor global account state into explicit per-account contexts before
   advertising simultaneous active identities. Keep v5 group sender keys off
   until physical-device binding and multi-device tests exist.

## Current Hub backend parity checkpoint

This table describes executable Hub backend behavior, not UI availability and
not future platform-adapter claims.

| User-visible family | Current trusted-local backend | Honest boundary |
| --- | --- | --- |
| Friends and nicknames | **Wired.** Signed friend codes, device-local encrypted nicknames, safety-number verification, explicit scope permission and pending-key replacement all reuse the original peer/key state. | A service username is not proof of OSL identity. The user must approve and verify the OSL friend locally. |
| Key changes | **Wired.** A changed signed bundle is held pending and cannot encrypt until its safety number is explicitly verified. | No silent trust-on-change. The Hub cannot verify a person through Instagram, Discord, or another service automatically. |
| Main password | **Wired.** Original Argon2id marker, shared lockout, encrypted-state reload and Cloudflare re-registration are used. | The password is a local at-rest/access gate, not the source of E2EE identity keys. |
| Stealth password | **Configuration wired.** Status, set and remove use the original marker and require the ordinary main password. | Login-time stealth behavior remains disabled until the Hub has a distinct, tested landing state. The backend reports `stealthActionWired=false`. |
| Burn password | **Configuration wired.** Status, set and remove use the original marker and require the ordinary main password. | Password entry does not yet trigger destruction. The backend reports `burnActionWired=false`; destructive Hub cleanup remains a separate confirmed command. |
| Text encryption | **Wired behind exact context.** Original peer encryption/decryption and local single-device protection require a generation-bound service/account/conversation lease. | Automated service placement is disabled until an adapter proves the exact native composer. Local loopback is not described as person-to-person E2EE. |
| Attachments | **Prepare/open wired behind exact context.** The original bounded attachment core seals to the current verified recipient set and opens only in the active trusted context. | Output is ciphertext ready for a later adapter/manual upload. The backend explicitly reports `automaticServiceUpload=false`; it never claims the file was sent. |
| Absolute timer | **Wired.** Scope TTL is stored locally and checked before local protected content is returned. | The current timer starts at creation, not first authenticated remote open, and does not delete native-service history. |
| View once | **Wired for single-device local protection.** First successful open atomically removes the local authorisation record; policy mismatch fails closed. | Peer/device view-once delivery is still blocked on authenticated opened receipts. It cannot prevent screenshots, cameras, copies or a modified recipient. |
| Chat burn | **Wired for indexed OSL state.** Exact active scope state, whitelists, wrapped local content and recorded remote blobs are removed with separate local/remote results. | Native Instagram/Discord/etc. history is untouched unless a future adapter separately reports deletion. |
| App burn | **Wired only for complete post-index accounts.** An immutable manifest is reviewed/frozen before each indexed scope is burned. | Legacy or incompletely enumerated accounts fail closed. Login profiles/cookies and native history remain untouched. |
| Account cleanup | **Wired as a separate confirmed flow.** Known Hub identities, state, profiles, registries and preferences are removed; Cloudflare unregister is separately reported. | It does not claim forensic erasure, instant Cloudflare backup expiry, or deletion from social-media providers. |
| Hidden read tracking | **Not implemented.** | Deliberately excluded. Opened receipts require recipient opt-in and authenticated consent. |
| Screenshot protection | **Capture resistance wired.** | It is not screenshot prevention and cannot stop cameras, malware or modified recipients. |
