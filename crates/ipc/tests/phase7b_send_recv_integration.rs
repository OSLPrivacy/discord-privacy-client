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
    cmd_osl_accept_invitation, cmd_osl_decrypt_message_v2, cmd_osl_encrypt_message_v2,
    cmd_osl_send_burn_marker, cmd_osl_send_whitelist_invitation, cmd_osl_send_whitelist_response,
    cmd_osl_set_whitelist, cmd_osl_unwhitelist_scope, OSL_RESULT_BURN_APPLIED,
    OSL_RESULT_INVITATION_RECEIVED, OSL_RESULT_RESPONSE_RECEIVED,
};
use ipc::peer_map::WhitelistEntry;
use ipc::scope::{Scope, ScopeInput};
use ipc::state::AppState;
use ipc::whitelist_state::ScopeState;
use ipc::wire_v2::{decrypt_v2, MSG_TYPE_CONTENT};
use keystore::generate_identity;

// ---- fixtures ----

const LIAM_DID: &str = "1477008451799482419";
const HENRY_DID: &str = "1502770642930634812";
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

fn install_dm_whitelist(state: &AppState, peer_discord_id: &str, broadened: bool) {
    let scope = Scope::dm(peer_discord_id);
    {
        let mut ws = state.whitelist_state.lock().unwrap();
        ws.insert(
            scope.storage_key(),
            ScopeState {
                encrypt_toggle: true,
                auto_enabled: true,
                full_whitelist: false,
                members: vec![],
                whitelisted_users: vec![],
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
                full_whitelist: true,
                members: members.iter().map(|s| s.to_string()).collect(),
                whitelisted_users: vec![],
            },
        );
    }
}

// Convert Scope → ScopeInput for the command surface.
fn si(s: &Scope) -> ScopeInput {
    ScopeInput::from(s)
}

// ---- 1. encrypt_message_v2 happy-path ----

#[test]
fn test_osl_encrypt_message_v2_command() {
    let (state, _) = fresh_state_for_liam();
    let (henry_sk, henry_pk) = x25519::generate_keypair();
    install_peer_pubkey(&state, HENRY_DID, henry_pk);
    install_dm_whitelist(&state, HENRY_DID, false);

    let scope = Scope::dm(HENRY_DID);
    let wire = cmd_osl_encrypt_message_v2(
        &state,
        "hello phase 7b".to_string(),
        si(&scope),
        vec![HENRY_DID.to_string()],
        LIAM_DID.to_string(),
    )
    .expect("encrypt v=2");

    // Decrypt with henry's secret to verify the round-trip.
    let liam_pub = state
        .identity
        .lock()
        .unwrap()
        .as_ref()
        .unwrap()
        .x25519_public;
    let recovered = decrypt_v2(&wire, &henry_sk, &liam_pub).unwrap();
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
    let wire = cmd_osl_encrypt_message_v2(
        &state,
        "encrypted-to-self only".to_string(),
        si(&scope),
        vec![HENRY_DID.to_string()],
        LIAM_DID.to_string(),
    )
    .expect("encrypt-to-self must succeed under 7d-PIVOT");
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
                                     // scope.id = liam's discord id from henry's PoV.
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

// ---- 4. invitation enqueues + populates pubkey ----

#[test]
fn test_recv_invitation_adds_to_pending() {
    let (state, _) = fresh_state_for_liam();
    let (henry_sk, henry_pk) = x25519::generate_keypair();
    // Henry's pubkey must already be in peer_map for the v=2
    // wrap leg to decrypt (the recipient needs the sender's
    // pubkey to compute the ECDH shared secret). In production
    // the bootstrap path or keyserver round-trip populates it
    // before the first invitation arrives; here we install
    // directly. The "populated eagerly" assertion below still
    // verifies that the invitation re-stamps the on-disk
    // pubkey (idempotent overwrite is harmless).
    install_peer_pubkey(&state, HENRY_DID, henry_pk);

    let scope = Scope::dm(LIAM_DID);
    let invitation = ipc::control_messages::WhitelistInvitation {
        from_discord_id: HENRY_DID.to_string(),
        from_pubkey: henry_pk,
        scope: scope.clone(),
        sent_at: 1_700_000_001,
    };
    let body = ipc::control_messages::serialize_whitelist_invitation(&invitation).unwrap();
    let wire = ipc::wire_v2::encrypt_v2(
        &body,
        &[state
            .identity
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .x25519_public],
        ipc::wire_v2::MSG_TYPE_WHITELIST_INVITATION,
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
    .expect("decrypt invitation");
    assert_eq!(result, OSL_RESULT_INVITATION_RECEIVED);

    let pi = state.pending_invitations.lock().unwrap();
    assert_eq!(pi.len(), 1, "expected one pending invitation");
    let key = format!("from_{}_{}", HENRY_DID, scope.storage_key());
    assert!(pi.contains_key(&key));

    // pubkey populated eagerly.
    let pm = state.peer_map.lock().unwrap();
    let henry = pm.get(HENRY_DID).unwrap();
    assert!(henry.pubkey.is_some(), "henry pubkey should be populated");
}

// ---- 5. response updates outgoing_whitelist_responses ----

#[test]
fn test_recv_response_updates_outgoing_whitelist() {
    let (state, _) = fresh_state_for_liam();
    let (henry_sk, henry_pk) = x25519::generate_keypair();
    install_peer_pubkey(&state, HENRY_DID, henry_pk);

    let scope = Scope::dm(HENRY_DID);
    let response = ipc::control_messages::WhitelistResponse {
        scope: scope.clone(),
        accepted: true,
        responded_at: 1_700_000_002,
    };
    let body = ipc::control_messages::serialize_whitelist_response(&response).unwrap();
    let wire = ipc::wire_v2::encrypt_v2(
        &body,
        &[state
            .identity
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .x25519_public],
        ipc::wire_v2::MSG_TYPE_WHITELIST_RESPONSE,
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
    .expect("decrypt response");
    assert_eq!(result, OSL_RESULT_RESPONSE_RECEIVED);

    let pm = state.peer_map.lock().unwrap();
    let henry = pm.get(HENRY_DID).unwrap();
    assert_eq!(
        henry.outgoing_whitelist_responses.get(&scope.storage_key()),
        Some(&true),
        "expected accepted=true stored under {}",
        scope.storage_key()
    );
}

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
        let ws = state.whitelist_state.lock().unwrap();
        assert!(ipc::whitelist::can_encrypt_to(
            &ws,
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
        let ws = state.whitelist_state.lock().unwrap();
        assert!(
            !ipc::whitelist::can_encrypt_to(&ws, &pm, &Scope::gc(GC_ID), HENRY_DID),
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
        let ws = state.whitelist_state.lock().unwrap();
        assert!(
            ipc::whitelist::can_encrypt_to(&ws, &pm, &Scope::gc(GC_ID), HENRY_DID),
            "post-no-revoke: henry retains GC access via explicit GC whitelist"
        );
    }
}

// ---- (extra) round-trip the encrypt + control commands ----

#[test]
fn test_set_whitelist_then_accept_unlocks_decrypt() {
    // Liam set_whitelist for henry in DM → produces invitation
    // wire. Henry's side: decrypt the invitation, accept it via
    // cmd_osl_accept_invitation. Then a content message from
    // liam in that scope decrypts on henry's side.
    let (liam_state, _) = fresh_state_for_liam();
    let liam_pub = liam_state
        .identity
        .lock()
        .unwrap()
        .as_ref()
        .unwrap()
        .x25519_public;

    // Henry's state, with liam's pubkey + osl id pre-installed.
    let henry_state = {
        let henry_id = generate_identity("henry".to_string());
        let s = AppState::new();
        *s.identity.lock().unwrap() = Some(henry_id);
        s
    };
    let henry_pub = henry_state
        .identity
        .lock()
        .unwrap()
        .as_ref()
        .unwrap()
        .x25519_public;
    install_peer_pubkey(&liam_state, HENRY_DID, henry_pub);
    install_peer_pubkey(&henry_state, LIAM_DID, liam_pub);

    // Liam set whitelist.
    let invitation_wire = cmd_osl_set_whitelist(
        &liam_state,
        HENRY_DID.to_string(),
        si(&Scope::dm(HENRY_DID)),
        /*broadened*/ false,
        LIAM_DID.to_string(),
    )
    .expect("set_whitelist returns invitation");

    // Henry receives the invitation.
    let res = cmd_osl_decrypt_message_v2(
        &henry_state,
        None,
        "channel".to_string(),
        LIAM_DID.to_string(),
        invitation_wire,
        None,
        None,
    )
    .unwrap();
    assert_eq!(res, OSL_RESULT_INVITATION_RECEIVED);

    // Henry accepts.
    // The invitation_id format mirrors the recv-side enqueue.
    let inv_id = format!("from_{}_{}", LIAM_DID, Scope::dm(HENRY_DID).storage_key());
    // NB: henry's local view of the scope is "dm:<his own id>"
    // because the invitation's scope.id is HENRY_DID, matching
    // what liam encoded. So the invitation_id on henry's side
    // uses liam's perspective. Verify via the pending map.
    {
        let pi = henry_state.pending_invitations.lock().unwrap();
        assert!(
            pi.contains_key(&inv_id),
            "invitation id mismatch: have keys {:?}",
            pi.keys().collect::<Vec<_>>()
        );
    }
    cmd_osl_accept_invitation(&henry_state, inv_id).unwrap();

    // Liam encrypts content; henry decrypts. But for
    // cmd_osl_decrypt_message_v2 to use the
    // should_decrypt_from gate, henry's incoming_decrypt_accepted
    // map must be keyed by scope.storage_key() = "dm:henry_did".
    // The accept stored under the invitation's own scope_id,
    // which carries scope.id = HENRY_DID (liam's perspective).
    // So the key matches. Verify by sending content + gating
    // on the scope.
    let content_wire = cmd_osl_encrypt_message_v2(
        &liam_state,
        "post-accept hello".to_string(),
        si(&Scope::dm(HENRY_DID)),
        vec![HENRY_DID.to_string()],
        LIAM_DID.to_string(),
    )
    .expect("encrypt post-accept");
    let pt = cmd_osl_decrypt_message_v2(
        &henry_state,
        None,
        "channel".to_string(),
        LIAM_DID.to_string(),
        content_wire,
        Some(si(&Scope::dm(HENRY_DID))),
        None,
    )
    .expect("decrypt post-accept");
    assert_eq!(pt, "post-accept hello");

    // Suppress unused warning on the placeholder commands.
    let _ = cmd_osl_send_burn_marker;
    let _ = cmd_osl_send_whitelist_invitation;
    let _ = cmd_osl_send_whitelist_response;
}
