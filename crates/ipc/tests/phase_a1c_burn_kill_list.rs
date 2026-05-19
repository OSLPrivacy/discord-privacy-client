//! Phase 9-A1c burn kill list tests.
//!
//! Verifies the defense-in-depth burn kill list:
//!   - `cmd_osl_mark_scope_burned` records per-message IDs on the
//!     `BurnedScopeEntry`.
//!   - `cmd_osl_decrypt_message_v2` refuses to decrypt for any
//!     `discord_message_id` in that list, regardless of whether the
//!     scope-level skip cache is currently set.
//!   - `cmd_osl_open_attachment_v2` honours the same gate before any
//!     wire/v=2/v=3 work runs (so re-engaging encryption can't
//!     accidentally surface previously-burned attachments either).
//!   - Serde round-trip preserves the list, and a legacy on-disk
//!     record (no `burned_message_ids` field) loads cleanly with an
//!     empty list.

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use crypto::{ed25519, ml_kem_768, x25519};
use ipc::burned_scopes_file::{BurnedScopeEntry, BurnedScopesFile};
use ipc::commands::{
    cmd_osl_decrypt_message_v2, cmd_osl_encrypt_message_v2_wire, cmd_osl_mark_scope_burned,
    cmd_osl_open_attachment_v2, cmd_osl_seal_attachment_with_cover_v3,
};
use ipc::peer_map::WhitelistEntry;
use ipc::scope::{Scope, ScopeInput};
use ipc::state::AppState;
use ipc::whitelist_state::ScopeState;
use keystore::{generate_identity, Identity};

const LIAM_DID: &str = "1477008451799482419";
const HENRY_DID: &str = "1502770642930634812";
const MSG_ID_BURNED: &str = "1700000000000000001";
const MSG_ID_FRESH: &str = "1700000000000000002";

fn fresh_state(name: &str) -> AppState {
    let state = AppState::new();
    *state.identity.lock().unwrap() = Some(generate_identity(name.to_string()));
    // F3.6: tests that hit `cmd_osl_seal_attachment_with_cover_v3`
    // need a Paid license_state — the F3.6 attachment-send gate
    // would otherwise block the seal. These tests predate F3.6 and
    // exercise burn-kill-list semantics on attachments, not tier
    // gating.
    *state.license_state.lock().unwrap() = keystore::LicenseStateDto {
        state: keystore::LicenseState::Paid,
        raw_status: "ACTIVE".to_string(),
        current_period_end: None,
        last_validated_at: None,
    };
    state
}

/// Build an Identity using caller-supplied X25519 + ML-KEM keypairs
/// so we can stand up a "henry" state whose secret keys actually
/// match the pubkeys we installed in "liam"'s peer_map. Ed25519
/// isn't used by the v=3 decrypt path so we generate fresh ones.
fn identity_from_keypairs(
    user_id: &str,
    x_sk: &x25519::SecretKey,
    x_pk: &x25519::PublicKey,
    mlkem_sk: &ml_kem_768::DecapsulationKey,
    mlkem_pk: &ml_kem_768::EncapsulationKey,
) -> Identity {
    let (ed_sk, ed_pk) = ed25519::generate_keypair();
    let mlkem_sk_bytes = {
        let z = mlkem_sk.to_bytes();
        let mut out = [0u8; ml_kem_768::DECAPSULATION_KEY_SIZE];
        out.copy_from_slice(&*z);
        out
    };
    Identity::from_bytes(
        user_id.to_string(),
        *x_sk.as_bytes(),
        *x_pk.as_bytes(),
        *ed_sk.as_bytes(),
        *ed_pk.as_bytes(),
        mlkem_sk_bytes,
        mlkem_pk.to_bytes(),
    )
}

fn install_peer_pubkey_v3(
    state: &AppState,
    discord_id: &str,
    pk: x25519::PublicKey,
    mlkem_pk: &ml_kem_768::EncapsulationKey,
) {
    let mut pm = state.peer_map.lock().unwrap();
    let pe = pm.entry(discord_id.to_string()).or_default();
    pe.pubkey = Some(STANDARD.encode(pk.as_bytes()));
    pe.ik_mlkem768_pub = Some(STANDARD.encode(mlkem_pk.to_bytes()));
    pe.discord_id = Some(discord_id.to_string());
}

fn install_dm_whitelist(state: &AppState, peer_discord_id: &str) {
    let scope = Scope::dm(peer_discord_id);
    {
        let mut ws = state.whitelist_state.lock().unwrap();
        ws.insert(
            scope.storage_key(),
            ScopeState {
                encrypt_toggle: true,
                auto_enabled: true,
            },
        );
    }
    let mut pm = state.peer_map.lock().unwrap();
    let pe = pm.entry(peer_discord_id.to_string()).or_default();
    pe.outgoing_whitelists.push(WhitelistEntry::Dm {
        broadened: false,
        enabled_at: None,
    });
}

fn mark_sender_accepted_in_scope(state: &AppState, sender_did: &str, scope: &Scope) {
    // 9-C1: handshake gate removed; this helper is a no-op kept
    // for call-site stability. Permissive decrypt means no sender-accept
    // state needs to exist.
    let _ = (state, sender_did, scope);
}

fn si(s: &Scope) -> ScopeInput {
    ScopeInput::from(s)
}

#[test]
fn burn_records_message_ids_in_scope() {
    let state = fresh_state("liam");

    cmd_osl_mark_scope_burned(
        &state,
        "dm".to_string(),
        HENRY_DID.to_string(),
        None,
        Some(HENRY_DID.to_string()),
        vec![MSG_ID_BURNED.to_string(), MSG_ID_FRESH.to_string()],
    )
    .expect("mark burned");

    let g = state.burned_scopes.lock().unwrap();
    let entry = g
        .scopes
        .iter()
        .find(|e| e.scope_kind == "dm" && e.scope_id == HENRY_DID)
        .expect("burn entry present");
    assert_eq!(entry.burned_message_ids.len(), 2);
    assert!(entry.burned_message_ids.iter().any(|m| m == MSG_ID_BURNED));
    assert!(entry.burned_message_ids.iter().any(|m| m == MSG_ID_FRESH));
}

#[test]
fn repeat_burn_unions_message_ids() {
    // Burning the same scope twice should grow the kill list rather
    // than replace it — protects against a partial second burn
    // erasing IDs recorded on the first pass.
    let state = fresh_state("liam");
    cmd_osl_mark_scope_burned(
        &state,
        "dm".to_string(),
        HENRY_DID.to_string(),
        None,
        Some(HENRY_DID.to_string()),
        vec![MSG_ID_BURNED.to_string()],
    )
    .expect("first mark");
    cmd_osl_mark_scope_burned(
        &state,
        "dm".to_string(),
        HENRY_DID.to_string(),
        None,
        Some(HENRY_DID.to_string()),
        vec![MSG_ID_BURNED.to_string(), MSG_ID_FRESH.to_string()],
    )
    .expect("second mark");

    let g = state.burned_scopes.lock().unwrap();
    let entry = g
        .scopes
        .iter()
        .find(|e| e.scope_kind == "dm" && e.scope_id == HENRY_DID)
        .expect("burn entry present");
    assert_eq!(entry.burned_message_ids.len(), 2);
    assert!(entry.burned_message_ids.iter().any(|m| m == MSG_ID_BURNED));
    assert!(entry.burned_message_ids.iter().any(|m| m == MSG_ID_FRESH));
}

#[test]
fn decrypt_blocked_for_message_in_burn_kill_list() {
    // Liam sends a v=3 wire to Henry. Henry decrypts twice with the
    // same wire + same scope; the second time, the message_id is in
    // the kill list and the decrypt must be refused before any
    // crypto work runs.
    let liam_state = fresh_state("liam");
    let (henry_sk, henry_pk) = x25519::generate_keypair();
    let (henry_mlkem_sk, henry_mlkem_pk) = ml_kem_768::generate_keypair();
    install_peer_pubkey_v3(&liam_state, HENRY_DID, henry_pk, &henry_mlkem_pk);
    install_dm_whitelist(&liam_state, HENRY_DID);
    let liam_scope = Scope::dm(HENRY_DID);

    let wire = cmd_osl_encrypt_message_v2_wire(
        &liam_state,
        "hello, will be burned".to_string(),
        si(&liam_scope),
        vec![HENRY_DID.to_string()],
        LIAM_DID.to_string(),
    )
    .expect("encrypt")
    .content;

    let henry_state = AppState::new();
    *henry_state.identity.lock().unwrap() = Some(identity_from_keypairs(
        "henry",
        &henry_sk,
        &henry_pk,
        &henry_mlkem_sk,
        &henry_mlkem_pk,
    ));
    // From Henry's PoV the DM scope id is Liam's discord id.
    let henry_scope = Scope::dm(LIAM_DID);
    mark_sender_accepted_in_scope(&henry_state, LIAM_DID, &henry_scope);

    // Baseline: with no kill-list entry, decrypt succeeds.
    let plaintext = cmd_osl_decrypt_message_v2(
        &henry_state,
        Some(MSG_ID_BURNED.to_string()),
        LIAM_DID.to_string(),
        LIAM_DID.to_string(),
        wire.clone(),
        Some(si(&henry_scope)),
        None,
    )
    .expect("baseline decrypt succeeds before burn");
    assert_eq!(plaintext, "hello, will be burned");

    // Mark scope burned with MSG_ID_BURNED in the kill list and
    // re-decrypt the same wire. The kill list gate must fire.
    cmd_osl_mark_scope_burned(
        &henry_state,
        "dm".to_string(),
        LIAM_DID.to_string(),
        None,
        Some(LIAM_DID.to_string()),
        vec![MSG_ID_BURNED.to_string()],
    )
    .expect("mark burned");

    let err = cmd_osl_decrypt_message_v2(
        &henry_state,
        Some(MSG_ID_BURNED.to_string()),
        LIAM_DID.to_string(),
        LIAM_DID.to_string(),
        wire,
        Some(si(&henry_scope)),
        None,
    )
    .unwrap_err();
    assert!(
        err.contains("in_burn_kill_list"),
        "expected kill-list block error, got: {err}"
    );
}

#[test]
fn decrypt_allowed_for_message_not_in_kill_list() {
    // Same setup as the blocked test, but assert that another
    // message_id in the same burned scope still decrypts: the kill
    // list is per-message, not per-scope.
    let liam_state = fresh_state("liam");
    let (henry_sk, henry_pk) = x25519::generate_keypair();
    let (henry_mlkem_sk, henry_mlkem_pk) = ml_kem_768::generate_keypair();
    install_peer_pubkey_v3(&liam_state, HENRY_DID, henry_pk, &henry_mlkem_pk);
    install_dm_whitelist(&liam_state, HENRY_DID);
    let liam_scope = Scope::dm(HENRY_DID);

    let wire = cmd_osl_encrypt_message_v2_wire(
        &liam_state,
        "fresh, not in kill list".to_string(),
        si(&liam_scope),
        vec![HENRY_DID.to_string()],
        LIAM_DID.to_string(),
    )
    .expect("encrypt")
    .content;

    let henry_state = AppState::new();
    *henry_state.identity.lock().unwrap() = Some(identity_from_keypairs(
        "henry",
        &henry_sk,
        &henry_pk,
        &henry_mlkem_sk,
        &henry_mlkem_pk,
    ));
    let henry_scope = Scope::dm(LIAM_DID);
    mark_sender_accepted_in_scope(&henry_state, LIAM_DID, &henry_scope);

    cmd_osl_mark_scope_burned(
        &henry_state,
        "dm".to_string(),
        LIAM_DID.to_string(),
        None,
        Some(LIAM_DID.to_string()),
        vec![MSG_ID_BURNED.to_string()],
    )
    .expect("mark burned");

    let plaintext = cmd_osl_decrypt_message_v2(
        &henry_state,
        Some(MSG_ID_FRESH.to_string()),
        LIAM_DID.to_string(),
        LIAM_DID.to_string(),
        wire,
        Some(si(&henry_scope)),
        None,
    )
    .expect("fresh msg not in kill list — decrypt ok");
    assert_eq!(plaintext, "fresh, not in kill list");
}

#[test]
fn kill_list_persists_through_serde_roundtrip() {
    let mut file = BurnedScopesFile::default();
    file.version = 1;
    file.scopes.push(BurnedScopeEntry {
        scope_kind: "dm".to_string(),
        scope_id: HENRY_DID.to_string(),
        server_id: None,
        channel_id: Some(HENRY_DID.to_string()),
        burned_at: 1_700_000_000,
        burned_message_ids: vec![MSG_ID_BURNED.to_string(), MSG_ID_FRESH.to_string()],
    });
    let json = serde_json::to_string(&file).expect("serialize");
    let reloaded: BurnedScopesFile = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(reloaded, file);
}

#[test]
fn empty_kill_list_loads_from_legacy_record_without_field() {
    // A pre-9-A1c record on disk has no `burned_message_ids` field
    // at all. Loading it must succeed (serde default) and produce
    // an empty list rather than failing the load.
    let legacy_json = r#"{
        "version": 1,
        "scopes": [{
            "scope_kind": "dm",
            "scope_id": "1502770642930634812",
            "channel_id": "1502770642930634812",
            "burned_at": 1700000000
        }]
    }"#;
    let file: BurnedScopesFile = serde_json::from_str(legacy_json).expect("legacy load");
    assert_eq!(file.scopes.len(), 1);
    assert!(
        file.scopes[0].burned_message_ids.is_empty(),
        "missing field defaults to empty vec"
    );
}

#[test]
fn attachment_open_blocked_for_message_in_burn_kill_list() {
    // Liam seals an attachment for Henry. Henry has the DM scope
    // marked burned with this message_id in the kill list. The open
    // must be refused before any wire-decode work runs.
    let liam_state = fresh_state("liam");
    let (henry_sk, henry_pk) = x25519::generate_keypair();
    let (henry_mlkem_sk, henry_mlkem_pk) = ml_kem_768::generate_keypair();
    install_peer_pubkey_v3(&liam_state, HENRY_DID, henry_pk, &henry_mlkem_pk);
    install_dm_whitelist(&liam_state, HENRY_DID);

    let liam_scope = Scope::dm(HENRY_DID);
    let original = vec![0x42u8; 2048];
    let sealed = cmd_osl_seal_attachment_with_cover_v3(
        &liam_state,
        si(&liam_scope),
        vec![HENRY_DID.to_string()],
        LIAM_DID.to_string(),
        STANDARD.encode(&original),
        "secret.png".to_string(),
        "deadbeef.mp4".to_string(),
    )
    .expect("seal v3");

    let henry_state = AppState::new();
    *henry_state.identity.lock().unwrap() = Some(identity_from_keypairs(
        "henry",
        &henry_sk,
        &henry_pk,
        &henry_mlkem_sk,
        &henry_mlkem_pk,
    ));
    let henry_scope = Scope::dm(LIAM_DID);
    mark_sender_accepted_in_scope(&henry_state, LIAM_DID, &henry_scope);

    cmd_osl_mark_scope_burned(
        &henry_state,
        "dm".to_string(),
        LIAM_DID.to_string(),
        None,
        Some(LIAM_DID.to_string()),
        vec![MSG_ID_BURNED.to_string()],
    )
    .expect("mark burned");

    let err = cmd_osl_open_attachment_v2(
        &henry_state,
        LIAM_DID.to_string(),
        Some(si(&henry_scope)),
        sealed.sealed_b64,
        None,
        Some(MSG_ID_BURNED.to_string()),
    )
    .unwrap_err();
    assert!(
        err.contains("in_burn_kill_list"),
        "expected attachment kill-list block error, got: {err}"
    );
}
