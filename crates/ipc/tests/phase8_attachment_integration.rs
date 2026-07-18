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
    cmd_osl_open_attachment, cmd_osl_seal_attachment, decoy_png, OSL_ATT_MAGIC, OSL_ATT_MAGIC_V2,
    OSL_ATT_MAGIC_V3,
};
use ipc::commands::{
    cmd_osl_decrypt_message_v2, cmd_osl_encrypt_attachment_envelope, cmd_osl_open_attachment_v2,
    cmd_osl_seal_attachment_with_cover_v2, cmd_osl_seal_attachment_with_cover_v3,
    AttachmentEnvelopeInput, OSL_RESULT_ATTACHMENT_PREFIX,
};
use ipc::peer_map::WhitelistEntry;
use ipc::scope::{Scope, ScopeInput};
use ipc::state::AppState;
use ipc::whitelist_state::ScopeState;
use keystore::generate_identity;

const LIAM_DID: &str = "900000000000000003";
const HENRY_DID: &str = "900000000000000001";

fn fresh_state_for_liam() -> AppState {
    let state = AppState::new();
    *state.identity.lock().unwrap() = Some(generate_identity("liam".to_string()));
    // F3.6: attachment-seal tests need a Paid license_state so the
    // F3.6 attachment-send tier gate at
    // `cmd_osl_seal_attachment_with_cover_v3` doesn't fire. These
    // tests predate F3.6 and exercise wire-format / round-trip
    // properties, not tier gating.
    *state.license_state.lock().unwrap() = keystore::LicenseStateDto {
        state: keystore::LicenseState::Paid,
        raw_status: "ACTIVE".to_string(),
        current_period_end: None,
        last_validated_at: None,
    };
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
                ..ScopeState::default()
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
    // 8e: upload filename is `.mp4` (was `.bin` post-8d-FIX2, `.png`
    // pre-FIX2). MP4 is non-image so Discord doesn't transcode, and
    // the file renders as a video-card preview surface instead of a
    // generic download card. The V1 seal path tested here still
    // produces a decoy-PNG wire; the V3 path is tested separately.
    assert!(
        sealed.random_filename.ends_with(".mp4"),
        "upload name must be .mp4 (8e)"
    );
    assert_eq!(sealed.random_filename.len(), "abcd1234.mp4".len());
    let key_b64 = sealed.att_key_b64.clone();
    let file_bytes = STANDARD.decode(&sealed.file_blob_b64).unwrap();

    // V1 seal still emits the decoy PNG prefix (legacy code path
    // kept for backward compat with files in flight). The OSL-ATT1
    // magic must follow the PNG bytes.
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
        vec![AttachmentEnvelopeInput {
            att_key_b64: att_key_b64.clone(),
            original_filename: "secret-photo.jpg".to_string(),
            random_filename: "abcd1234.png".to_string(),
            mime_type: "image/jpeg".to_string(),
        }],
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
    let attachments = v["attachments"].as_array().expect("attachments[] present");
    assert_eq!(attachments.len(), 1);
    let entry = &attachments[0];
    assert_eq!(entry["originalFilename"], "secret-photo.jpg");
    assert_eq!(entry["randomFilename"], "abcd1234.png");
    assert_eq!(entry["mimeType"], "image/jpeg");
    let recovered_key = STANDARD
        .decode(entry["attKey"].as_str().expect("attKey is string"))
        .expect("attKey is base64");
    assert_eq!(recovered_key, att_key_bytes.to_vec());
}

#[test]
fn envelope_round_trip_multi_attachment() {
    // Phase 8b: 3-attachment cover. Verifies the list ordering is
    // preserved and each entry's attKey survives the CBOR round trip.
    let liam_state = fresh_state_for_liam();
    let (henry_sk, henry_pk) = x25519::generate_keypair();
    install_peer_pubkey(&liam_state, HENRY_DID, henry_pk);
    install_dm_whitelist(&liam_state, HENRY_DID);

    let scope = Scope::dm(HENRY_DID);
    let inputs = vec![
        AttachmentEnvelopeInput {
            att_key_b64: STANDARD.encode([0x01u8; 32]),
            original_filename: "first.png".to_string(),
            random_filename: "aaaaaaaa.png".to_string(),
            mime_type: "image/png".to_string(),
        },
        AttachmentEnvelopeInput {
            att_key_b64: STANDARD.encode([0x02u8; 32]),
            original_filename: "second.jpg".to_string(),
            random_filename: "bbbbbbbb.png".to_string(),
            mime_type: "image/jpeg".to_string(),
        },
        AttachmentEnvelopeInput {
            att_key_b64: STANDARD.encode([0x03u8; 32]),
            original_filename: "third.mp4".to_string(),
            random_filename: "cccccccc.png".to_string(),
            mime_type: "video/mp4".to_string(),
        },
    ];
    let wire = cmd_osl_encrypt_attachment_envelope(
        &liam_state,
        si(&scope),
        vec![HENRY_DID.to_string()],
        LIAM_DID.to_string(),
        inputs,
    )
    .expect("encrypt multi envelope");

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

    let recovered = cmd_osl_decrypt_message_v2(
        &henry_state,
        None,
        "1234567890".to_string(),
        LIAM_DID.to_string(),
        wire,
        None,
        None,
    )
    .expect("decrypt multi envelope");
    let json_part = recovered.trim_start_matches(OSL_RESULT_ATTACHMENT_PREFIX);
    let v: serde_json::Value = serde_json::from_str(json_part).unwrap();
    let arr = v["attachments"].as_array().unwrap();
    assert_eq!(arr.len(), 3);
    assert_eq!(arr[0]["originalFilename"], "first.png");
    assert_eq!(arr[1]["originalFilename"], "second.jpg");
    assert_eq!(arr[2]["originalFilename"], "third.mp4");
    assert_eq!(arr[0]["randomFilename"], "aaaaaaaa.png");
    assert_eq!(arr[1]["randomFilename"], "bbbbbbbb.png");
    assert_eq!(arr[2]["randomFilename"], "cccccccc.png");
    assert_eq!(arr[2]["mimeType"], "video/mp4");
    // Per-entry att_key must round-trip distinctly.
    let k0 = STANDARD.decode(arr[0]["attKey"].as_str().unwrap()).unwrap();
    let k2 = STANDARD.decode(arr[2]["attKey"].as_str().unwrap()).unwrap();
    assert_eq!(k0, vec![0x01u8; 32]);
    assert_eq!(k2, vec![0x03u8; 32]);
}

#[test]
fn empty_envelope_input_rejected() {
    let state = fresh_state_for_liam();
    let scope = Scope::dm(HENRY_DID);
    install_peer_pubkey(&state, HENRY_DID, x25519::generate_keypair().1);
    install_dm_whitelist(&state, HENRY_DID);
    let err = cmd_osl_encrypt_attachment_envelope(
        &state,
        si(&scope),
        vec![HENRY_DID.to_string()],
        LIAM_DID.to_string(),
        vec![],
    )
    .unwrap_err();
    assert!(
        err.contains("no entries") || err.contains("empty"),
        "expected empty-list error, got: {err}"
    );
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

// ---- Phase 8d: V2 wire-format round-trip tests ----

fn fresh_henry_state_with_liam_pubkey(
    liam_state: &AppState,
    henry_sk: &x25519::SecretKey,
    henry_pk: x25519::PublicKey,
) -> AppState {
    let henry_state = AppState::new();
    {
        let mut id = keystore::generate_identity("henry".to_string());
        id.x25519_secret = henry_sk.clone();
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
    henry_state
}

/// Mark `sender_did` as accepted-in-scope from Henry's perspective —
/// the equivalent of Henry having clicked "accept invitation" for
/// the named scope. Lets the cover decrypt's `should_decrypt_from`
/// gate pass.
fn mark_sender_accepted_in_scope(state: &AppState, sender_did: &str, scope: &Scope) {
    // 9-C1: handshake gate removed; this helper is a no-op kept
    // for call-site stability. Permissive decrypt means no sender-accept
    // state needs to exist.
    let _ = (state, sender_did, scope);
}

#[test]
fn v2_seal_carries_v2_magic_and_round_trips() {
    let liam_state = fresh_state_for_liam();
    let (henry_sk, henry_pk) = x25519::generate_keypair();
    install_peer_pubkey(&liam_state, HENRY_DID, henry_pk);
    install_dm_whitelist(&liam_state, HENRY_DID);

    let scope = Scope::dm(HENRY_DID);
    let original = vec![0xAAu8; 8 * 1024];
    let sealed = cmd_osl_seal_attachment_with_cover_v2(
        &liam_state,
        si(&scope),
        vec![HENRY_DID.to_string()],
        LIAM_DID.to_string(),
        STANDARD.encode(&original),
        "selfie.jpg".to_string(),
        "deadbeef.bin".to_string(),
    )
    .expect("seal v2");
    assert_eq!(sealed.mime_type, "image/jpeg");
    assert_eq!(sealed.random_filename, "deadbeef.bin");
    let bytes = STANDARD.decode(&sealed.sealed_b64).unwrap();
    // 8d-FIX2: V2 wire starts directly with the magic — no decoy
    // PNG prefix. Discord transcodes image-MIME attachments at the
    // CDN, so we ship as `application/octet-stream` (.bin) and skip
    // the PNG wrapper entirely.
    assert_eq!(
        &bytes[..OSL_ATT_MAGIC_V2.len()],
        OSL_ATT_MAGIC_V2,
        "V2 magic must be at offset 0 (no decoy PNG prefix in 8d-FIX2)"
    );
    assert!(
        !bytes
            .windows(OSL_ATT_MAGIC.len())
            .any(|w| w == OSL_ATT_MAGIC),
        "V1 magic should not appear in a V2-only bundle"
    );

    // Henry, as a whitelisted recipient who has accepted Liam in
    // this scope, can fully open the V2 bundle without any
    // out-of-band cover.
    let henry_state = fresh_henry_state_with_liam_pubkey(&liam_state, &henry_sk, henry_pk);
    mark_sender_accepted_in_scope(&henry_state, LIAM_DID, &scope);
    let opened = cmd_osl_open_attachment_v2(
        &henry_state,
        LIAM_DID.to_string(),
        Some(si(&scope)),
        sealed.sealed_b64.clone(),
        None,
        None,
    )
    .expect("open v2");
    assert_eq!(opened.original_filename, "selfie.jpg");
    assert_eq!(opened.mime_type, "image/jpeg");
    let recovered = STANDARD.decode(&opened.plaintext_b64).unwrap();
    assert_eq!(recovered, original);
}

#[test]
fn v3_seal_carries_v3_magic_in_mp4_free_box_and_round_trips() {
    // Phase 8e: V3 seal wraps the V2 cover+filename+payload triple
    // in an MP4 `free` box appended to a decoy MP4. Open chain
    // detects V3 first, then V2/V1. Whitelisted recipient unwraps
    // without any out-of-band cover key.
    let liam_state = fresh_state_for_liam();
    let (henry_sk, henry_pk) = x25519::generate_keypair();
    install_peer_pubkey(&liam_state, HENRY_DID, henry_pk);
    install_dm_whitelist(&liam_state, HENRY_DID);

    let scope = Scope::dm(HENRY_DID);
    let original = vec![0xCDu8; 16 * 1024];
    let sealed = cmd_osl_seal_attachment_with_cover_v3(
        &liam_state,
        si(&scope),
        vec![HENRY_DID.to_string()],
        LIAM_DID.to_string(),
        STANDARD.encode(&original),
        "vacation.mp4".to_string(),
        "cafef00d.mp4".to_string(),
    )
    .expect("seal v3");
    assert_eq!(sealed.mime_type, "video/mp4");
    assert_eq!(sealed.random_filename, "cafef00d.mp4");
    let bytes = STANDARD.decode(&sealed.sealed_b64).unwrap();
    // ftyp box at offset 0 — MP4 container start.
    assert_eq!(
        &bytes[4..8],
        b"ftyp",
        "V3 wire must start with MP4 ftyp box"
    );
    // V3 magic is somewhere past the decoy bytes (inside a free box).
    assert!(
        bytes
            .windows(OSL_ATT_MAGIC_V3.len())
            .any(|w| w == OSL_ATT_MAGIC_V3),
        "V3 magic must be present in the sealed bundle"
    );
    // V2/V1 magics must NOT appear in a V3 bundle.
    assert!(
        !bytes
            .windows(OSL_ATT_MAGIC_V2.len())
            .any(|w| w == OSL_ATT_MAGIC_V2),
        "V2 magic should not appear in a V3-only bundle"
    );
    assert!(
        !bytes
            .windows(OSL_ATT_MAGIC.len())
            .any(|w| w == OSL_ATT_MAGIC),
        "V1 magic should not appear in a V3-only bundle"
    );

    // Henry, as a whitelisted recipient who has accepted Liam in
    // this scope, opens the V3 bundle through the same V2-named
    // command (the open chain auto-detects V3 → V2 → V1).
    let henry_state = fresh_henry_state_with_liam_pubkey(&liam_state, &henry_sk, henry_pk);
    mark_sender_accepted_in_scope(&henry_state, LIAM_DID, &scope);
    let opened = cmd_osl_open_attachment_v2(
        &henry_state,
        LIAM_DID.to_string(),
        Some(si(&scope)),
        sealed.sealed_b64.clone(),
        None,
        None,
    )
    .expect("open v3 via cmd_osl_open_attachment_v2");
    assert_eq!(opened.original_filename, "vacation.mp4");
    assert_eq!(opened.mime_type, "video/mp4");
    let recovered = STANDARD.decode(&opened.plaintext_b64).unwrap();
    assert_eq!(recovered, original);
}

#[test]
fn v3_open_chain_still_decodes_v2_and_v1() {
    // Decode chain must remain backward-compatible: V2 wires
    // (8d-FIX2) and V1 wires (8/8c) need to keep decrypting after
    // the V3 chain is added to cmd_osl_open_attachment_v2.
    let liam_state = fresh_state_for_liam();
    let (henry_sk, henry_pk) = x25519::generate_keypair();
    install_peer_pubkey(&liam_state, HENRY_DID, henry_pk);
    install_dm_whitelist(&liam_state, HENRY_DID);

    let scope = Scope::dm(HENRY_DID);
    let original = vec![0x77u8; 4096];

    // V2 round-trip via V3 open chain.
    let v2_sealed = cmd_osl_seal_attachment_with_cover_v2(
        &liam_state,
        si(&scope),
        vec![HENRY_DID.to_string()],
        LIAM_DID.to_string(),
        STANDARD.encode(&original),
        "legacy.png".to_string(),
        "0badcafe.bin".to_string(),
    )
    .expect("seal v2 for backward-compat test");
    let henry_state = fresh_henry_state_with_liam_pubkey(&liam_state, &henry_sk, henry_pk);
    mark_sender_accepted_in_scope(&henry_state, LIAM_DID, &scope);
    let opened_v2 = cmd_osl_open_attachment_v2(
        &henry_state,
        LIAM_DID.to_string(),
        Some(si(&scope)),
        v2_sealed.sealed_b64,
        None,
        None,
    )
    .expect("open v2 wire through v3 chain");
    assert_eq!(opened_v2.original_filename, "legacy.png");
    assert_eq!(STANDARD.decode(&opened_v2.plaintext_b64).unwrap(), original);

    // V1 round-trip via V3 open chain (legacy att_key path).
    let v1_sealed = cmd_osl_seal_attachment(&liam_state, original.clone(), "thumb.png".to_string())
        .expect("seal v1");
    let opened_v1 = cmd_osl_open_attachment_v2(
        &henry_state,
        LIAM_DID.to_string(),
        None,
        v1_sealed.file_blob_b64,
        Some(v1_sealed.att_key_b64.clone()),
        None,
    )
    .expect("open v1 wire through v3 chain with legacy key");
    assert_eq!(opened_v1.original_filename, "thumb.png");
    assert_eq!(STANDARD.decode(&opened_v1.plaintext_b64).unwrap(), original);
}

#[test]
fn v2_open_without_legacy_key_fails_on_v1_bundle() {
    // Seal via the V1 path and confirm open_v2 rejects without
    // a legacy key.
    let liam_state = fresh_state_for_liam();
    let original = vec![1u8; 1024];
    let sealed = cmd_osl_seal_attachment(&liam_state, original.clone(), "thumb.png".to_string())
        .expect("seal v1");
    let v1_bytes_b64 = sealed.file_blob_b64.clone();

    let (henry_sk, henry_pk) = x25519::generate_keypair();
    let henry_state = fresh_henry_state_with_liam_pubkey(&liam_state, &henry_sk, henry_pk);
    let err = cmd_osl_open_attachment_v2(
        &henry_state,
        LIAM_DID.to_string(),
        None,
        v1_bytes_b64.clone(),
        None,
        None,
    )
    .unwrap_err();
    assert!(
        err.contains("V1 file with no legacy att_key supplied"),
        "expected legacy-key-required error, got: {err}"
    );

    // With the legacy key, V1 fallback decrypts.
    let opened = cmd_osl_open_attachment_v2(
        &henry_state,
        LIAM_DID.to_string(),
        None,
        v1_bytes_b64,
        Some(sealed.att_key_b64.clone()),
        None,
    )
    .expect("open v1 via v2 with legacy key");
    let recovered = STANDARD.decode(&opened.plaintext_b64).unwrap();
    assert_eq!(recovered, original);
}

#[test]
fn v2_open_with_wrong_recipient_fails() {
    let liam_state = fresh_state_for_liam();
    let (_henry_sk, henry_pk) = x25519::generate_keypair();
    install_peer_pubkey(&liam_state, HENRY_DID, henry_pk);
    install_dm_whitelist(&liam_state, HENRY_DID);

    let scope = Scope::dm(HENRY_DID);
    let original = vec![7u8; 512];
    let sealed = cmd_osl_seal_attachment_with_cover_v2(
        &liam_state,
        si(&scope),
        vec![HENRY_DID.to_string()],
        LIAM_DID.to_string(),
        STANDARD.encode(&original),
        "doc.png".to_string(),
        "feedbabe.bin".to_string(),
    )
    .expect("seal v2");

    // A wholly unrelated identity (no whitelist relationship) opens
    // the same bundle. The v=2 cover header has no entry for their
    // pubkey hash, so the cover decrypt fails at the wire layer.
    let stranger_state = AppState::new();
    *stranger_state.identity.lock().unwrap() =
        Some(keystore::generate_identity("stranger".to_string()));
    let liam_pub = liam_state
        .identity
        .lock()
        .unwrap()
        .as_ref()
        .unwrap()
        .x25519_public;
    install_peer_pubkey(&stranger_state, LIAM_DID, liam_pub);

    let err = cmd_osl_open_attachment_v2(
        &stranger_state,
        LIAM_DID.to_string(),
        None,
        sealed.sealed_b64,
        None,
        None,
    )
    .unwrap_err();
    assert!(
        !err.is_empty(),
        "expected wire/v=2 decrypt error for non-recipient, got empty"
    );
}
