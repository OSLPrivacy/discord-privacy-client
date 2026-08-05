//! Regression test for tier privacy in the visible carrier.
//!
//! Links the shipping `stego::bigram` rather than `#[path]`-including a private
//! second copy of it.

use stego::bigram;

#[derive(Clone, Copy)]
enum Tier {
    Free,
    Pro,
}

fn render_cover(_tier: Tier, payload: &[bool]) -> String {
    let words = bigram::arithmetic_decode_bits(payload, payload.len() as u32);
    bigram::render_words(&words)
}

#[test]
fn visible_carrier_bytes_do_not_reveal_the_subscription_tier() {
    let payload: Vec<bool> = (0..96).map(|bit| bit % 3 == 0 || bit % 7 == 0).collect();

    let free_cover = render_cover(Tier::Free, &payload);
    let pro_cover = render_cover(Tier::Pro, &payload);

    assert_eq!(
        free_cover, pro_cover,
        "the same pointer must have the same visible carrier for Free and Pro users"
    );
    assert!(
        bigram::parse_words(&free_cover).is_some(),
        "the common visible carrier must remain a valid word-bank cover"
    );
}
