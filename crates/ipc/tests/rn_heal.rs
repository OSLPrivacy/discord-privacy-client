use ipc::wire_rn::{
    accept_and_persist_with_sealer, initiate_and_persist_with_sealer, receive_rn_with_sealer,
    recover_session, send_rn_with_sealer, RnError, RnPolicy, RnSessionStore,
    RN_CONTEXT_DISCORD_MANUAL,
};
use keystore::client::{PeerCapabilities, RN_CAP_WIRE_RN};
use keystore::MemorySealer;
use osl_ratchet_next::{
    peek_wire_version,
    test_support::{fresh_bundle, seeded_identity, seeded_rng},
    SessionParams, WIRE_VERSION_RN,
};

fn assert_rn_wire(wire: &str) {
    assert_eq!(
        peek_wire_version(wire).expect("valid RN wire"),
        WIRE_VERSION_RN,
        "healing must not emit a legacy v=3 wire"
    );
}

#[test]
fn desynchronised_pair_heals_by_rehandshaking_without_lowering_its_pins() {
    let alice_dir = tempfile::tempdir().expect("alice tempdir");
    let bob_dir = tempfile::tempdir().expect("bob tempdir");
    let alice_store = RnSessionStore::new(alice_dir.path().join("rn"));
    let bob_store = RnSessionStore::new(bob_dir.path().join("rn"));
    let sealer = MemorySealer::new();
    let mut rng = seeded_rng(0x19_c3);
    let (bob_prekeys, bob_bundle) = fresh_bundle(&mut rng);
    let alice_identity = seeded_identity(0x_a11ce);
    let alice_public = alice_identity.public();
    let bob_peer = *bob_bundle.identity.as_bytes();
    let alice_peer = *alice_public.as_bytes();
    let bob_ek = bob_bundle.pq_prekey.to_bytes();

    initiate_and_persist_with_sealer(
        &alice_store,
        &sealer,
        &alice_identity,
        alice_public.as_bytes(),
        &bob_bundle,
        PeerCapabilities::Verified(RN_CAP_WIRE_RN),
        &bob_ek,
        RN_CONTEXT_DISCORD_MANUAL,
        SessionParams::default(),
    )
    .expect("initial initiate");
    let bootstrap = send_rn_with_sealer(&alice_store, &sealer, &bob_peer, 1, b"initial")
        .expect("initial bootstrap");
    assert_rn_wire(&bootstrap);
    accept_and_persist_with_sealer(
        &bob_store,
        &sealer,
        &bob_prekeys,
        bob_bundle.identity.as_bytes(),
        &bob_ek,
        &bootstrap,
        RN_CONTEXT_DISCORD_MANUAL,
        SessionParams::default(),
    )
    .expect("initial accept");

    let live_message = send_rn_with_sealer(&bob_store, &sealer, &alice_peer, 2, b"before heal")
        .expect("live message");
    assert_rn_wire(&live_message);
    assert_eq!(
        receive_rn_with_sealer(&alice_store, &sealer, &bob_peer, &live_message)
            .expect("alice receives before heal")
            .plaintext,
        b"before heal"
    );

    recover_session(&alice_store, &bob_peer).expect("recover alice");
    recover_session(&bob_store, &alice_peer).expect("recover bob");
    assert!(alice_store
        .load_pin(&bob_peer)
        .expect("alice pin")
        .is_pinned_to_rn());
    assert!(bob_store
        .load_pin(&alice_peer)
        .expect("bob pin")
        .is_pinned_to_rn());
    assert!(matches!(
        ipc::wire_rn::select_wire_version(
            &alice_store.load_pin(&bob_peer).expect("alice pin"),
            PeerCapabilities::Absent,
            RnPolicy::Opportunistic,
        ),
        Err(RnError::PinnedToRn)
    ));

    initiate_and_persist_with_sealer(
        &alice_store,
        &sealer,
        &alice_identity,
        alice_public.as_bytes(),
        &bob_bundle,
        PeerCapabilities::Verified(RN_CAP_WIRE_RN),
        &bob_ek,
        RN_CONTEXT_DISCORD_MANUAL,
        SessionParams::default(),
    )
    .expect("re-initiate");
    let rebootstrap = send_rn_with_sealer(&alice_store, &sealer, &bob_peer, 3, b"rehandshake")
        .expect("rehandshake bootstrap");
    assert_rn_wire(&rebootstrap);
    accept_and_persist_with_sealer(
        &bob_store,
        &sealer,
        &bob_prekeys,
        bob_bundle.identity.as_bytes(),
        &bob_ek,
        &rebootstrap,
        RN_CONTEXT_DISCORD_MANUAL,
        SessionParams::default(),
    )
    .expect("rehandshake accept");

    let healed_message = send_rn_with_sealer(&bob_store, &sealer, &alice_peer, 4, b"after heal")
        .expect("healed message");
    assert_rn_wire(&healed_message);
    assert_eq!(
        receive_rn_with_sealer(&alice_store, &sealer, &bob_peer, &healed_message)
            .expect("alice receives after heal")
            .plaintext,
        b"after heal"
    );
}
