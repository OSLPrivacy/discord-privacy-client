use crypto::{ed25519, ml_kem_768, x25519};
use ipc::wire_v2::{decrypt_v3_for_sender, encrypt_v3, RecipientV3, V2Error, MSG_TYPE_CONTENT};
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
