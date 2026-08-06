use ipc::wire_rn::{
    initiate_and_persist_with_sealer, DirectChatRnAgreement, RnError, RnPeerPin, RnSessionStore,
    RN_CONTEXT_DISCORD_MANUAL,
};
use keystore::client::{
    PeerCapabilities, CLIENT_RN_CAPABILITY_FLOOR, RN_CAP_WIRE_RN, RN_CAP_WIRE_RN_LIVE,
};
use keystore::MemorySealer;
use osl_ratchet_next::primitives::x25519_keypair;
use osl_ratchet_next::test_support::{fresh_bundle, seeded_rng};
use osl_ratchet_next::SessionParams;

fn live_caps() -> PeerCapabilities {
    PeerCapabilities::Verified(RN_CAP_WIRE_RN | RN_CAP_WIRE_RN_LIVE)
}

fn local_caps() -> PeerCapabilities {
    PeerCapabilities::Verified(CLIENT_RN_CAPABILITY_FLOOR)
}

#[test]
fn two_supporting_direct_chat_fixtures_agree_and_one_unsupported_fixture_is_refused() {
    let mut rng = seeded_rng(0x0427);
    let sealer = MemorySealer::new();
    let mut agreed = 0usize;

    for label in ["supporting-fixture-alice", "supporting-fixture-bob"] {
        let dir = tempfile::tempdir().expect("isolated RN store");
        let store = RnSessionStore::new(dir.path().join("rn"));
        let (_peer_prekeys, peer_bundle) = fresh_bundle(&mut rng);
        let peer_identity = *peer_bundle.identity.as_bytes();
        let peer_mlkem = peer_bundle.pq_prekey.to_bytes();
        let (own_secret, own_public) = x25519_keypair(&mut rng);

        let agreement = DirectChatRnAgreement::prove(local_caps(), live_caps())
            .expect("both direct-chat peers prove live RN support");
        assert!(agreement.local.supports_rn_live());
        assert!(agreement.peer.supports_rn_live());

        initiate_and_persist_with_sealer(
            &store,
            &sealer,
            &own_secret,
            own_public.as_bytes(),
            &peer_bundle,
            agreement.peer,
            &peer_mlkem,
            RN_CONTEXT_DISCORD_MANUAL,
            SessionParams::default(),
        )
        .unwrap_or_else(|e| panic!("{label} should start the stronger key sequence: {e}"));

        assert!(
            store
                .load_session_with_sealer(&peer_identity, &sealer)
                .expect("load direct-chat RN session")
                .is_some(),
            "{label} must persist the stronger key sequence"
        );
        assert!(
            store
                .load_pin(&peer_identity)
                .expect("load direct-chat RN pin")
                .is_pinned_to_rn(),
            "{label} must pin the direct chat to RN"
        );
        agreed += 1;
    }

    let dir = tempfile::tempdir().expect("isolated RN store");
    let store = RnSessionStore::new(dir.path().join("rn"));
    let (_peer_prekeys, peer_bundle) = fresh_bundle(&mut rng);
    let peer_identity = *peer_bundle.identity.as_bytes();
    let peer_mlkem = peer_bundle.pq_prekey.to_bytes();
    let (own_secret, own_public) = x25519_keypair(&mut rng);
    let unsupported_peer = PeerCapabilities::Verified(RN_CAP_WIRE_RN);

    let refused = match initiate_and_persist_with_sealer(
        &store,
        &sealer,
        &own_secret,
        own_public.as_bytes(),
        &peer_bundle,
        unsupported_peer,
        &peer_mlkem,
        RN_CONTEXT_DISCORD_MANUAL,
        SessionParams::default(),
    ) {
        Ok(_) => panic!("a peer without live RN proof must not start the stronger key sequence"),
        Err(e) => e,
    };

    assert!(matches!(refused, RnError::DirectChatAgreementUnsupported));
    assert_eq!(
        store
            .load_pin(&peer_identity)
            .expect("load unsupported direct-chat pin"),
        RnPeerPin::UNKNOWN,
        "unsupported fixture must not pin the peer to RN"
    );
    assert!(
        store
            .load_session_with_sealer(&peer_identity, &sealer)
            .expect("load unsupported direct-chat session")
            .is_none(),
        "unsupported fixture must not persist an RN session"
    );

    println!("TASK 0427 supporting fixtures agreed={agreed}");
    println!("TASK 0427 unsupported fixtures refused=1 reason={refused}");
}
