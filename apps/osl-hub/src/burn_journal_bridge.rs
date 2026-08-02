//! Connects the burn replay guard to the durable tombstone record.
//!
//! `BurnReplayJournal` is the one receiver journal for destructive controls.
//! A caller supplies the payload-free tombstone plus the work that records it
//! and destroys local material; duplicate delivery returns before that work
//! can run a second time.

use crate::burn_contract::{BurnContractError, BurnJournalDisposition, BurnReplayJournal};
use message_lifecycle::Tombstone;

/// The outcome of applying one authenticated burn instruction.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum TombstoneJournalDisposition {
    /// The instruction was new, so the supplied destructive work ran once.
    Applied,
    /// The same instruction had already completed; no work was repeated.
    AlreadyApplied,
}

/// Apply an authorized burn exactly once through the shared replay journal.
///
/// The callback must atomically persist `tombstone` and remove the associated
/// local payload. It receives only the payload-free terminal record, so this
/// boundary cannot accidentally retain message content in the journal.
pub fn apply_burn_once(
    journal: &mut BurnReplayJournal,
    burn_id: [u8; 32],
    nonce: [u8; 24],
    tombstone: Tombstone,
    apply_tombstone_and_destroy: impl FnOnce(Tombstone),
) -> Result<TombstoneJournalDisposition, BurnContractError> {
    match journal.accept(burn_id, nonce)? {
        BurnJournalDisposition::Applied => {
            apply_tombstone_and_destroy(tombstone);
            Ok(TombstoneJournalDisposition::Applied)
        }
        BurnJournalDisposition::AlreadyApplied => Ok(TombstoneJournalDisposition::AlreadyApplied),
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;
    use message_lifecycle::{AckOutcome, DestructReason, MessageDirection};

    fn tombstone() -> Tombstone {
        Tombstone {
            message_id: [7; 32],
            peer_id: [8; 32],
            conversation_id: [9; 32],
            direction: MessageDirection::Incoming,
            created_at: 10,
            destroyed_at: 20,
            reason: DestructReason::Burn,
            delivered_at: Some(15),
            opened_at: None,
            destruction_ack: AckOutcome::Destroyed,
        }
    }

    #[test]
    fn tf_21_redelivered_instruction_does_not_repeat_tombstone_side_effects() {
        let mut journal = BurnReplayJournal::default();
        let side_effects = Cell::new(0);

        let first = apply_burn_once(&mut journal, [1; 32], [2; 24], tombstone(), |_| {
            side_effects.set(side_effects.get() + 1);
        });
        let redelivery = apply_burn_once(&mut journal, [1; 32], [2; 24], tombstone(), |_| {
            side_effects.set(side_effects.get() + 1);
        });

        assert_eq!(first, Ok(TombstoneJournalDisposition::Applied));
        assert_eq!(redelivery, Ok(TombstoneJournalDisposition::AlreadyApplied));
        assert_eq!(side_effects.get(), 1);
    }
}
