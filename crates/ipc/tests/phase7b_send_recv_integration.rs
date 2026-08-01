//! Phase 7b end-to-end send/recv integration tests.
//!
//! Drives the Rust IPC layer's wire-v=2 send/recv functions
//! directly (no Tauri, no boot.js) to lock the behaviours from
//! Task 7's spec:
//!
//! 1. `cmd_osl_encrypt_message_v2` happy-path produces a wire
//!    string that decrypts back to the same plaintext.
//! 2. Empty whitelist → `"no_whitelisted_recipients"`.
//! 3. Burn marker round-trip mutates peer_map + wipes wrapped_keys.
//! 4. Invitation round-trip enqueues to pending_invitations and
//!    populates the sender's pubkey eagerly.
//! 5. Response round-trip updates
//!    `peer_map.outgoing_whitelist_responses`.
//! 6. `cmd_osl_unwhitelist_scope` with `revoke_broadened=true`
//!    burns the DM scope AND clears broadened flags so the
//!    cross-scope grant goes away.
//! 7. `cmd_osl_unwhitelist_scope` with `revoke_broadened=false`
//!    burns just the DM scope; broadened access in other
//!    scopes is retained.

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use crypto::x25519;
use ipc::commands::{
    cmd_osl_decrypt_message_v2, cmd_osl_encrypt_message_v2_wire, cmd_osl_send_burn_marker,
    cmd_osl_unwhitelist_scope, OSL_RESULT_BURN_APPLIED,
};
use ipc::peer_map::WhitelistEntry;
use ipc::scope::{Scope, ScopeInput};
use ipc::state::AppState;
use ipc::whitelist_state::ScopeState;
use ipc::wire_v2::MSG_TYPE_CONTENT;
use keystore::generate_identity;

// ---- fixtures ----

const LIAM_DID: &str = "900000000000000003";
const HENRY_DID: &str = "900000000000000001";
const GC_ID: &str = "1234567890";

fn fresh_state_for_liam() -> (AppState, keystore::Identity) {
    let state = AppState::new();
    let id = generate_identity("liam".to_string());
    let id_clone = keystore::generate_identity("liam".to_string());
    // (We can't Clone the real Identity — just generate one we
    // hand back, separate from the one stored.) For tests where
    // we need the loaded identity's pubkey, use the AppState
    // directly via state.identity.lock().
    *state.identity.lock().unwrap() = Some(id);
    (state, id_clone)
}

fn install_peer_pubkey(state: &AppState, discord_id: &str, pk: x25519::PublicKey) {
    let mut pm = state.peer_map.lock().unwrap();
    let pe = pm.entry(discord_id.to_string()).or_default();
    pe.pubkey = Some(STANDARD.encode(pk.as_bytes()));
    pe.discord_id = Some(discord_id.to_string());
}

/// Phase 9-A1: install peer pubkey + ML-KEM pubkey. Required for
/// any test that sends through cmd_osl_encrypt_message_v2 (which is
/// v=3 only post-9-A1). Tests that don't send (recv-only paths)
/// can keep using install_peer_pubkey with X25519 only.
fn install_peer_pubkey_v3(
    state: &AppState,
    discord_id: &str,
    pk: x25519::PublicKey,
    mlkem_pk: &crypto::ml_kem_768::EncapsulationKey,
) {
    let mut pm = state.peer_map.lock().unwrap();
    let pe = pm.entry(discord_id.to_string()).or_default();
    pe.pubkey = Some(STANDARD.encode(pk.as_bytes()));
    pe.ik_mlkem768_pub = Some(STANDARD.encode(mlkem_pk.to_bytes()));
    pe.discord_id = Some(discord_id.to_string());
}

fn install_dm_whitelist(state: &AppState, peer_discord_id: &str, broadened: bool) {
    let scope = Scope::dm(peer_discord_id);
    {
        let mut ws = state.whitelist_state.lock().unwrap();
        ws.insert(
            scope.storage_key(),
            ScopeState {
                encrypt_toggle: true,
                auto_enabled: true,
                ..ScopeState::default()
            },
        );
    }
    let mut pm = state.peer_map.lock().unwrap();
    let pe = pm.entry(peer_discord_id.to_string()).or_default();
    pe.outgoing_whitelists.push(WhitelistEntry::Dm {
        broadened,
        enabled_at: None,
    });
}

fn install_gc_full_whitelist(state: &AppState, gc_id: &str, members: &[&str]) {
    let scope = Scope::gc(gc_id);
    {
        let mut ws = state.whitelist_state.lock().unwrap();
        ws.insert(
            scope.storage_key(),
            ScopeState {
                encrypt_toggle: true,
                auto_enabled: true,
                ..ScopeState::default()
            },
        );
    }
    let mut pm = state.peer_map.lock().unwrap();
    for m in members {
        let pe = pm.entry((*m).to_string()).or_default();
        pe.outgoing_whitelists.push(WhitelistEntry::Gc {
            id: gc_id.to_string(),
            user_specific: false,
        });
    }
}

// Convert Scope → ScopeInput for the command surface.
fn si(s: &Scope) -> ScopeInput {
    ScopeInput::from(s)
}

// ---- 1. encrypt_message_v2 happy-path ----

#[test]
fn test_osl_encrypt_message_v2_command() {
    // Phase 9-A1: this exercise now goes through the v=3 path
    // (cmd_osl_encrypt_message_v2 calls encrypt_v3 internally).
    // Henry needs both X25519 + ML-KEM pubkeys in liam's peer_map,
    // and we decrypt with henry's full Identity to recover the
    // plaintext.
    let (state, _) = fresh_state_for_liam();
    let (henry_sk, henry_pk) = x25519::generate_keypair();
    let (henry_mlkem_sk, henry_mlkem_pk) = crypto::ml_kem_768::generate_keypair();
    install_peer_pubkey_v3(&state, HENRY_DID, henry_pk, &henry_mlkem_pk);
    install_dm_whitelist(&state, HENRY_DID, false);

    let scope = Scope::dm(HENRY_DID);
    let wire = cmd_osl_encrypt_message_v2_wire(
        &state,
        "hello phase 7b".to_string(),
        si(&scope),
        vec![HENRY_DID.to_string()],
        LIAM_DID.to_string(),
    )
    .expect("encrypt v=3")
    .content;
    assert!(
        wire.starts_with("DPC0::"),
        "v=3 still uses DPC0:: prefix; version byte is inside payload"
    );

    let recovered = ipc::wire_v2::decrypt_v3(&wire, &henry_sk, &henry_mlkem_sk).unwrap();
    assert_eq!(recovered.msg_type, MSG_TYPE_CONTENT);
    assert_eq!(recovered.plaintext, b"hello phase 7b".to_vec());
}

// ---- 2. encrypt_message_v2 with empty whitelist (encrypt-to-self) ----

// 7d-PIVOT: under the new semantics, encrypt_toggle is independent
// of whitelist. Encrypting with no peer whitelist is now valid —
// the message encrypts to self only (you alone will be able to
// decrypt). The previous `no_whitelisted_recipients` error has
// been removed.
#[test]
fn test_osl_encrypt_no_whitelist_encrypts_to_self() {
    let (state, _) = fresh_state_for_liam();
    // Henry is in the channel but not whitelisted.
    let (_henry_sk, henry_pk) = x25519::generate_keypair();
    install_peer_pubkey(&state, HENRY_DID, henry_pk);

    let scope = Scope::dm(HENRY_DID);
    let wire = cmd_osl_encrypt_message_v2_wire(
        &state,
        "encrypted-to-self only".to_string(),
        si(&scope),
        vec![HENRY_DID.to_string()],
        LIAM_DID.to_string(),
    )
    .expect("encrypt-to-self must succeed under 7d-PIVOT")
    .content;
    assert!(
        wire.starts_with("DPC0::"),
        "expected DPC0 wire prefix, got: {wire}"
    );
}

// ---- 3. burn marker applies ----

#[test]
fn test_recv_burn_marker_applies_burn() {
    // Liam (us) decrypts a burn marker sent by henry. The marker
    // names the DM scope. Expected effects: peer_map[henry] gains
    // a BurnedScope::Dm entry; wrapped_keys with matching scope
    // are wiped (no rows here, so wipe is a no-op but the call
    // should succeed).
    let (state, _) = fresh_state_for_liam();
    let (henry_sk, henry_pk) = x25519::generate_keypair();
    install_peer_pubkey(&state, HENRY_DID, henry_pk);

    // Build the burn marker as if henry sent it.
    let scope = Scope::dm(LIAM_DID); // henry burns *his* DM with liam;
                                     // scope.id = synthetic sender id from henry's PoV.
    let marker = ipc::control_messages::BurnMarker {
        scope: scope.clone(),
        burned_at: 1_700_000_000,
    };
    let body = ipc::control_messages::serialize_burn_marker(&marker).unwrap();
    let wire = ipc::wire_v2::encrypt_v2(
        &body,
        &[state
            .identity
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .x25519_public],
        ipc::wire_v2::MSG_TYPE_BURN,
        &henry_sk,
    )
    .unwrap();

    let result = cmd_osl_decrypt_message_v2(
        &state,
        None,
        "channel".to_string(),
        HENRY_DID.to_string(),
        wire,
        None,
        None,
    )
    .expect("decrypt burn marker");
    assert_eq!(result, OSL_RESULT_BURN_APPLIED);

    let pm = state.peer_map.lock().unwrap();
    let henry_entry = pm.get(HENRY_DID).expect("henry in peer_map");
    assert!(
        henry_entry
            .burned_scopes
            .iter()
            .any(|b| matches!(b, ipc::peer_map::BurnedScope::Dm { .. })),
        "expected DM burned_scope on henry, got {:?}",
        henry_entry.burned_scopes
    );
}

// 9-C1: test_recv_invitation_adds_to_pending +
// test_recv_response_updates_outgoing_whitelist removed alongside
// the invitation handshake. Permissive-decrypt coverage lives in
// the C1 phase tests.

// ---- 6. unwhitelist DM with revoke_broadened=true ----

#[test]
fn test_unwhitelist_dm_with_broaden_revoke() {
    // Setup: DM-whitelist henry with broadened=true. He has
    // broadened access to a shared GC. No EXPLICIT GC whitelist
    // — henry's GC access lives solely via the DM broaden, so
    // un-whitelisting the DM + revoking broadened access takes
    // his GC access away.
    let (state, _) = fresh_state_for_liam();
    let (_henry_sk, henry_pk) = x25519::generate_keypair();
    install_peer_pubkey(&state, HENRY_DID, henry_pk);
    install_dm_whitelist(&state, HENRY_DID, /*broadened*/ true);

    // Sanity: before un-whitelist, can_encrypt_to GC says yes.
    {
        let pm = state.peer_map.lock().unwrap();
        assert!(ipc::whitelist::can_encrypt_to(
            &pm,
            &Scope::gc(GC_ID),
            HENRY_DID,
        ));
    }

    let _wire = cmd_osl_unwhitelist_scope(
        &state,
        HENRY_DID.to_string(),
        si(&Scope::dm(HENRY_DID)),
        vec![HENRY_DID.to_string()],
        LIAM_DID.to_string(),
        /*revoke_broadened*/ true,
    )
    .expect("unwhitelist DM");

    // DM scope is burned.
    {
        let pm = state.peer_map.lock().unwrap();
        let henry = pm.get(HENRY_DID).unwrap();
        assert!(
            henry
                .burned_scopes
                .iter()
                .any(|b| matches!(b, ipc::peer_map::BurnedScope::Dm { .. })),
            "DM scope must be burned"
        );
        // No remaining DM whitelist entry with broadened=true.
        let any_broadened = henry.outgoing_whitelists.iter().any(|w| {
            matches!(
                w,
                ipc::peer_map::WhitelistEntry::Dm {
                    broadened: true,
                    ..
                }
            )
        });
        assert!(
            !any_broadened,
            "broadened flag must be cleared after revoke"
        );
    }

    // Burned scope filter excludes henry from GC encrypt_to.
    // (DM scope is in burned_scopes; even though GC isn't, the
    // peer-burn check for the GC scope walks burned_scopes for
    // any matching kind+id, so DM burn doesn't affect GC. The
    // broadened revoke is what kills the GC grant.)
    {
        let pm = state.peer_map.lock().unwrap();
        assert!(
            !ipc::whitelist::can_encrypt_to(&pm, &Scope::gc(GC_ID), HENRY_DID),
            "post-revoke: henry must lose GC access"
        );
    }
}

// ---- 7. unwhitelist DM without revoke_broadened ----

#[test]
fn test_unwhitelist_dm_without_broaden_revoke() {
    let (state, _) = fresh_state_for_liam();
    let (_henry_sk, henry_pk) = x25519::generate_keypair();
    install_peer_pubkey(&state, HENRY_DID, henry_pk);
    install_dm_whitelist(&state, HENRY_DID, /*broadened*/ true);
    install_gc_full_whitelist(&state, GC_ID, &[HENRY_DID]);

    let _wire = cmd_osl_unwhitelist_scope(
        &state,
        HENRY_DID.to_string(),
        si(&Scope::dm(HENRY_DID)),
        vec![HENRY_DID.to_string()],
        LIAM_DID.to_string(),
        /*revoke_broadened*/ false,
    )
    .expect("unwhitelist DM without revoke");

    // DM scope is burned. The unwhitelist removes the DM
    // outgoing-whitelist entry, but does NOT touch the
    // broadened flag (it's removed alongside the entry — only
    // a re-added DM whitelist would carry the flag again). The
    // distinguishing test for revoke vs no-revoke is whether
    // the broadened *access* path still works in shared scopes.
    //
    // For the no-revoke path: we DROP the DM whitelist entry
    // entirely (so the broadened flag goes away). The GC scope
    // is unaffected by the DM burn — henry still appears in
    // ScopeState.members for the GC, so can_encrypt_to(GC) is
    // satisfied via the *explicit* GC grant. This matches the
    // §3.4 semantics: "Any independent whitelists in shared
    // scopes remain."
    {
        let pm = state.peer_map.lock().unwrap();
        assert!(
            ipc::whitelist::can_encrypt_to(&pm, &Scope::gc(GC_ID), HENRY_DID),
            "post-no-revoke: henry retains GC access via explicit GC whitelist"
        );
    }
}

// 9-C1: test_set_whitelist_then_accept_unlocks_decrypt removed
// alongside the invitation handshake. Permissive decrypt means
// there is no "unlock" step — if we have keys, we decrypt.
// Coverage moved to `phase_c1_handshake_removal.rs`.

#[allow(dead_code)]
fn _retain_unused_import_warnings_suppressor() {
    // Keep cmd_osl_send_burn_marker referenced so the import
    // doesn't trigger an unused-import warning until S2 lands tests
    // that actually exercise it directly here.
    let _ = cmd_osl_send_burn_marker;
}
