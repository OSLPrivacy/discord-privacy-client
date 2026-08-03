//! Explicit outcomes for a peer's destruction acknowledgement.
//!
//! An acknowledgement says what this device found when it processed a valid
//! destruction request.  It is deliberately not optional: no acknowledgement
//! is transport silence, while each value below is an affirmative,
//! authenticated statement from the receiving device.

use std::fmt;

/// The result a device reports after handling a destruction request.
///
/// These states must remain distinct.  `AlreadyAbsent` says this device had
/// held the target but it was gone before this request; `NeverHeld` says a
/// request was received for content this device cannot truthfully claim to
/// have had.  Neither is evidence that the other party complied.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DestructionAckKind {
    /// The target was present and this request destroyed it.
    Destroyed,
    /// The target had previously existed locally but was already gone.
    AlreadyAbsent,
    /// This device never had the requested target.
    NeverHeld,
}

impl DestructionAckKind {
    /// Stable, compact wire representation for a destruction acknowledgement.
    pub const fn to_wire(self) -> u8 {
        match self {
            Self::Destroyed => 1,
            Self::AlreadyAbsent => 2,
            Self::NeverHeld => 3,
        }
    }

    /// Decode a destruction-acknowledgement outcome without collapsing it to
    /// an ambiguous success boolean.
    pub fn from_wire(value: u8) -> Result<Self, DestructionAckError> {
        match value {
            1 => Ok(Self::Destroyed),
            2 => Ok(Self::AlreadyAbsent),
            3 => Ok(Self::NeverHeld),
            other => Err(DestructionAckError::UnknownKind(other)),
        }
    }
}

/// A malformed or unsupported destruction-acknowledgement outcome.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DestructionAckError {
    UnknownKind(u8),
}

impl fmt::Display for DestructionAckError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownKind(kind) => {
                write!(formatter, "destruction acknowledgement kind {kind} is unknown")
            }
        }
    }
}

impl std::error::Error for DestructionAckError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tf_24_all_destruction_outcomes_are_explicit_and_distinct() {
        let outcomes = [
            (DestructionAckKind::Destroyed, 1),
            (DestructionAckKind::AlreadyAbsent, 2),
            (DestructionAckKind::NeverHeld, 3),
        ];

        for (outcome, wire_value) in outcomes {
            assert_eq!(outcome.to_wire(), wire_value);
            assert_eq!(DestructionAckKind::from_wire(wire_value), Ok(outcome));
        }

        assert_eq!(
            DestructionAckKind::from_wire(0),
            Err(DestructionAckError::UnknownKind(0)),
            "silence is not a destruction acknowledgement"
        );
    }
}
