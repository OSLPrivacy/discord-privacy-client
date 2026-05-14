//! Phase 7b control-message ser/de tests.
//!
//! Locks the CBOR wire-shape for [`BurnMarker`] and the
//! `msg_type` → body dispatch through `wire_v2`. 9-C1 removed the
//! `WhitelistInvitation` / `WhitelistResponse` ser/de tests
//! alongside the invitation handshake.

use crypto::x25519;
use ipc::control_messages::{deserialize_burn_marker, serialize_burn_marker, BurnMarker};
use ipc::scope::Scope;
use ipc::wire_v2::{decrypt_v2, encrypt_v2, MSG_TYPE_BURN, MSG_TYPE_CONTENT};

// ---- 1. burn marker round-trip ----

#[test]
fn test_burn_marker_serialize_round_trip() {
    let m = BurnMarker {
        scope: Scope::dm("1502770642930634812"),
        burned_at: 1_700_000_000,
    };
    let bytes = serialize_burn_marker(&m).expect("serialize");
    let back = deserialize_burn_marker(&bytes).expect("deserialize");
    assert_eq!(back, m);
}

// ---- 2. type byte preserved through encrypt_v2 wrap ----

#[test]
fn test_encrypt_v2_with_control_message_type_byte_preserved() {
    let (sender_sk, _) = x25519::generate_keypair();
    let sender_pk = x25519::derive_public(&sender_sk);
    let (recipient_sk, recipient_pk) = x25519::generate_keypair();

    let burn = BurnMarker {
        scope: Scope::dm("1502770642930634812"),
        burned_at: 1_700_000_000,
    };
    let body = serialize_burn_marker(&burn).unwrap();

    let wire = encrypt_v2(&body, &[recipient_pk], MSG_TYPE_BURN, &sender_sk)
        .expect("encrypt v=2 burn marker");
    let recovered = decrypt_v2(&wire, &recipient_sk, &sender_pk).expect("decrypt v=2 burn marker");
    assert_eq!(recovered.msg_type, MSG_TYPE_BURN);

    let back = deserialize_burn_marker(&recovered.plaintext).expect("deserialize recovered body");
    assert_eq!(back, burn);
}

// ---- 3. decrypt dispatch on type byte ----

#[test]
fn test_decrypt_dispatch_on_type() {
    let (sender_sk, _) = x25519::generate_keypair();
    let sender_pk = x25519::derive_public(&sender_sk);
    let (recipient_sk, recipient_pk) = x25519::generate_keypair();

    let scope = Scope::dm("1502770642930634812");

    // 0x00 content — body is raw plaintext bytes.
    let content_wire = encrypt_v2(b"hello", &[recipient_pk], MSG_TYPE_CONTENT, &sender_sk).unwrap();
    let content = decrypt_v2(&content_wire, &recipient_sk, &sender_pk).unwrap();
    assert_eq!(content.msg_type, MSG_TYPE_CONTENT);
    assert_eq!(content.plaintext, b"hello".to_vec());

    // 0x01 burn marker.
    let burn = BurnMarker {
        scope: scope.clone(),
        burned_at: 42,
    };
    let burn_wire = encrypt_v2(
        &serialize_burn_marker(&burn).unwrap(),
        &[recipient_pk],
        MSG_TYPE_BURN,
        &sender_sk,
    )
    .unwrap();
    let burn_back = decrypt_v2(&burn_wire, &recipient_sk, &sender_pk).unwrap();
    assert_eq!(burn_back.msg_type, MSG_TYPE_BURN);
    assert_eq!(deserialize_burn_marker(&burn_back.plaintext).unwrap(), burn);
}
