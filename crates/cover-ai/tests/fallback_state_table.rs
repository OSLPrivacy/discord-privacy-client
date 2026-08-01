//! Regression test for the optional-cover degradation floor.

#[path = "../src/fallback.rs"]
mod fallback;

use fallback::{select_carrier, CarrierCapabilities, CarrierSource, UserVisibleTransition};

#[test]
fn missing_model_degrades_to_a_disclosed_word_bank_cover() {
    let decision = select_carrier(CarrierCapabilities {
        ai_model_available: false,
        word_bank_selection_available: true,
    });

    assert_eq!(
        decision.source,
        CarrierSource::WordBankSelectedReadableCover
    );
    assert_eq!(
        decision.transitions,
        [UserVisibleTransition::AiToWordBankSelected]
    );
    assert!(!decision.transitions[0].message().is_empty());
}

#[test]
fn no_optional_components_still_has_a_disclosed_plain_word_bank_floor() {
    let decision = select_carrier(CarrierCapabilities {
        ai_model_available: false,
        word_bank_selection_available: false,
    });

    assert_eq!(decision.source, CarrierSource::PlainWordBankCover);
    assert_eq!(
        decision.transitions,
        [
            UserVisibleTransition::AiToWordBankSelected,
            UserVisibleTransition::WordBankSelectedToPlainWordBank,
        ]
    );
}
