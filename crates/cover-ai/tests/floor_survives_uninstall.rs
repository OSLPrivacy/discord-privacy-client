//! Regression test for D44's optional-component floor.
//!
//! `cover-ai` has a Cargo manifest now, so this links the shipping crates
//! rather than `#[path]`-including private second copies of them.

use cover_ai::fallback::{
    select_carrier, CarrierCapabilities, CarrierSource, UserVisibleTransition,
};
use stego::bigram;

#[test]
fn no_optional_components_still_produces_a_round_trippable_word_bank_carrier() {
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
        ],
        "every unavailable optional component must be disclosed before using the floor"
    );

    // The floor is the shipping integer-only word-bank codec. Its payload is
    // deliberately arbitrary: no model pack, credit balance, network request,
    // or consent state participates in this encode/decode round trip.
    const PAYLOAD_BITS: u32 = 96;
    let payload: Vec<bool> = (0..PAYLOAD_BITS)
        .map(|bit| bit % 3 == 0 || bit % 7 == 0)
        .collect();
    let words = bigram::arithmetic_decode_bits(&payload, PAYLOAD_BITS);
    let cover = bigram::render_words(&words);
    let parsed = bigram::parse_words(&cover).expect("word-bank cover must parse");
    let recovered = bigram::arithmetic_encode_words(&parsed, PAYLOAD_BITS);

    assert_eq!(
        recovered, payload,
        "the plain word-bank carrier must round trip"
    );
}
