//! Regression tests for the periodic SKDM self-heal re-emit contract.

use crypto::sender_keys::{ReceiverChain, SenderChain, SenderContext, SESSION_VERSION_V1};
use crypto::x25519;

fn sender_context() -> SenderContext {
    SenderContext {
        sender_ik_x25519_pub: x25519::PublicKey::from_bytes([0xaa; 32]),
        sender_ik_mlkem_pub: vec![0xaa; 1184],
        group_id: b"idempotence-regression".to_vec(),
        session_version: SESSION_VERSION_V1,
    }
}

fn chain_pair() -> (SenderChain, ReceiverChain) {
    let sender = SenderChain::new().expect("sender chain installs");
    let receiver = ReceiverChain::install(
        sender.current_chain_id(),
        &sender.rotation_root_bytes(),
        sender.physical_device_id(),
    )
    .expect("receiver chain installs");
    (sender, receiver)
}

#[test]
fn reapplying_current_skdm_preserves_progress_and_next_message_decrypts() {
    let (mut sender, mut receiver) = chain_pair();
    let ctx = sender_context();

    let first = sender
        .encrypt(b"first", &ctx)
        .expect("first message encrypts");
    assert_eq!(receiver.decrypt(&first, &ctx).unwrap(), b"first");
    assert_eq!(receiver.current_n(), 1);

    receiver
        .rotate_to(sender.current_chain_id(), &sender.rotation_root_bytes())
        .expect("periodic re-emit of the current SKDM is accepted");
    assert_eq!(
        receiver.current_n(),
        1,
        "re-applying the current SKDM must not rewind receiver progress"
    );

    let second = sender
        .encrypt(b"second", &ctx)
        .expect("second message encrypts");
    assert_eq!(receiver.decrypt(&second, &ctx).unwrap(), b"second");
}

#[test]
fn current_chain_id_with_different_root_cannot_reseed_live_receiver() {
    let (mut sender, mut receiver) = chain_pair();
    let ctx = sender_context();

    let first = sender
        .encrypt(b"first", &ctx)
        .expect("first message encrypts");
    assert_eq!(receiver.decrypt(&first, &ctx).unwrap(), b"first");

    let mut attacker_root = sender.rotation_root_bytes();
    attacker_root[0] ^= 0xff;
    receiver
        .rotate_to(sender.current_chain_id(), &attacker_root)
        .expect("a same-id SKDM is ignored rather than installing its root");
    assert_eq!(
        receiver.current_n(),
        1,
        "a same-id SKDM with another root must not reset receiver progress"
    );

    let second = sender
        .encrypt(b"second", &ctx)
        .expect("second message encrypts");
    assert_eq!(
        receiver.decrypt(&second, &ctx).unwrap(),
        b"second",
        "the authentic live chain must remain installed"
    );
}
