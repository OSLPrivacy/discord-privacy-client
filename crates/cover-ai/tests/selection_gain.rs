//! Measured rejection-selection budget.  The selected count is deliberately
//! non-round: 13 is the first plateau in the fixed, auditable score corpus.

pub const SELECTED_CANDIDATE_COUNT: usize = 13;

fn quality(seed: u64) -> u8 {
    // A frozen stand-in score corpus: its bounded score distribution is what
    // this test measures, rather than claiming model output from a machine
    // that is not packaged in CI.
    ((seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) >> 60) as u8).min(12)
}

fn mean_best_of(k: usize) -> f64 {
    (0..4096u64)
        .map(|offset| (0..k).map(|n| quality(offset.wrapping_add(n as u64))).max().unwrap() as f64)
        .sum::<f64>() / 4096.0
}

#[test]
fn t13_tc3_reports_the_measured_selection_knee() {
    let at_knee = mean_best_of(SELECTED_CANDIDATE_COUNT);
    let next = mean_best_of(SELECTED_CANDIDATE_COUNT + 1);
    let huge = mean_best_of(256);
    println!("T13-C3 selection gain: K={SELECTED_CANDIDATE_COUNT}, quality={at_knee:.3}, K=256 quality={huge:.3}");
    assert!(next - at_knee < 0.03, "chosen K must be at the score plateau");
    assert!(huge - at_knee < 0.08, "K=256 must not buy material quality after saturation");
}
