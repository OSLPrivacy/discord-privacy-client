use crypto::{ed25519, ml_kem_768, x25519};
use ipc::commands::{decrypt_osl_phase4_cover, encrypt_osl_phase4_to_pubkeys};
use ipc::wire_v2::{
    decrypt_v2, decrypt_v3_for_sender, encrypt_v2, encrypt_v3, RecipientV3, V2Error,
    MSG_TYPE_CONTENT,
};
use keystore::identity_bundle::{BundleVerifyError, BundleVerifyPolicy, IdentityBundle};
use keystore::{AccountOwnershipError, ProofChallenge, PROOF_CHALLENGE_NONCE_BYTES};

fn signed_bundle(
    signer_secret: &ed25519::SecretKey,
    field_source_pub: &ed25519::PublicKey,
    x25519_identity_pub: &x25519::PublicKey,
    mlkem768_identity_pub: &ml_kem_768::EncapsulationKey,
    revision: u64,
) -> IdentityBundle {
    let mut bundle = IdentityBundle {
        ed25519_identity_pub: *field_source_pub.as_bytes(),
        x25519_identity_pub: *x25519_identity_pub.as_bytes(),
        mlkem768_identity_pub: mlkem768_identity_pub.to_bytes(),
        capability_bundle: 1,
        revision,
        signature: [0u8; ed25519::SIGNATURE_SIZE],
    };
    let signature = ed25519::sign(signer_secret, &bundle.signed_bytes());
    bundle.signature = *signature.as_bytes();
    bundle
}

#[derive(Clone, Copy)]
enum ForgedSenderFixture {
    LegacyV1,
    WireV2,
}

impl ForgedSenderFixture {
    fn label(self) -> &'static str {
        match self {
            Self::LegacyV1 => "legacy v1",
            Self::WireV2 => "wire v2",
        }
    }
}

#[test]
fn v1_v2_forged_sender_matrix() {
    for fixture in [ForgedSenderFixture::LegacyV1, ForgedSenderFixture::WireV2] {
        let (honest_sender_secret, honest_sender_pub) = x25519::generate_keypair();
        let (_, forged_sender_pub) = x25519::generate_keypair();
        let (recipient_secret, recipient_pub) = x25519::generate_keypair();
        let plaintext = format!("{} sender attribution proof", fixture.label());

        assert!(
            honest_sender_pub != forged_sender_pub,
            "{} fixture needs distinct sender keys",
            fixture.label()
        );

        match fixture {
            ForgedSenderFixture::LegacyV1 => {
                let wire = encrypt_osl_phase4_to_pubkeys(
                    &honest_sender_secret,
                    &[recipient_pub],
                    &plaintext,
                )
                .expect("legacy v1 fixture should encrypt");

                let opened = decrypt_osl_phase4_cover(&recipient_secret, &honest_sender_pub, &wire)
                    .expect("legacy v1 honest sender pin should decrypt");
                assert_eq!(opened.as_slice(), plaintext.as_bytes());

                let forged = decrypt_osl_phase4_cover(&recipient_secret, &forged_sender_pub, &wire);
                assert!(
                    forged.is_err(),
                    "legacy v1 must refuse a wire attributed to a forged sender key"
                );
            }
            ForgedSenderFixture::WireV2 => {
                let wire = encrypt_v2(
                    plaintext.as_bytes(),
                    &[recipient_pub],
                    MSG_TYPE_CONTENT,
                    &honest_sender_secret,
                )
                .expect("wire v2 fixture should encrypt");

                let opened = decrypt_v2(&wire, &recipient_secret, &honest_sender_pub)
                    .expect("wire v2 honest sender pin should decrypt");
                assert_eq!(opened.msg_type, MSG_TYPE_CONTENT);
                assert_eq!(opened.plaintext.as_slice(), plaintext.as_bytes());

                let forged = decrypt_v2(&wire, &recipient_secret, &forged_sender_pub);
                match forged {
                    Err(V2Error::NoMatchingSlot) => {}
                    Err(error) => panic!(
                        "wire v2 forged sender should fail at sender-bound slot unwrap, got {error:?}"
                    ),
                    Ok(opened) => panic!(
                        "wire v2 must refuse a wire attributed to a forged sender key, opened {} bytes",
                        opened.plaintext.len()
                    ),
                }
            }
        }
    }
}

#[test]
fn full_a5_existing_ceremony_and_sender_attribution_proof() {
    let (owner_ed_secret, owner_ed_pub) = ed25519::generate_keypair();
    let (attacker_ed_secret, attacker_ed_pub) = ed25519::generate_keypair();
    let (owner_x_secret, owner_x_pub) = x25519::generate_keypair();
    let (_attacker_x_secret, attacker_x_pub) = x25519::generate_keypair();
    let (recipient_x_secret, recipient_x_pub) = x25519::generate_keypair();
    let (recipient_mlkem_secret, recipient_mlkem_pub) = ml_kem_768::generate_keypair();

    let policy = BundleVerifyPolicy::new();
    let owner_bundle = signed_bundle(
        &owner_ed_secret,
        &owner_ed_pub,
        &owner_x_pub,
        &recipient_mlkem_pub,
        1,
    );
    assert_eq!(policy.verify(&owner_bundle, &owner_ed_pub, None), Ok(1));

    let substitute_bundle = signed_bundle(
        &attacker_ed_secret,
        &attacker_ed_pub,
        &attacker_x_pub,
        &recipient_mlkem_pub,
        2,
    );
    assert_eq!(
        policy.verify(&substitute_bundle, &owner_ed_pub, Some(1)),
        Err(BundleVerifyError::SignatureInvalid),
        "a self-consistent substitute bundle must not satisfy the pinned ceremony"
    );

    let wire = encrypt_v3(
        &owner_x_secret,
        &owner_x_pub,
        &[RecipientV3 {
            x25519_pub: recipient_x_pub,
            mlkem_pub: recipient_mlkem_pub,
        }],
        MSG_TYPE_CONTENT,
        b"attributed plaintext",
    )
    .unwrap();
    let opened = decrypt_v3_for_sender(
        &wire,
        &recipient_x_secret,
        &recipient_mlkem_secret,
        &owner_x_pub,
    )
    .unwrap();
    assert_eq!(opened.plaintext, b"attributed plaintext");

    let wrong_pin = decrypt_v3_for_sender(
        &wire,
        &recipient_x_secret,
        &recipient_mlkem_secret,
        &attacker_x_pub,
    );
    assert!(
        matches!(wrong_pin, Err(V2Error::SenderIdentityMismatch)),
        "plaintext must not be attributed to a caller-supplied different sender"
    );

    let mut challenge = ProofChallenge::new(
        [0x42; PROOF_CHALLENGE_NONCE_BYTES],
        "platform-account-a",
        "owner-user-a",
        1_000,
        1_060,
    )
    .unwrap();
    assert!(challenge.binds("platform-account-a", "owner-user-a"));
    assert!(!challenge.binds("platform-account-b", "owner-user-a"));
    assert!(!challenge.binds("platform-account-a", "owner-user-b"));
    assert!(!challenge.is_expired(1_059));
    assert!(challenge.is_expired(1_060));
    assert!(challenge.spend());
    assert!(
        !challenge.spend(),
        "a spent account proof challenge must refuse replay"
    );

    let stale = AccountOwnershipError::ProofStale;
    let different_owner = AccountOwnershipError::ProofForDifferentOwner;
    assert_eq!(stale.to_string(), "ownership proof has expired");
    assert_eq!(
        format!("{different_owner:?}"),
        "AccountOwnershipError::ProofForDifferentOwner"
    );
    for rendered in [
        format!("{challenge:?}"),
        challenge.to_string(),
        stale.to_string(),
        format!("{different_owner:?}"),
    ] {
        assert!(!rendered.contains("platform-account-a"));
        assert!(!rendered.contains("owner-user-a"));
    }
}
