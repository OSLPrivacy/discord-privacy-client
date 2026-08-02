#[path = "selection_gain.rs"]
mod selection_gain;

const MAX_SELECTION_ENTROPY_LOSS_BITS: f64 = 4.0;

#[test]
fn t13_tc4_caps_capability_bias_from_rejection_selection() {
    let loss = (selection_gain::SELECTED_CANDIDATE_COUNT as f64).log2();
    println!("T13-C4 capability selection entropy loss: {loss:.3} bits");
    assert!(loss <= MAX_SELECTION_ENTROPY_LOSS_BITS, "selection K leaks too much capability bias");
}
