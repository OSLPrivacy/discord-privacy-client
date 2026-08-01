//! Phase 9-A1 v=3 PQ-hybrid wire integration tests.
//!
//! Drives the `wire_v2::encrypt_v3` / `wire_v2::decrypt_v3` pair
//! directly with raw crypto inputs (no AppState, no Tauri, no
//! commands surface) so the assertions stay on the wire byte layout
//! + cryptographic round-trip rather than on AppState behaviour.
//!
//! What we lock:
//!  1. Single-recipient round-trip preserves bytes + msg_type.
//!  2. Multi-recipient round-trip: every recipient decrypts the
//!     same plaintext.
//!  3. The version byte at offset 0 of the decoded payload is 0x03.
//!  4. Sender X25519 ik pub is embedded in the global header at
//!     offset 2..34.
//!  5. Per-recipient slot length matches SLOT_V3_BYTES.
//!  6. Tampering with one recipient's slot breaks only that slot;
//!     others still decrypt.
//!  7. Tampering with the body ciphertext breaks decryption for
//!     every recipient.
//!  8. A non-recipient (correct identity but never enrolled in the
//!     recipient list) gets NoMatchingSlot.
//!  9. v=2 wires still decode via the v=2 path (legacy preserved).
//! 10. v=3 message size for 1 recipient lands in the spec'd
//!     ballpark (~1300 bytes after base64 + DPC0:: prefix).

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use crypto::{ml_kem_768, x25519};
use ipc::wire_v2::{
    decrypt_v2, decrypt_v3, encrypt_v2, encrypt_v3, RecipientV3, MSG_TYPE_CONTENT, SLOT_V3_BYTES,
    WIRE_VERSION_V3,
};

fn make_recipient() -> (
    x25519::SecretKey,
    x25519::PublicKey,
    ml_kem_768::DecapsulationKey,
    ml_kem_768::EncapsulationKey,
) {
    let (x_sk, x_pk) = x25519::generate_keypair();
    let (m_sk, m_pk) = ml_kem_768::generate_keypair();
    (x_sk, x_pk, m_sk, m_pk)
}

fn make_sender() -> (x25519::SecretKey, x25519::PublicKey) {
    x25519::generate_keypair()
}

#[test]
fn v3_single_recipient_round_trip() {
    let (sender_sk, sender_pk) = make_sender();
    let (rx_sk, rx_pk, rx_msk, rx_mpk) = make_recipient();
    let plaintext = b"hello phase 9 A1";

    let wire = encrypt_v3(
        &sender_sk,
        &sender_pk,
        &[RecipientV3 {
            x25519_pub: rx_pk,
            mlkem_pub: rx_mpk,
        }],
        MSG_TYPE_CONTENT,
        plaintext,
    )
    .expect("encrypt v=3");
    assert!(wire.starts_with("DPC0::"));

    let recovered = decrypt_v3(&wire, &rx_sk, &rx_msk).expect("decrypt v=3");
    assert_eq!(recovered.msg_type, MSG_TYPE_CONTENT);
    assert_eq!(recovered.plaintext, plaintext.to_vec());
}

#[test]
fn v3_version_byte_in_payload_is_0x03() {
    let (sender_sk, sender_pk) = make_sender();
    let (_, rx_pk, _, rx_mpk) = make_recipient();
    let wire = encrypt_v3(
        &sender_sk,
        &sender_pk,
        &[RecipientV3 {
            x25519_pub: rx_pk,
            mlkem_pub: rx_mpk,
        }],
        MSG_TYPE_CONTENT,
        b"x",
    )
    .unwrap();
    let body = wire.strip_prefix("DPC0::").unwrap();
    let raw = STANDARD.decode(body).unwrap();
    assert_eq!(raw[0], WIRE_VERSION_V3);
}

#[test]
fn v3_sender_ik_pub_in_global_header() {
    let (sender_sk, sender_pk) = make_sender();
    let (_, rx_pk, _, rx_mpk) = make_recipient();
    let wire = encrypt_v3(
        &sender_sk,
        &sender_pk,
        &[RecipientV3 {
            x25519_pub: rx_pk,
            mlkem_pub: rx_mpk,
        }],
        MSG_TYPE_CONTENT,
        b"hi",
    )
    .unwrap();
    let body = wire.strip_prefix("DPC0::").unwrap();
    let raw = STANDARD.decode(body).unwrap();
    // [version | msg_type | sender_ik(32) | N | slots ...]
    assert_eq!(&raw[2..34], sender_pk.as_bytes().as_slice());
}

#[test]
fn v3_multi_recipient_each_decrypts_independently() {
    let (sender_sk, sender_pk) = make_sender();
    let (rx1_sk, rx1_pk, rx1_msk, rx1_mpk) = make_recipient();
    let (rx2_sk, rx2_pk, rx2_msk, rx2_mpk) = make_recipient();
    let (rx3_sk, rx3_pk, rx3_msk, rx3_mpk) = make_recipient();

    let plaintext = b"three recipients see this";
    let wire = encrypt_v3(
        &sender_sk,
        &sender_pk,
        &[
            RecipientV3 {
                x25519_pub: rx1_pk,
                mlkem_pub: rx1_mpk,
            },
            RecipientV3 {
                x25519_pub: rx2_pk,
                mlkem_pub: rx2_mpk,
            },
            RecipientV3 {
                x25519_pub: rx3_pk,
                mlkem_pub: rx3_mpk,
            },
        ],
        MSG_TYPE_CONTENT,
        plaintext,
    )
    .unwrap();

    for (sk, msk) in [
        (&rx1_sk, &rx1_msk),
        (&rx2_sk, &rx2_msk),
        (&rx3_sk, &rx3_msk),
    ] {
        let r = decrypt_v3(&wire, sk, msk).unwrap();
        assert_eq!(r.plaintext, plaintext.to_vec());
    }
}

#[test]
fn v3_recipient_count_in_header_matches_slot_count() {
    let (sender_sk, sender_pk) = make_sender();
    let (_, rx1_pk, _, rx1_mpk) = make_recipient();
    let (_, rx2_pk, _, rx2_mpk) = make_recipient();
    let wire = encrypt_v3(
        &sender_sk,
        &sender_pk,
        &[
            RecipientV3 {
                x25519_pub: rx1_pk,
                mlkem_pub: rx1_mpk,
            },
            RecipientV3 {
                x25519_pub: rx2_pk,
                mlkem_pub: rx2_mpk,
            },
        ],
        MSG_TYPE_CONTENT,
        b"y",
    )
    .unwrap();
    let raw = STANDARD
        .decode(wire.strip_prefix("DPC0::").unwrap())
        .unwrap();
    assert_eq!(raw[34], 2);
    // version + msg_type + sender_ik(32) + N(1) + 2 × SLOT_V3_BYTES
    // = 4 + 32 + 2 × SLOT_V3_BYTES before body framing.
    let expected_min = 4 + 32 + 2 * SLOT_V3_BYTES;
    assert!(raw.len() >= expected_min);
}

#[test]
fn v3_tampering_one_slot_does_not_break_other_recipients() {
    let (sender_sk, sender_pk) = make_sender();
    let (rx1_sk, rx1_pk, rx1_msk, rx1_mpk) = make_recipient();
    let (_rx2_sk, rx2_pk, _rx2_msk, rx2_mpk) = make_recipient();

    let wire = encrypt_v3(
        &sender_sk,
        &sender_pk,
        &[
            RecipientV3 {
                x25519_pub: rx1_pk,
                mlkem_pub: rx1_mpk,
            },
            RecipientV3 {
                x25519_pub: rx2_pk,
                mlkem_pub: rx2_mpk,
            },
        ],
        MSG_TYPE_CONTENT,
        b"tamper-test",
    )
    .unwrap();

    // Flip a byte inside slot #2 (rx2's slot). Recipient #1 should
    // still decrypt fine.
    let body = wire.strip_prefix("DPC0::").unwrap();
    let mut raw = STANDARD.decode(body).unwrap();
    let slot2_start = 4 + 32 + SLOT_V3_BYTES; // version+msg_type+ik+N then past slot 1
    raw[slot2_start + 100] ^= 0x01; // flip a byte deep in slot 2's mlkem ct
    let tampered = format!("DPC0::{}", STANDARD.encode(&raw));

    let r1 = decrypt_v3(&tampered, &rx1_sk, &rx1_msk).expect("rx1 still decrypts");
    assert_eq!(r1.plaintext, b"tamper-test".to_vec());
}

#[test]
fn v3_tampering_body_ct_breaks_every_recipient() {
    let (sender_sk, sender_pk) = make_sender();
    let (rx1_sk, rx1_pk, rx1_msk, rx1_mpk) = make_recipient();
    let (rx2_sk, rx2_pk, rx2_msk, rx2_mpk) = make_recipient();

    let wire = encrypt_v3(
        &sender_sk,
        &sender_pk,
        &[
            RecipientV3 {
                x25519_pub: rx1_pk,
                mlkem_pub: rx1_mpk,
            },
            RecipientV3 {
                x25519_pub: rx2_pk,
                mlkem_pub: rx2_mpk,
            },
        ],
        MSG_TYPE_CONTENT,
        b"body-tamper",
    )
    .unwrap();

    let mut raw = STANDARD
        .decode(wire.strip_prefix("DPC0::").unwrap())
        .unwrap();
    // Last byte is inside body tag / ciphertext — flipping it
    // invalidates the AEAD for every recipient.
    let last = raw.len() - 1;
    raw[last] ^= 0x01;
    let tampered = format!("DPC0::{}", STANDARD.encode(&raw));

    assert!(decrypt_v3(&tampered, &rx1_sk, &rx1_msk).is_err());
    assert!(decrypt_v3(&tampered, &rx2_sk, &rx2_msk).is_err());
}

#[test]
fn v3_non_recipient_gets_no_matching_slot() {
    let (sender_sk, sender_pk) = make_sender();
    let (_, rx_pk, _, rx_mpk) = make_recipient();
    // Outsider — never named in the recipient list.
    let (outsider_sk, _outsider_pk, outsider_msk, _outsider_mpk) = make_recipient();

    let wire = encrypt_v3(
        &sender_sk,
        &sender_pk,
        &[RecipientV3 {
            x25519_pub: rx_pk,
            mlkem_pub: rx_mpk,
        }],
        MSG_TYPE_CONTENT,
        b"private",
    )
    .unwrap();

    let err = decrypt_v3(&wire, &outsider_sk, &outsider_msk).unwrap_err();
    // Surfaces via the V2Error::NoMatchingSlot variant — Display
    // string is the canonical contract.
    let s = format!("{err}");
    assert!(
        s.contains("not a recipient"),
        "expected NoMatchingSlot-style error, got: {s}"
    );
}

#[test]
fn v3_does_not_decrypt_v2_wires() {
    // v=2 wires must NOT decode via decrypt_v3 — wrong version byte
    // should reject early so we don't waste a pqxdh::respond on
    // an unrelated payload.
    let (sender_sk, sender_pk) = make_sender();
    let (rx_sk, rx_pk, _, _) = make_recipient();
    let _ = sender_pk;
    let _ = rx_pk;

    let v2_wire = encrypt_v2(b"v2-only", &[rx_pk], MSG_TYPE_CONTENT, &sender_sk).unwrap();

    // decrypt_v2 path still works for v=2 wires.
    let v2_back = decrypt_v2(&v2_wire, &rx_sk, &sender_pk).unwrap();
    assert_eq!(v2_back.plaintext, b"v2-only".to_vec());

    // Feeding the same v=2 wire to decrypt_v3 must fail (wrong
    // version byte).
    let (_, _, rx_msk_v3, _) = make_recipient(); // any mlkem secret; the version check fires first
    assert!(decrypt_v3(&v2_wire, &rx_sk, &rx_msk_v3).is_err());
}

#[test]
fn v3_single_recipient_wire_size_is_in_ballpark() {
    let (sender_sk, sender_pk) = make_sender();
    let (_, rx_pk, _, rx_mpk) = make_recipient();
    let wire = encrypt_v3(
        &sender_sk,
        &sender_pk,
        &[RecipientV3 {
            x25519_pub: rx_pk,
            mlkem_pub: rx_mpk,
        }],
        MSG_TYPE_CONTENT,
        b"x",
    )
    .unwrap();
    let raw_len = STANDARD
        .decode(wire.strip_prefix("DPC0::").unwrap())
        .unwrap()
        .len();
    // version + msg_type + sender_ik(32) + N(1) + 1 × SLOT_V3_BYTES
    // (1190) + body_nonce(12) + 1B body ct + 16B tag = 1255.
    // Allow some slack for AEAD padding / metadata.
    assert!(
        raw_len > 1200 && raw_len < 1300,
        "v=3 single-recipient raw len = {raw_len} bytes (expected ~1255)"
    );
}
