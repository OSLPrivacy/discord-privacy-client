use crypto::pqxdh::SessionKey;
use crypto::ratchet::{DoubleRatchet, SessionContext, SESSION_VERSION_V1};
use crypto::sender_keys::{EncryptedMessage as SenderKeyMessage, SenderContext, SenderKeyState};
use crypto::{ml_kem_768, pqxdh, x25519};
use ipc::wire_v2::{
    decrypt_v3_for_sender, decrypt_v4, decrypt_v5, encrypt_v3, encrypt_v4_from_ratchet, encrypt_v5,
    RecipientV3, V2Error, MSG_TYPE_CONTENT,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ForgedSenderFixture {
    V3PinnedSender,
    V4RatchetSenderAd,
    V5SenderKeyAd,
}

#[test]
fn forged_sender_fixtures_v3_v4_v5_are_all_rejected() {
    for fixture in [
        ForgedSenderFixture::V3PinnedSender,
        ForgedSenderFixture::V4RatchetSenderAd,
        ForgedSenderFixture::V5SenderKeyAd,
    ] {
        fixture.assert_rejected();
    }
}

impl ForgedSenderFixture {
    fn assert_rejected(self) {
        match self {
            Self::V3PinnedSender => assert_v3_pinned_sender_rejects_forgery(),
            Self::V4RatchetSenderAd => assert_v4_sender_context_rejects_forgery(),
            Self::V5SenderKeyAd => assert_v5_sender_context_rejects_forgery(),
        }
    }
}

fn assert_v3_pinned_sender_rejects_forgery() {
    let bob = keystore::generate_identity("a99-v3-bob".to_string());
    let alice = keystore::generate_identity("a99-v3-alice".to_string());
    let mallory = keystore::generate_identity("a99-v3-mallory".to_string());
    assert_ne!(alice.x25519_public, mallory.x25519_public);

    let wire = encrypt_v3(
        &mallory.x25519_secret,
        &mallory.x25519_public,
        &[RecipientV3 {
            x25519_pub: bob.x25519_public,
            mlkem_pub: bob.mlkem_encapsulation_key(),
        }],
        MSG_TYPE_CONTENT,
        b"v3 forged sender must fail",
    )
    .expect("syntactically valid v3 fixture");

    let err = decrypt_v3_for_sender(
        &wire,
        &bob.x25519_secret,
        &bob.mlkem_decapsulation_key(),
        &alice.x25519_public,
    )
    .expect_err("v3 wire sender must match the pinned sender key");
    assert!(
        matches!(err, V2Error::SenderIdentityMismatch),
        "expected v3 sender identity mismatch, got {err:?}"
    );

    let opened = decrypt_v3_for_sender(
        &wire,
        &bob.x25519_secret,
        &bob.mlkem_decapsulation_key(),
        &mallory.x25519_public,
    )
    .expect("same v3 fixture must open when attributed to its actual sender");
    assert_eq!(opened.plaintext, b"v3 forged sender must fail");
}

struct V4Setup {
    alice: DoubleRatchet,
    bob: DoubleRatchet,
    alice_ik_sk: x25519::SecretKey,
    alice_ik_pub: x25519::PublicKey,
    bob_ik_sk: x25519::SecretKey,
    bob_ik_pub: x25519::PublicKey,
    bob_sk: SessionKey,
    bob_spk_sk: x25519::SecretKey,
    bob_recipient: RecipientV3,
    bob_mlkem_dk: ml_kem_768::DecapsulationKey,
}

fn v4_setup() -> V4Setup {
    let (alice_ik_sk, alice_ik_pub) = x25519::generate_keypair();
    let (bob_ik_sk, bob_ik_pub) = x25519::generate_keypair();
    let (bob_spk_sk, bob_spk_pub) = x25519::generate_keypair();
    let (bob_mlkem_dk, bob_mlkem_ek) = ml_kem_768::generate_keypair();
    let (alice_sk, handshake) =
        pqxdh::initiate(&alice_ik_sk, &bob_ik_pub, &bob_spk_pub, None, &bob_mlkem_ek)
            .expect("alice initiates v4 DR");
    let bob_sk = pqxdh::respond(
        &bob_ik_sk,
        &bob_spk_sk,
        None,
        &bob_mlkem_dk,
        &alice_ik_pub,
        &handshake,
    )
    .expect("bob responds to v4 DR");
    let alice_ctx = SessionContext {
        local_ik_x25519_pub: alice_ik_pub,
        local_ik_mlkem_pub: vec![0xa9; 1184],
        peer_ik_x25519_pub: bob_ik_pub,
        peer_ik_mlkem_pub: vec![0x99; 1184],
        conversation_id: b"a99-v4-regression".to_vec(),
        session_version: SESSION_VERSION_V1,
    };
    let bob_ctx = SessionContext {
        local_ik_x25519_pub: bob_ik_pub,
        local_ik_mlkem_pub: vec![0x99; 1184],
        peer_ik_x25519_pub: alice_ik_pub,
        peer_ik_mlkem_pub: vec![0xa9; 1184],
        conversation_id: b"a99-v4-regression".to_vec(),
        session_version: SESSION_VERSION_V1,
    };

    V4Setup {
        alice: DoubleRatchet::new_initiator(&alice_sk, &bob_spk_pub, alice_ctx)
            .expect("alice v4 ratchet"),
        bob: DoubleRatchet::new_responder(&bob_sk, &bob_spk_sk, bob_ctx).expect("bob v4 ratchet"),
        alice_ik_sk,
        alice_ik_pub,
        bob_ik_sk,
        bob_ik_pub,
        bob_sk,
        bob_spk_sk,
        bob_recipient: RecipientV3 {
            x25519_pub: bob_ik_pub,
            mlkem_pub: ml_kem_768::EncapsulationKey::from_bytes(&bob_mlkem_ek.to_bytes()),
        },
        bob_mlkem_dk,
    }
}

fn assert_v4_sender_context_rejects_forgery() {
    let mut setup = v4_setup();
    let (_, forged_sender_pub) = x25519::generate_keypair();
    assert_ne!(forged_sender_pub, setup.alice_ik_pub);

    let encrypted = setup
        .alice
        .encrypt(b"v4 forged sender must fail")
        .expect("v4 ratchet encrypts");
    let (wrap_key, handshake) = pqxdh::initiate(
        &setup.alice_ik_sk,
        &setup.bob_recipient.x25519_pub,
        &setup.bob_recipient.x25519_pub,
        None,
        &setup.bob_recipient.mlkem_pub,
    )
    .expect("v4 wrap key");
    let wire = encrypt_v4_from_ratchet(
        &setup.alice_ik_pub,
        &setup.bob_recipient,
        &wrap_key,
        &handshake,
        MSG_TYPE_CONTENT,
        true,
        &encrypted,
    )
    .expect("v4 wire encodes");
    let parsed = decrypt_v4(&wire, &setup.bob_ik_sk, &setup.bob_mlkem_dk).expect("v4 wire opens");
    assert_eq!(parsed.sender_ik_pub, setup.alice_ik_pub);

    let recovered = crypto::ratchet::EncryptedMessage {
        header_nonce: parsed.enc_header_nonce,
        enc_header: parsed.enc_header,
        message_nonce: parsed.body_nonce,
        ciphertext: parsed.body_ct,
    };
    let forged_ctx = SessionContext {
        local_ik_x25519_pub: setup.bob_ik_pub,
        local_ik_mlkem_pub: vec![0x99; 1184],
        peer_ik_x25519_pub: forged_sender_pub,
        peer_ik_mlkem_pub: vec![0xa9; 1184],
        conversation_id: b"a99-v4-regression".to_vec(),
        session_version: SESSION_VERSION_V1,
    };
    let mut bob_keyed_to_forged_sender =
        DoubleRatchet::new_responder(&setup.bob_sk, &setup.bob_spk_sk, forged_ctx)
            .expect("forged v4 receiver ratchet constructs");
    let err = bob_keyed_to_forged_sender
        .decrypt(&recovered)
        .expect_err("v4 sender identity AD must reject a forged sender");
    assert!(
        matches!(err, crypto::Error::AeadFailure),
        "expected v4 AEAD failure from forged sender AD, got {err:?}"
    );
    assert_eq!(
        setup
            .bob
            .decrypt(&recovered)
            .expect("honest v4 sender opens"),
        b"v4 forged sender must fail"
    );
}

fn assert_v5_sender_context_rejects_forgery() {
    let real_sender = keystore::generate_identity("a99-v5-real-sender".to_string());
    let forged_sender = keystore::generate_identity("a99-v5-forged-sender".to_string());
    assert_ne!(real_sender.x25519_public, forged_sender.x25519_public);

    let mut sender_state = SenderKeyState::new();
    sender_state
        .install_sender()
        .expect("v5 sender chain installs");
    let (chain_id, rotation_root, physical_device_id) = {
        let chain = sender_state.sender_chain().expect("v5 sender chain exists");
        (
            chain.current_chain_id(),
            chain.rotation_root_bytes(),
            chain.physical_device_id(),
        )
    };

    let mut honest_receiver = SenderKeyState::new();
    honest_receiver
        .install_receiver(
            real_sender.x25519_public.as_bytes().to_vec(),
            chain_id,
            &rotation_root,
            physical_device_id,
        )
        .expect("honest v5 receiver chain installs");
    let mut forged_receiver = SenderKeyState::new();
    forged_receiver
        .install_receiver(
            forged_sender.x25519_public.as_bytes().to_vec(),
            chain_id,
            &rotation_root,
            physical_device_id,
        )
        .expect("forged v5 receiver chain installs");

    let group_id = b"a99-v5-regression-group".to_vec();
    let honest_ctx = SenderContext {
        sender_ik_x25519_pub: real_sender.x25519_public,
        sender_ik_mlkem_pub: real_sender.mlkem_public_bytes.to_vec(),
        group_id: group_id.clone(),
        session_version: crypto::sender_keys::SESSION_VERSION_V1,
    };
    let encrypted = sender_state
        .encrypt(b"v5 forged sender must fail", &honest_ctx)
        .expect("v5 sender-key encryption succeeds");
    let wire = encrypt_v5(&real_sender.x25519_public, MSG_TYPE_CONTENT, 0, &encrypted)
        .expect("v5 wire encodes");
    let parsed = decrypt_v5(&wire).expect("v5 wire parses");
    assert_eq!(parsed.sender_ik_pub, real_sender.x25519_public);

    let recovered = SenderKeyMessage {
        header_nonce: parsed.header_nonce,
        enc_header: parsed.enc_header,
        message_nonce: parsed.message_nonce,
        ciphertext: parsed.ciphertext,
    };
    let forged_ctx = SenderContext {
        sender_ik_x25519_pub: forged_sender.x25519_public,
        sender_ik_mlkem_pub: real_sender.mlkem_public_bytes.to_vec(),
        group_id,
        session_version: crypto::sender_keys::SESSION_VERSION_V1,
    };
    let err = forged_receiver
        .decrypt_from(
            forged_sender.x25519_public.as_bytes(),
            &recovered,
            &forged_ctx,
        )
        .expect_err("v5 sender-key AD must reject a forged sender");
    assert!(
        matches!(err, crypto::Error::AeadFailure),
        "expected v5 AEAD failure from forged sender AD, got {err:?}"
    );
    assert_eq!(
        honest_receiver
            .decrypt_from(
                real_sender.x25519_public.as_bytes(),
                &recovered,
                &honest_ctx
            )
            .expect("honest v5 sender opens"),
        b"v5 forged sender must fail"
    );
}
