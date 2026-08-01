//! The carrier fallback policy.
//!
//! Encryption is identical for every source below.  The only thing that
//! changes is how readable the cover text is.  Keep this table as the sole
//! definition of that degradation order so optional AI can never gate a send.

/// Where a cover was selected, named for its readability rather than a
/// misleading claim about any change to encryption.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CarrierSource {
    AiSelectedReadableCover,
    WordBankSelectedReadableCover,
    PlainWordBankCover,
}

/// The availability inputs to the fallback table.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CarrierCapabilities {
    pub ai_model_available: bool,
    pub word_bank_selection_available: bool,
}

/// A notice which callers must display when cover readability degrades.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UserVisibleTransition {
    AiToWordBankSelected,
    WordBankSelectedToPlainWordBank,
}

impl UserVisibleTransition {
    /// Plain language suitable for the send UI; callers must not suppress it.
    pub const fn message(self) -> &'static str {
        match self {
            Self::AiToWordBankSelected => {
                "AI cover text is unavailable; using a word-bank-selected cover instead."
            }
            Self::WordBankSelectedToPlainWordBank => {
                "A word-bank-selected cover is unavailable; using a plain word-bank cover instead."
            }
        }
    }
}

/// The resolved source plus the required disclosure for a fallback.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CarrierDecision {
    pub source: CarrierSource,
    pub transitions: Vec<UserVisibleTransition>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct FallbackRow {
    source: CarrierSource,
    unavailable_transition: Option<UserVisibleTransition>,
}

const FALLBACK_TABLE: [FallbackRow; 3] = [
    FallbackRow {
        source: CarrierSource::AiSelectedReadableCover,
        unavailable_transition: Some(UserVisibleTransition::AiToWordBankSelected),
    },
    FallbackRow {
        source: CarrierSource::WordBankSelectedReadableCover,
        unavailable_transition: Some(UserVisibleTransition::WordBankSelectedToPlainWordBank),
    },
    FallbackRow {
        source: CarrierSource::PlainWordBankCover,
        unavailable_transition: None,
    },
];

/// Resolve the only permitted degradation chain: AI, selected word bank, then
/// the always-present plain word bank.
pub fn select_carrier(capabilities: CarrierCapabilities) -> CarrierDecision {
    let mut transitions = Vec::new();

    for row in FALLBACK_TABLE {
        let available = match row.source {
            CarrierSource::AiSelectedReadableCover => capabilities.ai_model_available,
            CarrierSource::WordBankSelectedReadableCover => {
                capabilities.word_bank_selection_available
            }
            CarrierSource::PlainWordBankCover => true,
        };
        if available {
            return CarrierDecision {
                source: row.source,
                transitions,
            };
        }
        transitions.push(
            row.unavailable_transition
                .expect("each optional fallback row discloses its degradation"),
        );
    }

    unreachable!("the plain word bank is always available")
}
