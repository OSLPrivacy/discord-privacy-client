//! Layer 10 / Phase 4.5: Rust-side round-trip verification.
//!
//! Phase 4 ships the encrypt half of the OSL pipeline. Phase 5
//! ships the decrypt half: the production decoder
//! [`ipc::commands::decrypt_osl_phase4_cover`] is now the canonical
//! reference implementation, and this test consumes it directly.
//! (Earlier the test carried its own near-verbatim helper; promoting
//! the production decoder removes the duplication.)
//!
//! ## What this test exercises
//!
//! - `ipc::commands::encrypt_osl_phase4_to_pubkeys` — the pure
//!   encoder, including its Phase 5 auto-include-sender behaviour.
//! - `ipc::commands::decrypt_osl_phase4_cover` — the pure decoder.
//! - The full wire format: version byte, recipient count, slot
//!   layout, bulk message AEAD.
//! - X25519 ECDH, HKDF-SHA256 wrap-key derivation,
//!   XChaCha20-Poly1305 AEAD on both legs.
//! - Mode 0 stego encode/decode (the outer `DPC0::` prefix).
//! - Recipient slot scan via `pub_hint` low-byte match.
//!
//! Phase 5-specific tests (DecodeError variants, sender decrypts
//! own message, cache behaviour) live in
//! `osl_phase5_decrypt.rs`.

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use crypto::x25519;
use ipc::commands::{
    decrypt_osl_phase4_cover, encrypt_osl_phase4_to_pubkeys, OSL_PHASE4_FIXED_FRAMING_BYTES,
    OSL_PHASE4_PER_RECIPIENT_BYTES, OSL_PHASE4_PLAINTEXT_BYTE_CAP, OSL_PHASE4_WIRE_VERSION,
};
use keystore::generate_identity;

#[test]
fn one_to_one_roundtrip() {
    let sender = generate_identity("alice".to_string());
    let recipient = generate_identity("bob".to_string());

    let plaintext = "hello bob, this is alice";

    let cover = encrypt_osl_phase4_to_pubkeys(
        &sender.x25519_secret,
        &[recipient.x25519_public.clone()],
        plaintext,
    )
    .expect("encrypt");

    assert!(cover.starts_with("DPC0::"), "cover should be Mode 0 stego");
    let recovered =
        decrypt_osl_phase4_cover(&recipient.x25519_secret, &sender.x25519_public, &cover)
            .expect("decode");
    assert_eq!(recovered, plaintext.as_bytes());
}

#[test]
fn multi_recipient_each_can_decrypt() {
    let sender = generate_identity("alice".to_string());
    let r1 = generate_identity("bob".to_string());
    let r2 = generate_identity("carol".to_string());
    let r3 = generate_identity("dave".to_string());

    let plaintext = "group chat encrypted to three";

    let cover = encrypt_osl_phase4_to_pubkeys(
        &sender.x25519_secret,
        &[
            r1.x25519_public.clone(),
            r2.x25519_public.clone(),
            r3.x25519_public.clone(),
        ],
        plaintext,
    )
    .expect("encrypt");

    for (label, secret) in [
        ("bob", &r1.x25519_secret),
        ("carol", &r2.x25519_secret),
        ("dave", &r3.x25519_secret),
    ] {
        let recovered = decrypt_osl_phase4_cover(secret, &sender.x25519_public, &cover)
            .unwrap_or_else(|e| panic!("decode for {label}: {e}"));
        assert_eq!(
            recovered,
            plaintext.as_bytes(),
            "decrypt mismatch for {label}"
        );
    }
}

#[test]
fn non_recipient_cannot_decrypt() {
    let sender = generate_identity("alice".to_string());
    let recipient = generate_identity("bob".to_string());
    let stranger = generate_identity("eve".to_string());

    let cover = encrypt_osl_phase4_to_pubkeys(
        &sender.x25519_secret,
        &[recipient.x25519_public.clone()],
        "secret",
    )
    .expect("encrypt");

    // Eve has the cover string and the sender's public key — same
    // information a passive observer of the channel would have —
    // but lacks the recipient's secret. Decoding must fail
    // (no slot matches her pub_hint, or wrap AEAD tag fails on
    // any 1/256 collisions).
    let result = decrypt_osl_phase4_cover(&stranger.x25519_secret, &sender.x25519_public, &cover);
    assert!(
        result.is_err(),
        "stranger should not be able to decode; got {:?}",
        result
    );
}

#[test]
fn empty_plaintext_rejected() {
    let sender = generate_identity("alice".to_string());
    let recipient = generate_identity("bob".to_string());

    let err = encrypt_osl_phase4_to_pubkeys(
        &sender.x25519_secret,
        &[recipient.x25519_public.clone()],
        "",
    )
    .expect_err("empty plaintext should fail closed");
    assert!(err.contains("empty"), "error should mention empty: {err}");
}

#[test]
fn oversized_plaintext_rejected() {
    let sender = generate_identity("alice".to_string());
    let recipient = generate_identity("bob".to_string());

    let big = "a".repeat(OSL_PHASE4_PLAINTEXT_BYTE_CAP + 1);
    let err = encrypt_osl_phase4_to_pubkeys(
        &sender.x25519_secret,
        &[recipient.x25519_public.clone()],
        &big,
    )
    .expect_err("oversized plaintext should fail closed");
    assert!(
        err.contains("exceeds soft cap"),
        "error should mention cap: {err}"
    );
}

#[test]
fn zero_recipients_with_sender_only_still_succeeds() {
    // Phase 5: encoder auto-includes sender, so passing zero
    // explicit recipients yields a 1-recipient (self-only)
    // encryption rather than a zero-recipient error. This
    // matches the expected dogfood pattern of empty
    // channels.json entries — sender encrypts to themselves
    // for personal-note-style use.
    let sender = generate_identity("alice".to_string());
    let cover = encrypt_osl_phase4_to_pubkeys(&sender.x25519_secret, &[], "personal note")
        .expect("encrypt to self should succeed");
    let recovered = decrypt_osl_phase4_cover(&sender.x25519_secret, &sender.x25519_public, &cover)
        .expect("self-decrypt");
    assert_eq!(recovered, b"personal note");
}

#[test]
fn over_budget_recipient_count_rejected() {
    // With N=18 the per-recipient framing alone (18 * 73 = 1314 B)
    // plus the fixed 42 B framing is already 1356 B, leaving only
    // 44 B for plaintext. A 100-byte plaintext should overflow.
    // Note: encoder auto-includes sender, so the input list is
    // 17 recipients and the effective N is 18.
    let sender = generate_identity("alice".to_string());
    let recipients: Vec<x25519::PublicKey> = (0..17)
        .map(|_| generate_identity("r".to_string()).x25519_public.clone())
        .collect();
    let plaintext = "a".repeat(100);
    let err = encrypt_osl_phase4_to_pubkeys(&sender.x25519_secret, &recipients, &plaintext)
        .expect_err("over-budget should fail closed");
    assert!(
        err.contains("Mode 0 cap"),
        "error should mention Mode 0 cap: {err}"
    );
}

#[test]
fn wire_format_self_consistency_check() {
    // Spot-check the framing constants against actual output.
    // With Phase 5's auto-include-sender, a 1-recipient
    // encryption produces N=2 slots in the wire (recipient +
    // sender).
    let sender = generate_identity("alice".to_string());
    let recipient = generate_identity("bob".to_string());
    let plaintext = "exactly 32 chars long plaintext.";
    assert_eq!(plaintext.len(), 32);

    let cover = encrypt_osl_phase4_to_pubkeys(
        &sender.x25519_secret,
        &[recipient.x25519_public.clone()],
        plaintext,
    )
    .expect("encrypt");

    let body = cover.strip_prefix("DPC0::").expect("Mode 0 prefix");
    let raw = STANDARD.decode(body).expect("base64 decode");
    let expected_len =
        OSL_PHASE4_FIXED_FRAMING_BYTES + 2 * OSL_PHASE4_PER_RECIPIENT_BYTES + plaintext.len();
    assert_eq!(
        raw.len(),
        expected_len,
        "wire byte count should equal framing + 2 slots (recipient + sender) + plaintext"
    );

    // Header byte-by-byte spot check.
    assert_eq!(raw[0], OSL_PHASE4_WIRE_VERSION, "first byte = version");
    assert_eq!(raw[1], 2u8, "second byte = N (recipient + auto-sender)");

    // Slot 0 is the explicit recipient (input order preserved).
    // Slot 1 is the sender (auto-appended).
    let recipient_low = recipient.x25519_public.as_bytes()[0];
    let sender_low = sender.x25519_public.as_bytes()[0];
    assert_eq!(raw[2], recipient_low, "slot 0 pub_hint");
    assert_eq!(
        raw[2 + OSL_PHASE4_PER_RECIPIENT_BYTES],
        sender_low,
        "slot 1 pub_hint = sender (auto-appended)"
    );
}

#[test]
fn sender_can_decrypt_own_message() {
    // The Phase 5 auto-include-sender behaviour: when the
    // sender's own message bounces back from the server (as
    // MESSAGE_CREATE), they can decrypt it. Without this, the
    // sender would see DPC0:: cover for their own messages
    // until they refresh their client.
    let sender = generate_identity("alice".to_string());
    let recipient = generate_identity("bob".to_string());

    let cover = encrypt_osl_phase4_to_pubkeys(
        &sender.x25519_secret,
        &[recipient.x25519_public.clone()],
        "hello, future me",
    )
    .expect("encrypt");

    // Sender decrypts using their own secret + their own public
    // (the auto-appended slot in the wire).
    let recovered = decrypt_osl_phase4_cover(&sender.x25519_secret, &sender.x25519_public, &cover)
        .expect("sender self-decrypt");
    assert_eq!(recovered, b"hello, future me");
}

#[test]
fn explicit_self_recipient_deduped() {
    // If a caller passes their own pubkey explicitly (e.g. a
    // future channels.json that lists the sender for
    // future-proofing), the encoder dedups so we don't waste a
    // slot on it.
    let sender = generate_identity("alice".to_string());
    let recipient = generate_identity("bob".to_string());

    let cover = encrypt_osl_phase4_to_pubkeys(
        &sender.x25519_secret,
        &[
            sender.x25519_public.clone(), // explicit self
            recipient.x25519_public.clone(),
        ],
        "deduped",
    )
    .expect("encrypt");

    let body = cover.strip_prefix("DPC0::").expect("Mode 0 prefix");
    let raw = STANDARD.decode(body).expect("base64 decode");
    // N should be 2 (sender + recipient), not 3 (sender +
    // recipient + auto-appended sender).
    assert_eq!(raw[1], 2u8, "explicit self should not duplicate");
}
