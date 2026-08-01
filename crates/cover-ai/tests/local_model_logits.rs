#[path = "../src/logit_selection.rs"]
mod logit_selection;

use logit_selection::select_from_full_logits;

#[test]
fn full_logits_produce_a_scored_token_selection() {
    let selected = select_from_full_logits(&[-4.0, f32::NAN, 0.25, 1.75, f32::NEG_INFINITY])
        .expect("a finite logit is selectable");

    assert_eq!(selected.token_index, 3);
    assert_eq!(selected.logit, 1.75);
}

#[test]
fn no_finite_logit_fails_closed() {
    assert_eq!(
        select_from_full_logits(&[f32::NAN, f32::NEG_INFINITY]),
        None
    );
}
