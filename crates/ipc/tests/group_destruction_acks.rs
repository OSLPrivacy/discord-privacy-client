//! Group destruction acknowledgements are counted per member device.
//!
//! A member can have more than one device, so a group receipt must keep every
//! delivery target distinct.  It may become permanently incomplete when a
//! member abandons one device; that is an honest outcome, not a failure or a
//! success.

use ipc::revocation::{
    RevocationOutbox, RevocationOutboxEntry, STATUS_ACKNOWLEDGED, STATUS_SENT_REQUEST,
};

const GROUP_SCOPE: &str = "gc:design-review";

fn member_device_ack(member: &str, device: &str) -> RevocationOutboxEntry {
    let target = format!("{member}:{device}");
    RevocationOutboxEntry {
        recipient_id: target.clone(),
        scope_id_label: GROUP_SCOPE.to_owned(),
        storage_key: GROUP_SCOPE.to_owned(),
        burn_id_hex: format!("burn-{target}"),
        collapse_key_hex: format!("lane-{target}"),
        burn_epoch: 1,
        burn_upto_seq: 7,
        notice_b64: "AAAA".to_owned(),
        attempts: 0,
        next_attempt_at: 0,
        acknowledged: false,
        created_at: 0,
    }
}

#[test]
fn group_destruction_acknowledgement_counts_every_member_device() {
    let mut outbox = RevocationOutbox::default();
    let targets = [
        ("alice", "phone"),
        ("alice", "laptop"),
        ("bob", "phone"),
        ("carol", "abandoned-tablet"),
    ];

    for (member, device) in targets {
        outbox.enqueue(member_device_ack(member, device)).unwrap();
    }

    for target in ["alice:phone", "alice:laptop", "bob:phone"] {
        outbox.record_acknowledged(&format!("burn-{target}"));
    }
    outbox.record_attempt("burn-carol:abandoned-tablet", 0);

    assert_eq!(outbox.acknowledged_for_scope(GROUP_SCOPE), 3);
    assert_eq!(outbox.pending_for_scope(GROUP_SCOPE), 1);
    assert_eq!(
        outbox.status_for_scope(GROUP_SCOPE),
        STATUS_SENT_REQUEST,
        "one unconfirmed member-device must keep a 3 of 4 group receipt from becoming success"
    );

    outbox.record_acknowledged("burn-carol:abandoned-tablet");
    assert_eq!(outbox.acknowledged_for_scope(GROUP_SCOPE), 4);
    assert_eq!(outbox.pending_for_scope(GROUP_SCOPE), 0);
    assert_eq!(outbox.status_for_scope(GROUP_SCOPE), STATUS_ACKNOWLEDGED);
}
