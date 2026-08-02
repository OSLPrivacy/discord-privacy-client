#[path = "selection_gain.rs"]
mod selection_gain;

#[test]
fn t13_tc4b_selection_distortion_stays_under_the_declared_cap() {
    // For uniform candidate ranks the selected rank has CDF 1-(1-x)^K.  Its
    // mean displacement from an unselected rank is the auditable distortion
    // proxy used to cap K until a packaged analyser replaces this harness.
    let k = selection_gain::SELECTED_CANDIDATE_COUNT as f64;
    let displacement = k / (k + 1.0) - 0.5;
    println!("T13-C4b selected/unselected rank displacement at K={k:.0}: {displacement:.3}");
    assert!(displacement < 0.45, "selection distortion exceeds the channel cap");
}
