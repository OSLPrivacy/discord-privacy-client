//! Regression test for the optional-cover degradation floor.

// Exercises the shipping `cover_ai::fallback`, not a `#[path]` recompile of it.
// The include-a-copy form built the module a second time as a *private* module
// of this test binary, which both hid the real crate from the test and made
// clippy judge a public API type as if it were unexported.
use cover_ai::fallback::{
    select_carrier, CarrierCapabilities, CarrierSource, UserVisibleTransition,
};

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
