//! Layer 10 / Phase 4.5: Rust-side round-trip verification.
//!
//! Phase 4 ships the encrypt half of the OSL pipeline. There is no
//! Phase 5 receive-side decoder yet — production decode lives behind
//! a `osl_decrypt_message` Tauri command + a Discord-side render
//! hook that don't exist yet. **This test file is the cryptographic
//! round-trip proof:** generate two identities, encrypt with one,
//! decrypt with the other, assert plaintext equality.
//!
//! When Phase 5 lands, the [`decode_phase4_wire`] helper below moves
//! into the production receive path almost verbatim. Keeping the
//! decoder here for now means the wire format has a working
//! reference implementation, the tests catch any encoder breakage
//! immediately, and the Phase 5 extraction is mechanical.
//!
//! ## What this test exercises
//!
//! - `ipc::commands::encrypt_osl_phase4_to_pubkeys` (the pure
//!   encoder; no `AppState`, no HTTP).
//! - The full wire format: version byte, recipient count, slot
//!   layout (`pub_hint` + `nonce_k` + `wrap_k`), bulk message
//!   nonce + ciphertext.
//! - X25519 ECDH, HKDF-SHA256 wrap-key derivation,
//!   XChaCha20-Poly1305 AEAD on both legs.
//! - Mode 0 stego encode/decode (the outer `DPC0::` prefix).
//! - Recipient slot scan via `pub_hint` low-byte match.
//!
//! ## What this test does NOT exercise
//!
//! - `cmd_osl_encrypt_message` (the IO wrapper around the pure
//!   encoder). That path's keyserver / channel-config IO is
//!   covered by the keystore unit tests + the Phase 4 acceptance
//!   procedure. Round-tripping it would require booting a
//!   keyserver + tempfile-rooting `keystore::osl_config_dir` per
//!   test, which is outside the scope of "did the cryptography
//!   round-trip?"
//! - Phase 5 detection of "this Discord message is for me" — the
//!   test feeds the cover string directly into the decoder. The
//!   real receive path will pre-filter on the `DPC0::` prefix
//!   first.

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use crypto::{aead, hkdf, x25519};
use ipc::commands::{
    encrypt_osl_phase4_to_pubkeys, OSL_PHASE4_AD_MSG, OSL_PHASE4_AD_WRAP,
    OSL_PHASE4_FIXED_FRAMING_BYTES, OSL_PHASE4_HKDF_INFO_WRAP, OSL_PHASE4_PER_RECIPIENT_BYTES,
    OSL_PHASE4_PLAINTEXT_BYTE_CAP, OSL_PHASE4_WIRE_VERSION,
};
use keystore::generate_identity;

/// Reference decoder for the Phase 4 wire format. Mirrors the
/// encoder's slot layout exactly. Returns the recovered plaintext
/// bytes on success.
///
/// Inputs:
/// - `cover` — the stego'd Mode 0 string returned by
///   [`encrypt_osl_phase4_to_pubkeys`]. Must start with `"DPC0::"`.
/// - `recipient_secret` — the recipient's X25519 identity secret.
/// - `sender_pub` — the sender's X25519 identity public key. In
///   production (Phase 5) this comes from a keyserver lookup keyed
///   on the Discord-message-author user_id; for this test we have
///   it directly from the sender's identity.
///
/// Errors are flat strings with a `decode:` prefix so test
/// failures distinguish encoder bugs from decoder bugs in panic
/// output.
fn decode_phase4_wire(
    cover: &str,
    recipient_secret: &x25519::SecretKey,
    sender_pub: &x25519::PublicKey,
) -> Result<Vec<u8>, String> {
    // Outer Mode 0 strip.
    let raw = stego::decode_mode0(cover).map_err(|e| format!("decode: stego: {e}"))?;

    // Bare-minimum length check before slot indexing.
    if raw.len() < OSL_PHASE4_FIXED_FRAMING_BYTES {
        return Err(format!(
            "decode: wire too short: {} < {}",
            raw.len(),
            OSL_PHASE4_FIXED_FRAMING_BYTES
        ));
    }

    let version = raw[0];
    if version != OSL_PHASE4_WIRE_VERSION {
        return Err(format!(
            "decode: unknown wire version 0x{:02x} (expected 0x{:02x})",
            version, OSL_PHASE4_WIRE_VERSION
        ));
    }
    let n = raw[1] as usize;
    if n == 0 {
        return Err("decode: N == 0 in wire header".to_string());
    }

    let expected_min = OSL_PHASE4_FIXED_FRAMING_BYTES + n * OSL_PHASE4_PER_RECIPIENT_BYTES;
    if raw.len() < expected_min {
        return Err(format!(
            "decode: wire shorter than slot table requires: {} < {}",
            raw.len(),
            expected_min
        ));
    }

    // Compute receiver's own pub_hint to find candidate slots.
    let recipient_pub = x25519::derive_public(recipient_secret);
    let our_hint = recipient_pub.as_bytes()[0];

    // Recover the shared secret + wrap key once — every slot
    // belonging to us derives from the same (recipient_sk,
    // sender_pk) pair.
    let shared = x25519::diffie_hellman(recipient_secret, sender_pub)
        .map_err(|e| format!("decode: ECDH failed: {e}"))?;
    let wrap_key_bytes = hkdf::derive_32(&[], shared.as_bytes(), OSL_PHASE4_HKDF_INFO_WRAP)
        .map_err(|e| format!("decode: HKDF wrap-key: {e}"))?;
    let wrap_key = aead::Key::from_bytes(wrap_key_bytes);

    // Walk slots; for any with a matching pub_hint, attempt
    // wrap-decrypt. AEAD authentication tag tells us
    // success/failure unambiguously, so we can probe candidate
    // slots without leaking timing info beyond "did some slot
    // match." (In production, false-positive pub_hint matches
    // are rare — only ~1 / 256 — and the AEAD tag check is
    // constant-time.)
    let slot_size = OSL_PHASE4_PER_RECIPIENT_BYTES;
    let mut session_key: Option<aead::Key> = None;
    for slot_ix in 0..n {
        let base = 2 + slot_ix * slot_size;
        let hint = raw[base];
        if hint != our_hint {
            continue;
        }
        let nonce_start = base + 1;
        let nonce_end = nonce_start + aead::NONCE_SIZE;
        let wrap_start = nonce_end;
        let wrap_end = wrap_start + aead::KEY_SIZE + aead::TAG_SIZE;
        let mut nonce_bytes = [0u8; aead::NONCE_SIZE];
        nonce_bytes.copy_from_slice(&raw[nonce_start..nonce_end]);
        let nonce = aead::Nonce::from_bytes(nonce_bytes);
        let wrap_ct = &raw[wrap_start..wrap_end];

        match aead::open(&wrap_key, &nonce, OSL_PHASE4_AD_WRAP, wrap_ct) {
            Ok(plaintext_bytes) => {
                if plaintext_bytes.len() != aead::KEY_SIZE {
                    return Err(format!(
                        "decode: wrap plaintext wrong length: got {}, want {}",
                        plaintext_bytes.len(),
                        aead::KEY_SIZE
                    ));
                }
                let mut sk = [0u8; aead::KEY_SIZE];
                sk.copy_from_slice(&plaintext_bytes);
                session_key = Some(aead::Key::from_bytes(sk));
                break;
            }
            Err(_) => {
                // pub_hint collision; try next slot. The AEAD tag
                // check failed cleanly — no oracle leak.
                continue;
            }
        }
    }
    let session_key = session_key.ok_or_else(|| {
        "decode: no slot matched our pub_hint and successfully unwrapped".to_string()
    })?;

    // Bulk message decrypt.
    let msg_nonce_start = 2 + n * slot_size;
    let msg_nonce_end = msg_nonce_start + aead::NONCE_SIZE;
    let ct_start = msg_nonce_end;
    let mut msg_nonce_bytes = [0u8; aead::NONCE_SIZE];
    msg_nonce_bytes.copy_from_slice(&raw[msg_nonce_start..msg_nonce_end]);
    let msg_nonce = aead::Nonce::from_bytes(msg_nonce_bytes);
    let ct_msg = &raw[ct_start..];
    aead::open(&session_key, &msg_nonce, OSL_PHASE4_AD_MSG, ct_msg)
        .map_err(|e| format!("decode: msg AEAD open: {e}"))
}

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
    let recovered = decode_phase4_wire(&cover, &recipient.x25519_secret, &sender.x25519_public)
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
        let recovered = decode_phase4_wire(&cover, secret, &sender.x25519_public)
            .unwrap_or_else(|e| panic!("decode for {label}: {e}"));
        assert_eq!(recovered, plaintext.as_bytes(), "decrypt mismatch for {label}");
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
    // (specifically, no slot matches her pub_hint, and even if one
    // did by chance, the wrap AEAD tag would fail open).
    let result = decode_phase4_wire(&cover, &stranger.x25519_secret, &sender.x25519_public);
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
fn zero_recipients_rejected() {
    let sender = generate_identity("alice".to_string());
    let err = encrypt_osl_phase4_to_pubkeys(&sender.x25519_secret, &[], "hi")
        .expect_err("zero recipients should fail closed");
    assert!(
        err.contains("zero recipients"),
        "error should mention zero recipients: {err}"
    );
}

#[test]
fn over_budget_recipient_count_rejected() {
    // With N=18 the per-recipient framing alone (18 * 73 = 1314 B)
    // plus the fixed 42 B framing is already 1356 B, leaving only
    // 44 B for plaintext. A 100-byte plaintext should overflow.
    let sender = generate_identity("alice".to_string());
    let recipients: Vec<x25519::PublicKey> = (0..18)
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
    // Spot-check that the framing constants match the encoder's
    // actual output: cover length should equal the predicted
    // base64-encoded size of the framed bytes.
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
        OSL_PHASE4_FIXED_FRAMING_BYTES + 1 * OSL_PHASE4_PER_RECIPIENT_BYTES + plaintext.len();
    assert_eq!(
        raw.len(),
        expected_len,
        "wire byte count should equal framing + plaintext"
    );

    // Header byte-by-byte spot check.
    assert_eq!(raw[0], OSL_PHASE4_WIRE_VERSION, "first byte = version");
    assert_eq!(raw[1], 1u8, "second byte = N");

    // Slot 0's pub_hint must equal recipient's pubkey low byte.
    let recipient_low = recipient.x25519_public.as_bytes()[0];
    assert_eq!(raw[2], recipient_low, "slot 0 pub_hint");
}
