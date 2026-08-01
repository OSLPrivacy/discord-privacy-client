//! A group member's local clock may observe expiry before another member sees
//! a burn.  The terminal outcome must be a join, not whichever event arrived
//! first: burn has the stable, higher-severity explanation on every device.

use message_lifecycle::{LifecycleLimits, LogicalMessageLifecycle, ReceiptStatus};

const GROUP_MESSAGE_ID: [u8; 32] = [0x18; 32];
const GROUP_SCOPE_DIGEST: [u8; 32] = [0xd4; 32];
const EXPIRY_AT: u64 = 10_000;

fn limits() -> LifecycleLimits {
    LifecycleLimits {
        max_parts: 4,
        max_part_bytes: 1_024,
        max_total_bytes: 4_096,
    }
}

fn received_group_copy() -> LogicalMessageLifecycle {
    LogicalMessageLifecycle::prepare(
        GROUP_MESSAGE_ID,
        GROUP_SCOPE_DIGEST,
        1,
        EXPIRY_AT - 60,
        EXPIRY_AT,
        limits(),
    )
    .expect("the group message fixture is valid")
}

fn displayed_terminal_reason(status: ReceiptStatus) -> &'static str {
    match status {
        ReceiptStatus::Burned => "Burn",
        ReceiptStatus::Expired => "Expired",
        ReceiptStatus::Failed => "Evicted",
        other => panic!("non-terminal state cannot have a destruction reason: {other:?}"),
    }
}

#[test]
fn skewed_group_members_converge_on_burn_instead_of_arrival_order() {
    // Each tuple is (member name, whether this local clock sees expiry before
    // receiving the burn).  The clocks deliberately span the deadline.
    let schedules = [
        ("alice", true),
        ("bob", false),
        ("carol", true),
        ("dana", false),
        ("erin", true),
    ];

    let outcomes = schedules.map(|(_member, expiry_arrives_first)| {
        let mut copy = received_group_copy();
        if expiry_arrives_first {
            copy.expire(EXPIRY_AT)
                .expect("a fast local clock observes expiry first");
            copy.burn()
                .expect("burn supersedes an already-expired local copy");
        } else {
            copy.burn().expect("burn applies before local expiry");
            assert!(
                copy.expire(EXPIRY_AT).is_err(),
                "expiry cannot replace a stronger terminal outcome"
            );
        }

        (copy.status(), displayed_terminal_reason(copy.status()))
    });

    for (terminal_state, reason) in outcomes {
        assert_eq!(terminal_state, ReceiptStatus::Burned);
        assert_eq!(reason, "Burn");
    }
}
