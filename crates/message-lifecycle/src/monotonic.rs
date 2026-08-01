//! Monotonic sender-side reduction for recipient privacy receipts.
//!
//! Receipt frames can arrive late or be replayed.  A sender may learn that a
//! message was opened before it receives the earlier delivered frame, so the
//! durable row must retain the strongest fact it has observed.

/// The durable, sender-visible receipt state for one message.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReceiptState {
    NotConfirmed,
    Delivered,
    Opened,
}

/// A recipient assertion accepted from an authenticated receipt frame.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReceiptKind {
    Delivered,
    Opened,
}

/// The effect of reducing one receipt into a durable row.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReceiptMutation {
    Advanced,
    Duplicate,
    Stale,
}

impl ReceiptState {
    /// Apply an authenticated receipt without allowing the stored state to
    /// regress.  Earlier receipts arriving after a later receipt are dropped.
    pub fn apply(&mut self, receipt: ReceiptKind) -> ReceiptMutation {
        let next = match receipt {
            ReceiptKind::Delivered => Self::Delivered,
            ReceiptKind::Opened => Self::Opened,
        };

        match next.rank().cmp(&self.rank()) {
            std::cmp::Ordering::Greater => {
                *self = next;
                ReceiptMutation::Advanced
            }
            std::cmp::Ordering::Equal => ReceiptMutation::Duplicate,
            std::cmp::Ordering::Less => ReceiptMutation::Stale,
        }
    }

    const fn rank(self) -> u8 {
        match self {
            Self::NotConfirmed => 0,
            Self::Delivered => 1,
            Self::Opened => 2,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ReceiptKind, ReceiptMutation, ReceiptState};

    #[test]
    fn tf_12_late_earlier_receipt_never_regresses_the_row() {
        let mut row = ReceiptState::NotConfirmed;

        assert_eq!(row.apply(ReceiptKind::Opened), ReceiptMutation::Advanced);
        assert_eq!(row, ReceiptState::Opened);

        assert_eq!(row.apply(ReceiptKind::Delivered), ReceiptMutation::Stale);
        assert_eq!(row, ReceiptState::Opened);
    }

    #[test]
    fn duplicate_receipts_are_idempotent() {
        let mut row = ReceiptState::NotConfirmed;

        assert_eq!(row.apply(ReceiptKind::Delivered), ReceiptMutation::Advanced);
        assert_eq!(
            row.apply(ReceiptKind::Delivered),
            ReceiptMutation::Duplicate
        );
        assert_eq!(row, ReceiptState::Delivered);
    }
}
