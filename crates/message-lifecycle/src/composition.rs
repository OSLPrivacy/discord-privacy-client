//! Shared, payload-free vocabulary for destructive message lifecycle features.
//!
//! The blob and local-copy machines are deliberately separate: a server object
//! becoming unavailable says nothing about plaintext that a recipient device
//! may already hold.  This module contains only durable state and pure
//! classification rules; it grants no authority to fetch, decrypt, or delete.

use serde::{Deserialize, Serialize};

/// Server-authoritative availability of one per-device ciphertext blob.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum BlobState {
    Absent,
    Stored,
    Claimed,
    Gone,
}

/// Why a local payload was destroyed.
///
/// `UnknownDestructive` is intentionally terminal. A newer destructive reason
/// must not turn an older client into an availability downgrade.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum DestructReason {
    Burn,
    ViewOnceConsumed,
    Expired,
    Evicted,
    UnknownDestructive,
}

impl DestructReason {
    /// R2's fixed display precedence. This is severity, never arrival order.
    pub const fn precedence(self) -> u8 {
        match self {
            Self::Burn => 4,
            Self::ViewOnceConsumed => 3,
            Self::Expired => 2,
            Self::Evicted => 1,
            // An unrecognised destructive reason must not be relabelled as a
            // weaker known event. Callers can render this as a generic removal.
            Self::UnknownDestructive => 5,
        }
    }

    /// Chooses the one stable reason to display when destructive effects join.
    pub const fn higher_precedence(self, other: Self) -> Self {
        if self.precedence() >= other.precedence() {
            self
        } else {
            other
        }
    }

    /// R5: an unrecognised reason carried by a known destruct instruction is
    /// still destructive.
    pub const fn from_wire(code: u8) -> Self {
        match code {
            1 => Self::Burn,
            2 => Self::ViewOnceConsumed,
            3 => Self::Expired,
            4 => Self::Evicted,
            _ => Self::UnknownDestructive,
        }
    }
}

/// Device-authoritative state of the local plaintext copy.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum LocalCopyState {
    Announced,
    Held,
    Rendered,
    Unavailable,
    Destroyed(DestructReason),
}

/// What a device attests it did after receiving a destruct instruction.
///
/// `Unconfirmed` represents absence of an acknowledgement; it is neither
/// compliance nor refusal and must remain distinguishable from every ack.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum AckOutcome {
    Unconfirmed,
    Destroyed,
    AlreadyAbsent,
    NeverHeld,
}

/// Whether the tombstoned message was incoming or outgoing for this device.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum MessageDirection {
    Incoming,
    Outgoing,
}

/// A parsed lifecycle control instruction.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum LifecycleControl {
    Destruct(DestructReason),
    Ignore,
}

impl LifecycleControl {
    /// R5 expressed at the parsing boundary: an unknown non-destructive frame
    /// is ignored, while an unknown reason in a known destruct frame destroys.
    pub const fn from_wire(is_destruct: bool, reason_code: u8) -> Self {
        if is_destruct {
            Self::Destruct(DestructReason::from_wire(reason_code))
        } else {
            Self::Ignore
        }
    }
}

/// Payload-free record that survives destruction of the message content.
///
/// It intentionally has no plaintext, key, pointer, or server management
/// capability. The opaque identifiers are supplied by the durable caller.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Tombstone {
    pub message_id: [u8; 32],
    pub peer_id: [u8; 32],
    pub conversation_id: [u8; 32],
    pub direction: MessageDirection,
    pub created_at: u64,
    pub destroyed_at: u64,
    pub reason: DestructReason,
    pub delivered_at: Option<u64>,
    pub opened_at: Option<u64>,
    pub destruction_ack: AckOutcome,
}

#[cfg(test)]
mod tests {
    use super::{AckOutcome, DestructReason, LifecycleControl, LocalCopyState};

    #[test]
    fn tf_01_reason_precedence_is_fixed_not_arrival_order() {
        assert_eq!(
            DestructReason::Expired.higher_precedence(DestructReason::Burn),
            DestructReason::Burn
        );
        assert_eq!(
            DestructReason::ViewOnceConsumed.higher_precedence(DestructReason::Expired),
            DestructReason::ViewOnceConsumed
        );
    }

    #[test]
    fn unknown_destructive_reason_fails_closed_but_unknown_non_destructive_type_is_ignored() {
        assert_eq!(
            LifecycleControl::from_wire(true, 99),
            LifecycleControl::Destruct(DestructReason::UnknownDestructive)
        );
        assert_eq!(
            LifecycleControl::from_wire(false, 99),
            LifecycleControl::Ignore
        );
    }

    #[test]
    fn acknowledgement_silence_is_not_an_acknowledged_absence() {
        assert_ne!(AckOutcome::Unconfirmed, AckOutcome::AlreadyAbsent);
        assert_ne!(
            LocalCopyState::Unavailable,
            LocalCopyState::Destroyed(DestructReason::Expired)
        );
    }
}
