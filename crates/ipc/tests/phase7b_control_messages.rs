//! Phase 7b control-message ser/de tests.
//!
//! Locks the CBOR wire-shape for [`BurnMarker`],
//! [`WhitelistInvitation`], [`WhitelistResponse`] and the
//! `msg_type` → body dispatch through `wire_v2`.

use crypto::x25519;
use ipc::control_messages::{
    deserialize_burn_marker, deserialize_whitelist_invitation, deserialize_whitelist_response,
    serialize_burn_marker, serialize_whitelist_invitation, serialize_whitelist_response,
    BurnMarker, WhitelistInvitation, WhitelistResponse,
};
use ipc::scope::Scope;
use ipc::wire_v2::{
    decrypt_v2, encrypt_v2, MSG_TYPE_BURN, MSG_TYPE_CONTENT, MSG_TYPE_WHITELIST_INVITATION,
    MSG_TYPE_WHITELIST_RESPONSE,
};

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

// ---- 2. invitation round-trip ----

#[test]
fn test_invitation_serialize_round_trip() {
    let (_sk, pk) = x25519::generate_keypair();
    let m = WhitelistInvitation {
        from_discord_id: "1477008451799482419".to_string(),
        from_pubkey: pk,
        scope: Scope::server_channel("9876", "5432"),
        sent_at: 1_700_000_001,
    };
    let bytes = serialize_whitelist_invitation(&m).expect("serialize");
    let back = deserialize_whitelist_invitation(&bytes).expect("deserialize");
    assert_eq!(back, m);
    assert_eq!(back.from_pubkey.as_bytes(), pk.as_bytes());
}

// ---- 3. response round-trip ----

#[test]
fn test_response_serialize_round_trip() {
    for accepted in [true, false] {
        let m = WhitelistResponse {
            scope: Scope::gc("gc-123"),
            accepted,
            responded_at: 1_700_000_002,
        };
        let bytes = serialize_whitelist_response(&m).unwrap();
        let back = deserialize_whitelist_response(&bytes).unwrap();
        assert_eq!(back, m);
    }
}

// ---- 4. type byte preserved through encrypt_v2 wrap ----

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

// ---- 5. decrypt dispatch on type byte ----

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

    // 0x02 invitation.
    let (_isk, ipk) = x25519::generate_keypair();
    let inv = WhitelistInvitation {
        from_discord_id: "from".to_string(),
        from_pubkey: ipk,
        scope: scope.clone(),
        sent_at: 1,
    };
    let inv_wire = encrypt_v2(
        &serialize_whitelist_invitation(&inv).unwrap(),
        &[recipient_pk],
        MSG_TYPE_WHITELIST_INVITATION,
        &sender_sk,
    )
    .unwrap();
    let inv_back = decrypt_v2(&inv_wire, &recipient_sk, &sender_pk).unwrap();
    assert_eq!(inv_back.msg_type, MSG_TYPE_WHITELIST_INVITATION);
    assert_eq!(
        deserialize_whitelist_invitation(&inv_back.plaintext).unwrap(),
        inv
    );

    // 0x03 response.
    let resp = WhitelistResponse {
        scope,
        accepted: true,
        responded_at: 2,
    };
    let resp_wire = encrypt_v2(
        &serialize_whitelist_response(&resp).unwrap(),
        &[recipient_pk],
        MSG_TYPE_WHITELIST_RESPONSE,
        &sender_sk,
    )
    .unwrap();
    let resp_back = decrypt_v2(&resp_wire, &recipient_sk, &sender_pk).unwrap();
    assert_eq!(resp_back.msg_type, MSG_TYPE_WHITELIST_RESPONSE);
    assert_eq!(
        deserialize_whitelist_response(&resp_back.plaintext).unwrap(),
        resp
    );
}
