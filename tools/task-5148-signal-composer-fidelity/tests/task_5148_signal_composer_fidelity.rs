use task_5148_signal_composer_fidelity::{
    known_good_contract_receipt, known_good_shipping_receipt, validate_fidelity,
    validate_shipping_exclusion,
};

#[test]
fn exact_envelope_contract_accepts_each_state_independently() {
    let receipt = known_good_contract_receipt();
    validate_fidelity(&receipt).unwrap();
    println!("TASK5148_CONTRACT_STATE_PASS={}", receipt.state);
}

#[test]
fn shipping_exclusion_is_a_separate_check() {
    let receipt = known_good_shipping_receipt();
    validate_shipping_exclusion(&receipt).unwrap();
    println!(
        "TASK5148_SHIPPING_CONTRACT captured_osl_pixels={} uia_identity={} uia_bounds={}",
        receipt.captured_osl_pixels, receipt.uia_identity_retained, receipt.uia_bounds_retained
    );
}

#[test]
fn starved_live_candidate_and_fixture_resolver_are_rejected() {
    let mut starved = known_good_contract_receipt();
    starved.live_candidate = false;
    assert_eq!(
        validate_fidelity(&starved).unwrap_err(),
        "live-bound candidate missing"
    );

    let mut fixture = known_good_contract_receipt();
    fixture.resolver = "task_1031_open_direct_message_fixture";
    assert!(validate_fidelity(&fixture)
        .unwrap_err()
        .contains("resolver fixture forbidden"));
}
