//! T19-T12: an unaccepted TOFU key change drops the old RN session once per
//! observed replacement bundle, not once per send.

use ipc::rn_health::{tofu_bundle_digest, tofu_change_needs_session_reset};
use ipc::tofu::KeyBundle;

fn bundle(x25519: &str) -> KeyBundle {
    KeyBundle {
        ed25519_pub: "ed25519".to_owned(),
        x25519_pub: x25519.to_owned(),
        mlkem768_pub: "mlkem768".to_owned(),
        ratchet_initial_pub: Some("ratchet-initial".to_owned()),
    }
}

#[test]
fn unaccepted_changed_bundle_resets_once_across_fifty_sends() {
    let changed = bundle("rotated-x25519");
    let digest = tofu_bundle_digest(&changed);
    let mut pending = None;
    let mut session_resets = 0;

    for _ in 0..50 {
        if tofu_change_needs_session_reset(pending.as_ref(), &changed) {
            session_resets += 1;
            pending = Some(changed.clone());
        }
        assert_eq!(tofu_bundle_digest(&changed), digest);
    }

    assert_eq!(session_resets, 1);
}

#[test]
fn another_bundle_digest_gets_its_own_reset() {
    let first = bundle("rotated-x25519-a");
    let second = bundle("rotated-x25519-b");

    assert!(tofu_change_needs_session_reset(None, &first));
    assert!(!tofu_change_needs_session_reset(Some(&first), &first));
    assert!(tofu_change_needs_session_reset(Some(&first), &second));
    assert_ne!(tofu_bundle_digest(&first), tofu_bundle_digest(&second));
}
