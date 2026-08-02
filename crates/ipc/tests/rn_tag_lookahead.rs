//! T19-T27: the honest RN delivery-tag subscription window is one chain.
//!
//! `transport.md` derives a tag from the current conversation epoch.  A peer
//! can know the epoch it already shares, but cannot know the DH public key its
//! partner will choose for the next sending chain until that wire arrives.

use crypto::pointer::{derive_capabilities, Pointer};
use osl_ratchet_next::{
    test_support::established_pair_with_params, Session, SessionParams,
};

const LOOKAHEAD_CHAINS: usize = 1;
const POINTER: Pointer = Pointer::from_bytes([0x27; 20]);

fn current_epoch_secret(session: &Session) -> [u8; 32] {
    // `export_state` is the sealed-at-rest representation used by the IPC
    // store. Its fixed prefix is format version, role, session id, then root.
    // Do not log or persist this test-only copy of the secret.
    let state = session.export_state().expect("export live session state");
    state[18..50]
        .try_into()
        .expect("state export has the fixed root-key prefix")
}

fn delivery_tag(session: &Session) -> [u8; 16] {
    let epoch = current_epoch_secret(session);
    derive_capabilities(&POINTER, &epoch, &epoch, &epoch)
        .expect("derive transport tag from current epoch")
        .delivery_tag
}

#[test]
fn receiver_can_honestly_subscribe_to_one_current_chain_not_across_an_unseen_dh_step() {
    let (mut alice, mut bob, mut rng) =
        established_pair_with_params(0x19_e6, SessionParams::default());

    // Bob's one honest pre-arrival subscription is for the epoch/chain he
    // currently shares with Alice.
    let current_tag = delivery_tag(&bob);
    assert_eq!(LOOKAHEAD_CHAINS, 1, "T1 OQ-6 answer: one chain");

    // Alice receives Bob's reply. That ratchets Alice's next sending chain
    // with a freshly generated DH public key Bob cannot know yet.
    let reply = bob.encrypt(0, b"reply", &mut rng).expect("bob reply");
    alice.decrypt(&reply, &mut rng).expect("alice receives reply");
    let future = alice
        .encrypt(0, b"first message on Alice's new DH chain", &mut rng)
        .expect("alice sends next chain");

    // Before the wire arrives, Bob still has only the old epoch tag.
    assert_eq!(delivery_tag(&bob), current_tag);
    bob.decrypt(&future, &mut rng)
        .expect("live receiver accepts Alice's unseen-DH message");
    let arrived_chain_tag = delivery_tag(&bob);
    assert_ne!(
        arrived_chain_tag, current_tag,
        "the next chain's delivery tag cannot be precomputed before its DH step arrives"
    );
}
