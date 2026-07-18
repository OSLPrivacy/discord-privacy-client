//! Layer 10 / Phase 5: receive-side decoder tests.
//!
//! Exercises the production decoder
//! [`ipc::commands::decrypt_osl_phase4_cover`] across the
//! [`ipc::commands::DecodeError`] surface. Phase 4.5's
//! `osl_phase4_roundtrip.rs` covers the happy paths; this file
//! focuses on:
//!
//! - Each `DecodeError` variant being produced under the right
//!   trigger.
//! - The Phase 5 sender-self-decrypt UX flow (auto-include-sender).
//! - The `AppState::sender_pubkey_cache` semantics (5-min TTL,
//!   cache-hit behaviour).
//!
//! IPC-level tests for `cmd_osl_decrypt_message` (which involves a
//! live keyserver round-trip on cache miss) live in
//! `crates/keystore/tests/client_test.rs`-style mock-server
//! patterns; this file is purely the pure-decoder + pubkey-cache
//! layer.

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use ipc::commands::{
    decrypt_osl_phase4_cover, decrypt_osl_phase4_from_wire, encrypt_osl_phase4_to_pubkeys,
    DecodeError, OSL_PHASE4_WIRE_VERSION,
};
use ipc::state::SenderPubkeyCache;
use keystore::generate_identity;

// ---- DecodeError variant coverage ----

#[test]
fn bad_prefix_when_cover_lacks_dpc0() {
    let recipient = generate_identity("bob".to_string());
    let sender = generate_identity("alice".to_string());
    let err = decrypt_osl_phase4_cover(
        &recipient.x25519_secret,
        &sender.x25519_public,
        "this is not an OSL cover string",
    )
    .expect_err("non-DPC0 should be BadPrefix");
    assert!(matches!(err, DecodeError::BadPrefix), "got {err:?}");
}

#[test]
fn base64_error_on_malformed_body() {
    let recipient = generate_identity("bob".to_string());
    let sender = generate_identity("alice".to_string());
    let err = decrypt_osl_phase4_cover(
        &recipient.x25519_secret,
        &sender.x25519_public,
        "DPC0::!!!not-base64-at-all!!!",
    )
    .expect_err("garbage base64 should error");
    assert!(matches!(err, DecodeError::Base64(_)), "got {err:?}");
}

#[test]
fn too_short_when_wire_below_minimum() {
    let recipient = generate_identity("bob".to_string());
    let sender = generate_identity("alice".to_string());
    // 8 raw bytes < OSL_PHASE4_FIXED_FRAMING_BYTES (42).
    let cover = format!("DPC0::{}", STANDARD.encode([0u8; 8]));
    let err = decrypt_osl_phase4_cover(&recipient.x25519_secret, &sender.x25519_public, &cover)
        .expect_err("undersized wire should error");
    assert!(matches!(err, DecodeError::TooShort { .. }), "got {err:?}");
}

#[test]
fn unsupported_version_byte() {
    let recipient = generate_identity("bob".to_string());
    let sender = generate_identity("alice".to_string());
    // 42 bytes (minimum framing), version byte = 0x99 (unknown).
    let mut wire = vec![0u8; 42];
    wire[0] = 0x99;
    wire[1] = 0; // N=0; the version check fires first so this
                 // doesn't matter, but pick a value that wouldn't
                 // bypass the assertion.
    let err = decrypt_osl_phase4_from_wire(&recipient.x25519_secret, &sender.x25519_public, &wire)
        .expect_err("unknown version should error");
    match err {
        DecodeError::UnsupportedVersion { got, expected } => {
            assert_eq!(got, 0x99);
            assert_eq!(expected, OSL_PHASE4_WIRE_VERSION);
        }
        other => panic!("expected UnsupportedVersion, got {other:?}"),
    }
}

#[test]
fn zero_recipients_in_header() {
    let recipient = generate_identity("bob".to_string());
    let sender = generate_identity("alice".to_string());
    // Valid version, N=0.
    let mut wire = vec![0u8; 42];
    wire[0] = OSL_PHASE4_WIRE_VERSION;
    wire[1] = 0;
    let err = decrypt_osl_phase4_from_wire(&recipient.x25519_secret, &sender.x25519_public, &wire)
        .expect_err("N=0 should error");
    assert!(matches!(err, DecodeError::ZeroRecipients), "got {err:?}");
}

#[test]
fn no_matching_slot_when_not_recipient() {
    // Encrypt to recipient. Try to decrypt with stranger's
    // secret. Stranger's pub_hint is unlikely to collide with
    // any slot's hint (with 1-recipient + auto-sender = 2 slots
    // and 256 hint values, P[no collision] ≈ 254/256). Run a
    // few iterations to avoid the rare flake.
    let sender = generate_identity("alice".to_string());
    let recipient = generate_identity("bob".to_string());

    let cover =
        encrypt_osl_phase4_to_pubkeys(&sender.x25519_secret, &[recipient.x25519_public], "secret")
            .expect("encrypt");

    // Try up to 50 strangers; expect at least one (in practice
    // ~99.2%) returns NoMatchingSlot. A stranger with a random
    // hint collision will hit MessageAeadFailed instead — both
    // are valid Err shapes that prove "stranger can't decrypt."
    let mut saw_no_match = false;
    let mut saw_aead_fail = false;
    for _ in 0..50 {
        let stranger = generate_identity("eve".to_string());
        match decrypt_osl_phase4_cover(&stranger.x25519_secret, &sender.x25519_public, &cover) {
            Err(DecodeError::NoMatchingSlot) => saw_no_match = true,
            Err(DecodeError::MessageAeadFailed(_)) => saw_aead_fail = true,
            Ok(_) => panic!("stranger should not decrypt"),
            Err(other) => panic!("unexpected error {other:?}"),
        }
        if saw_no_match {
            break;
        }
    }
    assert!(
        saw_no_match || saw_aead_fail,
        "stranger should hit NoMatchingSlot or MessageAeadFailed"
    );
}

#[test]
fn message_aead_failed_when_msg_ciphertext_corrupted() {
    let sender = generate_identity("alice".to_string());
    let recipient = generate_identity("bob".to_string());
    let cover = encrypt_osl_phase4_to_pubkeys(
        &sender.x25519_secret,
        &[recipient.x25519_public],
        "untouched",
    )
    .expect("encrypt");

    // Decode → flip one bit in the message ciphertext (last
    // byte) → re-encode.
    let body = cover.strip_prefix("DPC0::").expect("Mode 0 prefix");
    let mut raw = STANDARD.decode(body).expect("base64");
    let last = raw.len() - 1;
    raw[last] ^= 0x01;

    let err = decrypt_osl_phase4_from_wire(&recipient.x25519_secret, &sender.x25519_public, &raw)
        .expect_err("corrupted msg ct should fail");
    assert!(
        matches!(err, DecodeError::MessageAeadFailed(_)),
        "got {err:?}"
    );
}

// ---- Sender-self-decrypt UX flow ----

#[test]
fn sender_decrypts_own_message_with_auto_appended_slot() {
    // The optimistic-render UX guarantee: when sender hits
    // Enter, the server bounces MESSAGE_CREATE back to them.
    // The decoder uses their own secret + their own pubkey
    // (auto-appended slot) and recovers plaintext.
    let sender = generate_identity("alice".to_string());
    let recipient = generate_identity("bob".to_string());

    let cover = encrypt_osl_phase4_to_pubkeys(
        &sender.x25519_secret,
        &[recipient.x25519_public],
        "first dogfood message",
    )
    .expect("encrypt");

    // From the receive-hook's perspective: lookup sender's
    // pubkey by the message author's user_id (which IS the
    // sender themselves on bounce-back), pass to decoder.
    let recovered = decrypt_osl_phase4_cover(
        &sender.x25519_secret, // recipient_secret = own secret
        &sender.x25519_public, // sender_pub = own pub (it's
        // their own message, so the
        // "sender" in the wire IS them)
        &cover,
    )
    .expect("self-decrypt");
    assert_eq!(recovered, b"first dogfood message");
}

#[test]
fn recipient_uses_sender_pub_not_their_own() {
    // Sanity: when bob receives alice's message, decoder needs
    // alice's pubkey (NOT bob's own pubkey for the sender
    // parameter). Verifying we don't accidentally swap
    // arguments somewhere.
    let alice = generate_identity("alice".to_string());
    let bob = generate_identity("bob".to_string());
    let cover = encrypt_osl_phase4_to_pubkeys(
        &alice.x25519_secret,
        &[bob.x25519_public],
        "from alice to bob",
    )
    .expect("encrypt");

    // Wrong: bob using bob's pub as sender_pub.
    let wrong = decrypt_osl_phase4_cover(
        &bob.x25519_secret,
        &bob.x25519_public, // <-- should be alice
        &cover,
    );
    assert!(wrong.is_err(), "wrong sender pub should not decrypt");

    // Right: bob using alice's pub.
    let right = decrypt_osl_phase4_cover(&bob.x25519_secret, &alice.x25519_public, &cover)
        .expect("correct sender pub should decrypt");
    assert_eq!(right, b"from alice to bob");
}

// ---- SenderPubkeyCache semantics ----

#[test]
fn cache_get_returns_none_for_unknown_user() {
    let cache = SenderPubkeyCache::default();
    assert!(cache.get("alice").is_none());
}

#[test]
fn cache_get_returns_inserted_pubkey() {
    let cache = SenderPubkeyCache::default();
    let id = generate_identity("alice".to_string());
    cache.insert("alice".to_string(), id.x25519_public);

    let got = cache.get("alice").expect("hit");
    assert_eq!(got.as_bytes(), id.x25519_public.as_bytes());
}

#[test]
fn cache_replace_overwrites_prior_entry() {
    let cache = SenderPubkeyCache::default();
    let id_old = generate_identity("alice".to_string());
    let id_new = generate_identity("alice".to_string());

    cache.insert("alice".to_string(), id_old.x25519_public);
    cache.insert("alice".to_string(), id_new.x25519_public);

    let got = cache.get("alice").expect("hit");
    assert_eq!(got.as_bytes(), id_new.x25519_public.as_bytes());
    assert_ne!(got.as_bytes(), id_old.x25519_public.as_bytes());
}

#[test]
fn cache_clear_evicts_all() {
    let cache = SenderPubkeyCache::default();
    let id = generate_identity("alice".to_string());
    cache.insert("alice".to_string(), id.x25519_public);
    cache.insert("bob".to_string(), id.x25519_public);
    assert_eq!(cache.len(), 2);

    cache.clear();
    assert_eq!(cache.len(), 0);
    assert!(cache.get("alice").is_none());
}

// TTL expiry isn't directly testable without wall-clock control
// or injecting a clock. The 5-minute window is short enough to
// be observed in production but too long for a unit test. The
// expiry path IS exercised on cache.get() — see the lazy-eviction
// branch in SenderPubkeyCache::get. Manual verification: change
// SENDER_PUBKEY_CACHE_TTL to Duration::from_millis(50) locally,
// insert + sleep + get, observe None.
