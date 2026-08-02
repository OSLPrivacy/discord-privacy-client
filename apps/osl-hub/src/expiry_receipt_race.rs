//! Recording receipt facts after their payload has expired.
//!
//! An expiry tombstone is deliberately still receipt-addressable.  A receipt
//! describes an event that already happened; it neither restores a payload nor
//! changes the terminal destruction state.

use ipc::tombstone_file::TombstoneFile;
use message_lifecycle::Tombstone;

/// Receipt facts which can arrive after a message has become terminal.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LateReceiptKind {
    Delivered,
    Opened,
}

/// Result of attempting to attach a receipt fact to an existing tombstone.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LateReceiptDisposition {
    Recorded,
    AlreadyRecorded,
    UnknownMessage,
    WrongPeerOrConversation,
}

/// Attach an authenticated receipt fact to a payload-free tombstone.
///
/// The caller has already verified the receipt signature.  This function binds
/// it to that signer's peer and conversation before changing the tombstone.
/// It never creates a tombstone for an unknown id: doing that would turn a
/// receipt into an existence oracle.  The terminal reason and destruction time
/// are intentionally untouched, so a late receipt cannot restart an expiry
/// clock or make content available again.
pub fn record_late_receipt(
    tombstones: &mut TombstoneFile,
    message_id: [u8; 32],
    peer_id: [u8; 32],
    conversation_id: [u8; 32],
    kind: LateReceiptKind,
    observed_at: u64,
) -> LateReceiptDisposition {
    let Some(tombstone) = tombstones
        .entries
        .iter_mut()
        .find(|entry| entry.message_id == message_id)
    else {
        return LateReceiptDisposition::UnknownMessage;
    };

    if tombstone.peer_id != peer_id || tombstone.conversation_id != conversation_id {
        return LateReceiptDisposition::WrongPeerOrConversation;
    }

    let receipt_time = match kind {
        LateReceiptKind::Delivered => &mut tombstone.delivered_at,
        LateReceiptKind::Opened => &mut tombstone.opened_at,
    };
    match *receipt_time {
        // Keep the earliest authenticated observation.  A retry or a delayed
        // frame is not evidence that the event happened later.
        Some(existing) if existing <= observed_at => LateReceiptDisposition::AlreadyRecorded,
        _ => {
            *receipt_time = Some(observed_at);
            LateReceiptDisposition::Recorded
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use message_lifecycle::{AckOutcome, DestructReason, MessageDirection};

    fn expired_tombstone() -> Tombstone {
        Tombstone {
            message_id: [1; 32],
            peer_id: [2; 32],
            conversation_id: [3; 32],
            direction: MessageDirection::Outgoing,
            created_at: 10,
            destroyed_at: 20,
            reason: DestructReason::Expired,
            delivered_at: None,
            opened_at: None,
            destruction_ack: AckOutcome::Destroyed,
        }
    }

    #[test]
    fn tf_35_late_receipt_is_recorded_without_resurrecting_an_expired_message() {
        let mut tombstones = TombstoneFile::default();
        tombstones.record(expired_tombstone());

        assert_eq!(
            record_late_receipt(
                &mut tombstones,
                [1; 32],
                [2; 32],
                [3; 32],
                LateReceiptKind::Delivered,
                15,
            ),
            LateReceiptDisposition::Recorded
        );

        let tombstone = &tombstones.entries[0];
        assert_eq!(tombstone.delivered_at, Some(15));
        assert_eq!(tombstone.reason, DestructReason::Expired);
        assert_eq!(tombstone.destroyed_at, 20);
        assert!(tombstones.is_message_destroyed(&[1; 32]));
    }

    #[test]
    fn a_late_receipt_cannot_update_another_peers_tombstone() {
        let mut tombstones = TombstoneFile::default();
        tombstones.record(expired_tombstone());

        assert_eq!(
            record_late_receipt(
                &mut tombstones,
                [1; 32],
                [9; 32],
                [3; 32],
                LateReceiptKind::Delivered,
                15,
            ),
            LateReceiptDisposition::WrongPeerOrConversation
        );
        assert_eq!(tombstones.entries[0].delivered_at, None);
    }
}
