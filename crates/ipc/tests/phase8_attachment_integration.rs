//! Phase 8 attachment integration tests.
//!
//! Round-trips the Tauri-facing seal/open + envelope commands end-to-end
//! without any Tauri or boot.js scaffolding: a fresh AppState per test,
//! a real generate_identity for Liam, Henry's pubkey installed in
//! peer_map, a DM whitelist installed so the recipient set isn't empty.
//!
//! What we lock here:
//!   1. seal → open round trip preserves bytes + filename + MIME.
//!   2. open fails on the wrong AEAD key (auth tag verification).
//!   3. open fails when the OSL-ATT1 magic is absent.
//!   4. encrypt_attachment_envelope ships a v=2 wire that decrypts
//!      back to the same envelope on the recipient side and produces
//!      the OSL_CONTROL_ATTACHMENT__ sentinel for boot.js.
//!   5. random_upload_filename produces 8-hex-char + ".png".
//!
//! Boot.js-level acceptance (DOM rewrite + Discord upload interception)
//! is exercised by hand against a running Tauri dev session — these
//! tests guarantee the Rust contract those JS paths depend on.

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use crypto::x25519;
use ipc::attachment_wire::{
    cmd_osl_open_attachment, cmd_osl_seal_attachment, decoy_png, OSL_ATT_MAGIC,
};
use ipc::commands::{
    cmd_osl_decrypt_message_v2, cmd_osl_encrypt_attachment_envelope, OSL_RESULT_ATTACHMENT_PREFIX,
};
use ipc::peer_map::WhitelistEntry;
use ipc::scope::{Scope, ScopeInput};
use ipc::state::AppState;
use ipc::whitelist_state::ScopeState;
use keystore::generate_identity;

const LIAM_DID: &str = "1477008451799482419";
const HENRY_DID: &str = "1502770642930634812";

fn fresh_state_for_liam() -> AppState {
    let state = AppState::new();
    *state.identity.lock().unwrap() = Some(generate_identity("liam".to_string()));
    state
}

fn install_peer_pubkey(state: &AppState, discord_id: &str, pk: x25519::PublicKey) {
    let mut pm = state.peer_map.lock().unwrap();
    let pe = pm.entry(discord_id.to_string()).or_default();
    pe.pubkey = Some(STANDARD.encode(pk.as_bytes()));
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
                full_whitelist: false,
                members: vec![],
                whitelisted_users: vec![],
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

fn si(s: &Scope) -> ScopeInput {
    ScopeInput::from(s)
}

#[test]
fn seal_open_round_trip_preserves_bytes_and_filename() {
    let state = fresh_state_for_liam();
    let plaintext = vec![0xABu8; 4096];
    let sealed = cmd_osl_seal_attachment(&state, plaintext.clone(), "vacation.jpg".to_string())
        .expect("seal");
    assert_eq!(sealed.mime_type, "image/jpeg");
    assert!(
        sealed.random_filename.ends_with(".png"),
        "decoy upload name must be .png so Discord renders the decoy preview"
    );
    assert_eq!(sealed.random_filename.len(), "abcd1234.png".len());
    let key_b64 = sealed.att_key_b64.clone();
    let file_bytes = STANDARD.decode(&sealed.file_blob_b64).unwrap();

    // OSL-ATT1 magic must be somewhere inside the uploaded file
    // (after the decoy PNG bytes).
    let magic_off = file_bytes
        .windows(OSL_ATT_MAGIC.len())
        .position(|w| w == OSL_ATT_MAGIC)
        .expect("magic present in sealed blob");
    assert!(
        magic_off >= decoy_png().len() / 2,
        "magic should come after the decoy PNG (decoy_len={}, magic_off={})",
        decoy_png().len(),
        magic_off
    );

    let opened = cmd_osl_open_attachment(&state, key_b64, file_bytes).expect("open");
    assert_eq!(opened.original_filename, "vacation.jpg");
    assert_eq!(opened.mime_type, "image/jpeg");
    let recovered = STANDARD.decode(&opened.plaintext_b64).unwrap();
    assert_eq!(recovered, plaintext);
}

#[test]
fn open_with_wrong_key_fails_auth() {
    let state = fresh_state_for_liam();
    let plaintext = vec![1u8; 1024];
    let sealed = cmd_osl_seal_attachment(&state, plaintext, "img.png".to_string()).expect("seal");
    let file_bytes = STANDARD.decode(&sealed.file_blob_b64).unwrap();
    // Replace the AEAD key with a different 32-byte value.
    let wrong_key_b64 = STANDARD.encode([0xFFu8; 32]);
    let err = cmd_osl_open_attachment(&state, wrong_key_b64, file_bytes).unwrap_err();
    assert!(
        err.contains("open_attachment") || err.contains("InnerCrypto"),
        "expected inner-crypto failure, got: {err}"
    );
}

#[test]
fn open_without_magic_fails_with_magic_not_found() {
    let state = fresh_state_for_liam();
    // A blob with no OSL framing at all.
    let bytes = vec![0u8; 10_000];
    let key_b64 = STANDARD.encode([0u8; 32]);
    let err = cmd_osl_open_attachment(&state, key_b64, bytes).unwrap_err();
    assert!(
        err.contains("MagicNotFound") || err.contains("magic"),
        "expected MagicNotFound, got: {err}"
    );
}

#[test]
fn open_with_short_key_rejected() {
    let state = fresh_state_for_liam();
    let bogus_key = STANDARD.encode([0u8; 16]); // wrong length
    let err = cmd_osl_open_attachment(&state, bogus_key, vec![0u8; 100]).unwrap_err();
    assert!(
        err.contains("att_key must be 32 bytes"),
        "expected length error, got: {err}"
    );
}

#[test]
fn envelope_round_trip_yields_sentinel_for_boot_js() {
    // Liam encrypts an envelope for Henry. Henry's recv side
    // decodes the v=2 wire and surfaces the OSL_CONTROL_ATTACHMENT
    // sentinel with the JSON payload.
    let liam_state = fresh_state_for_liam();
    let (henry_sk, henry_pk) = x25519::generate_keypair();
    install_peer_pubkey(&liam_state, HENRY_DID, henry_pk);
    install_dm_whitelist(&liam_state, HENRY_DID);

    let scope = Scope::dm(HENRY_DID);
    let att_key_bytes = [0xA5u8; 32];
    let att_key_b64 = STANDARD.encode(att_key_bytes);
    let wire = cmd_osl_encrypt_attachment_envelope(
        &liam_state,
        si(&scope),
        vec![HENRY_DID.to_string()],
        LIAM_DID.to_string(),
        att_key_b64.clone(),
        "secret-photo.jpg".to_string(),
        "abcd1234.png".to_string(),
        "image/jpeg".to_string(),
    )
    .expect("encrypt envelope");
    assert!(
        wire.starts_with("DPC0::"),
        "envelope wire must be a DPC0:: cover"
    );

    // Henry-side recv: a fresh state with Henry's identity loaded
    // and Liam's pubkey in his peer_map.
    let henry_state = AppState::new();
    {
        let mut id = keystore::generate_identity("henry".to_string());
        id.x25519_secret = henry_sk;
        id.x25519_public = henry_pk;
        *henry_state.identity.lock().unwrap() = Some(id);
    }
    let liam_pub = liam_state
        .identity
        .lock()
        .unwrap()
        .as_ref()
        .unwrap()
        .x25519_public;
    install_peer_pubkey(&henry_state, LIAM_DID, liam_pub);

    // The scope acceptance gate is exercised by 7b's content-message
    // tests. Here we pass scope_input=None so we exercise the
    // envelope's CBOR decode + sentinel emission path on its own —
    // the gate's coverage is shared with MSG_TYPE_CONTENT.
    let recovered = cmd_osl_decrypt_message_v2(
        &henry_state,
        None,
        "1234567890".to_string(),
        LIAM_DID.to_string(),
        wire,
        None,
        None,
    )
    .expect("decrypt v=2 envelope");

    assert!(
        recovered.starts_with(OSL_RESULT_ATTACHMENT_PREFIX),
        "recv must emit attachment sentinel, got: {recovered}"
    );
    let json_part = recovered.trim_start_matches(OSL_RESULT_ATTACHMENT_PREFIX);
    let v: serde_json::Value = serde_json::from_str(json_part).expect("sentinel payload is JSON");
    assert_eq!(v["originalFilename"], "secret-photo.jpg");
    assert_eq!(v["randomFilename"], "abcd1234.png");
    assert_eq!(v["mimeType"], "image/jpeg");
    let recovered_key = STANDARD
        .decode(v["attKey"].as_str().expect("attKey is string"))
        .expect("attKey is base64");
    assert_eq!(recovered_key, att_key_bytes.to_vec());
}

#[test]
fn unsupported_extension_rejected_at_seal_time() {
    let state = fresh_state_for_liam();
    let err =
        cmd_osl_seal_attachment(&state, vec![0u8; 100], "passwords.txt".to_string()).unwrap_err();
    assert!(
        err.contains("unsupported"),
        "expected unsupported-extension error, got: {err}"
    );
}
