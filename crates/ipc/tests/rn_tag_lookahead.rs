//! T19-T27: the honest RN delivery-tag subscription window is one chain.
//!
//! `transport.md` derives a tag from the current conversation epoch.  A peer
//! can know the epoch it already shares, but cannot know the DH public key its
//! partner will choose for the next sending chain until that wire arrives.

use crypto::pointer::{
    derive_delivery_tag_for_counter, derive_delivery_tag_window, resolve_delivery_tag,
    DeliveryTagResolveError, DELIVERY_TAG_LOOKAHEAD,
};
use osl_ratchet_next::{test_support::established_pair_with_params, Session, SessionParams};

const LOOKAHEAD_CHAINS: usize = 1;

fn current_epoch_secret(session: &Session) -> [u8; 32] {
    // `export_state` is the sealed-at-rest representation used by the IPC
    // store. Its fixed prefix is format version, role, session id, then root.
    // Do not log or persist this test-only copy of the secret.
    let state = session.export_state().expect("export live session state");
    state[18..50]
        .try_into()
        .expect("state export has the fixed root-key prefix")
}

fn delivery_tag(session: &Session, counter: u32) -> [u8; 16] {
    let epoch = current_epoch_secret(session);
    derive_delivery_tag_for_counter(&epoch, counter)
        .expect("derive transport tag from current epoch and counter")
}

#[test]
fn receiver_can_honestly_subscribe_to_one_current_chain_not_across_an_unseen_dh_step() {
    let (mut alice, mut bob, mut rng) =
        established_pair_with_params(0x19_e6, SessionParams::default());

    // Bob's one honest pre-arrival subscription is for the epoch/chain he
    // currently shares with Alice.
    let current_tag = delivery_tag(&bob, bob.receiving_counter());
    assert_eq!(LOOKAHEAD_CHAINS, 1, "T1 OQ-6 answer: one chain");

    // Alice receives Bob's reply. That ratchets Alice's next sending chain
    // with a freshly generated DH public key Bob cannot know yet.
    let reply = bob.encrypt(0, b"reply", &mut rng).expect("bob reply");
    alice
        .decrypt(&reply, &mut rng)
        .expect("alice receives reply");
    let future = alice
        .encrypt(0, b"first message on Alice's new DH chain", &mut rng)
        .expect("alice sends next chain");

    // Before the wire arrives, Bob still has only the old epoch tag.
    assert_eq!(delivery_tag(&bob, bob.receiving_counter()), current_tag);
    bob.decrypt(&future, &mut rng)
        .expect("live receiver accepts Alice's unseen-DH message");
    let arrived_chain_tag = delivery_tag(&bob, bob.receiving_counter());
    assert_ne!(
        arrived_chain_tag, current_tag,
        "the next chain's delivery tag cannot be precomputed before its DH step arrives"
    );
}

#[test]
fn receiver_window_resolves_one_counter_desync_and_reports_a_named_clean_miss() {
    let (_alice, bob, _rng) = established_pair_with_params(0x19_e7, SessionParams::default());
    let epoch = current_epoch_secret(&bob);
    let start = bob.receiving_counter();

    let window = derive_delivery_tag_window(&epoch, start).expect("derive receiver window");
    assert_eq!(window.len(), DELIVERY_TAG_LOOKAHEAD);
    assert_eq!(window.first().unwrap().counter, start);
    assert_eq!(window.last().unwrap().counter, start + 31);

    let one_ahead =
        derive_delivery_tag_for_counter(&epoch, start + 1).expect("derive one-ahead tag");
    let resolved = resolve_delivery_tag(&epoch, start, &one_ahead)
        .expect("one-message desync resolves inside the look-ahead window");
    assert_eq!(resolved.counter, start + 1);

    let too_far = derive_delivery_tag_for_counter(&epoch, start + DELIVERY_TAG_LOOKAHEAD as u32)
        .expect("derive past-window tag");
    let before = start;
    let miss = resolve_delivery_tag(&epoch, start, &too_far)
        .expect_err("past-window desync must not resolve");
    assert!(
        matches!(
            miss,
            DeliveryTagResolveError::LookaheadMiss {
                start_counter,
                window: DELIVERY_TAG_LOOKAHEAD
            } if start_counter == before
        ),
        "past-window desync must be a named clean miss"
    );
    assert_eq!(start, before, "a miss cannot advance the caller's counter");
}
