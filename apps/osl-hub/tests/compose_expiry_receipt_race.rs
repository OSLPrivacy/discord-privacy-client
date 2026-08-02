//! T2-71 / TF-71: expiry may win the race with an authenticated receipt.
//!
//! The payload has already been removed when the receipt arrives.  Its
//! tombstone remains addressable for receipt accounting, but the receipt must
//! not move the terminal expiry time or restore readable content.

#![cfg(feature = "core")]

#[path = "../src/expiry_receipt_race.rs"]
mod expiry_receipt_race;

use expiry_receipt_race::{record_late_receipt, LateReceiptDisposition, LateReceiptKind};
use ipc::tombstone_file::TombstoneFile;
use message_lifecycle::{AckOutcome, DestructReason, MessageDirection, Tombstone};

fn expired_message() -> Tombstone {
    Tombstone {
        message_id: [0x11; 32],
        peer_id: [0x22; 32],
        conversation_id: [0x33; 32],
        direction: MessageDirection::Outgoing,
        created_at: 100,
        destroyed_at: 200,
        reason: DestructReason::Expired,
        delivered_at: None,
        opened_at: None,
        destruction_ack: AckOutcome::Destroyed,
    }
}

#[test]
fn tf_71_expiry_wins_while_a_receipt_is_in_flight() {
    let mut tombstones = TombstoneFile::default();
    tombstones.record(expired_message());

    assert_eq!(
        record_late_receipt(
            &mut tombstones,
            [0x11; 32],
            [0x22; 32],
            [0x33; 32],
            LateReceiptKind::Delivered,
            150,
        ),
        LateReceiptDisposition::Recorded
    );

    let tombstone = &tombstones.entries[0];
    assert_eq!(tombstone.delivered_at, Some(150));
    assert_eq!(tombstone.reason, DestructReason::Expired);
    assert_eq!(tombstone.destroyed_at, 200);
    assert!(tombstones.is_message_destroyed(&[0x11; 32]));

    assert_eq!(
        record_late_receipt(
            &mut tombstones,
            [0x11; 32],
            [0x22; 32],
            [0x33; 32],
            LateReceiptKind::Delivered,
            250,
        ),
        LateReceiptDisposition::AlreadyRecorded
    );
    assert_eq!(tombstones.entries[0].delivered_at, Some(150));
    assert_eq!(tombstones.entries[0].destroyed_at, 200);
}
