//! T19-T25: a capability change does not mutate an existing v=3 conversation.
//!
//! OSL-RN is a separate, explicitly bootstrapped session.  Seeing bit 1 makes
//! a peer eligible for that bootstrap, but it must not turn an already-issued
//! v=3 ciphertext into a `0x10` message or make its in-flight v=3 history
//! unreadable.

use base64::Engine as _;
use ipc::wire_rn::{
    select_wire_version, send_rn_with_sealer, RnPeerPin, RnPolicy, RnSessionStore,
    SelectedVersion,
};
use ipc::wire_v2::{decrypt_v3_for_sender, encrypt_v3, RecipientV3, MSG_TYPE_CONTENT};
use keystore::client::{PeerCapabilities, RN_CAP_WIRE_RN, RN_CAP_WIRE_RN_LIVE};
use keystore::{generate_identity, MemorySealer};

#[test]
fn v3_history_stays_v3_and_decryptable_until_a_fresh_rn_bootstrap() {
    let alice = generate_identity("t19-e4-alice".to_owned());
    let bob = generate_identity("t19-e4-bob".to_owned());
    let legacy_plaintext = b"in-flight v3 must remain decryptable";
    let legacy_wire = encrypt_v3(
        &alice.x25519_secret,
        &alice.x25519_public,
        &[RecipientV3 {
            x25519_pub: bob.x25519_public,
            mlkem_pub: bob.mlkem_encapsulation_key(),
        }],
        MSG_TYPE_CONTENT,
        legacy_plaintext,
    )
    .expect("create v3 history before either peer advertises RN live support");

    let raw = base64::engine::general_purpose::STANDARD
        .decode(legacy_wire.strip_prefix("DPC0::").expect("OSL prefix"))
        .expect("v3 base64");
    assert_eq!(raw.first(), Some(&3), "fixture must be a v3 wire");

    let live_caps = PeerCapabilities::Verified(RN_CAP_WIRE_RN | RN_CAP_WIRE_RN_LIVE);
    assert!(
        matches!(
            select_wire_version(&RnPeerPin::UNKNOWN, live_caps, RnPolicy::Opportunistic),
            Ok(SelectedVersion::Rn)
        ),
        "bit 1 makes a *fresh* RN bootstrap eligible"
    );

    let session_dir = tempfile::tempdir().expect("session tempdir");
    let store = RnSessionStore::new(session_dir.path().join("rn"));
    let sealer = MemorySealer::new();
    let rn_attempt = send_rn_with_sealer(
        &store,
        &sealer,
        bob.x25519_public.as_bytes(),
        MSG_TYPE_CONTENT,
        b"must not be emitted into v3 history",
    );
    assert!(
        rn_attempt.is_err(),
        "capability observation alone must not emit 0x10 without an explicit fresh bootstrap"
    );
    assert!(
        store
            .load_session_with_sealer(bob.x25519_public.as_bytes(), &sealer)
            .expect("inspect empty RN store")
            .is_none(),
        "a v3 conversation must not acquire RN state merely because capabilities changed"
    );

    let opened = decrypt_v3_for_sender(
        &legacy_wire,
        &bob.x25519_secret,
        &bob.mlkem_decapsulation_key(),
        &alice.x25519_public,
    )
    .expect("in-flight v3 remains decryptable after both peers advertise bit 1");
    assert_eq!(opened.plaintext, legacy_plaintext);
}
