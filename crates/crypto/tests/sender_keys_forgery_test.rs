//! The sender-key AEAD authenticates keys supplied by its caller, but v=5 has
//! no sender signature.  A group member with Alice's distributed receiver
//! chain can therefore create a message that opens under Alice's chain and
//! identity keys.  Carrier attribution is the separate IPC-layer defence.

use crypto::aead::{self, Key};
use crypto::hkdf;
use crypto::random;
use crypto::sender_keys::{
    canonical_ad_sender_keys, EncryptedMessage, Header, SenderContext, SenderKeyState,
    SESSION_VERSION_V1,
};
use crypto::Error;
use keystore;

#[test]
fn receiver_chain_holder_can_forge_a_message_that_opens_as_its_owner() {
    let alice = keystore::generate_identity("alice-forgery-test".to_string());
    let mallory = keystore::generate_identity("mallory-forgery-test".to_string());
    let group_id = b"sender-key-forgery-group".to_vec();
    let claimed_alice = b"alice-carrier-id".to_vec();

    // Mallory has Alice's receiver chain after receiving Alice's SKDM.
    let mut alice_chain = SenderKeyState::new();
    alice_chain.install_sender().expect("Alice chain installs");
    let (chain_id, rotation_root, physical_device_id) = {
        let chain = alice_chain.sender_chain().expect("Alice chain exists");
        (
            chain.current_chain_id(),
            chain.rotation_root_bytes(),
            chain.physical_device_id(),
        )
    };

    let mut mallory_receiver = SenderKeyState::new();
    mallory_receiver
        .install_receiver(
            claimed_alice.clone(),
            chain_id,
            &rotation_root,
            physical_device_id,
        )
        .expect("Alice receiver chain installs for Mallory");

    let alice_context = SenderContext {
        sender_ik_x25519_pub: alice.x25519_public,
        sender_ik_mlkem_pub: alice.mlkem_public_bytes.to_vec(),
        group_id,
        session_version: SESSION_VERSION_V1,
    };
    // The receiver-chain root derives the same first chain key as Alice's
    // sender state.  Mallory can follow the public construction to make a v=5
    // encrypted header and payload whose AD claims Alice's public keys.
    let chain_key = hkdf::derive_32(
        &rotation_root,
        &chain_id.to_le_bytes(),
        b"sender-keys/chain-init",
    )
    .expect("receiver root derives Alice's initial chain key");
    let header_key = Key::from_bytes(
        hkdf::derive_32(&[], &chain_key, b"sender-keys/header-key")
            .expect("chain key derives a header key"),
    );
    let message_key = Key::from_bytes(
        hkdf::derive_32(&[], &chain_key, b"sender-keys/msg-key")
            .expect("chain key derives a message key"),
    );
    let header = Header {
        physical_device_id,
        chain_id,
        n: 0,
        prev_chain_length: 0,
        session_version: SESSION_VERSION_V1,
    };
    let header_nonce = random::random_nonce();
    let enc_header = aead::seal(&header_key, &header_nonce, b"", &header.to_bytes())
        .expect("shared chain can encrypt the header");
    let message_nonce = random::random_nonce();
    let mut ad = canonical_ad_sender_keys(
        alice_context.sender_ik_x25519_pub.as_bytes(),
        &alice_context.sender_ik_mlkem_pub,
        &physical_device_id,
        &alice_context.group_id,
        chain_id,
        0,
        0,
        SESSION_VERSION_V1,
    );
    ad.extend_from_slice(&enc_header);
    let forged = EncryptedMessage {
        header_nonce,
        enc_header,
        message_nonce,
        ciphertext: aead::seal(
            &message_key,
            &message_nonce,
            &ad,
            b"Mallory wrote this, but it opens as Alice",
        )
        .expect("shared chain can encrypt with Alice's public keys in AD"),
    };

    let mallory_context = SenderContext {
        sender_ik_x25519_pub: mallory.x25519_public,
        sender_ik_mlkem_pub: mallory.mlkem_public_bytes.to_vec(),
        group_id: alice_context.group_id.clone(),
        session_version: SESSION_VERSION_V1,
    };
    let err = mallory_receiver
        .decrypt_from(&claimed_alice, &forged, &mallory_context)
        .expect_err("the same ciphertext must not open under Mallory's public keys");
    assert!(matches!(err, Error::AeadFailure));

    // A failed attempt did not advance Mallory's receiver state, so the same
    // ciphertext then opens when presented as Alice.
    let opened = mallory_receiver
        .decrypt_from(&claimed_alice, &forged, &alice_context)
        .expect("forged ciphertext opens under Alice's receiver chain");
    assert_eq!(opened, b"Mallory wrote this, but it opens as Alice");
}
