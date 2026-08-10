use task_5148_signal_composer_fidelity::{
    known_good_contract_receipt, validate_fidelity, validate_mutant_inventory, REQUIRED_MUTANTS,
};

#[test]
fn every_named_mutant_is_rejected_and_restored_contract_is_green() {
    let mut resolver = known_good_contract_receipt();
    resolver.resolver = "task_1031_open_direct_message_fixture";
    assert!(validate_fidelity(&resolver).is_err());

    let mut font = known_good_contract_receipt();
    font.font_from_live_sample = false;
    assert!(validate_fidelity(&font).is_err());

    let mut boundary = known_good_contract_receipt();
    boundary.boundary_displacement_px = 1;
    assert!(validate_fidelity(&boundary).is_err());

    validate_fidelity(&known_good_contract_receipt()).unwrap();
    validate_mutant_inventory(&REQUIRED_MUTANTS).unwrap();
    println!("TASK5148B_MUTANTS_REJECTED=3 RESTORED_CONTRACT=PASS");
}

#[test]
fn starving_the_red_inventory_is_named() {
    assert_eq!(
        validate_mutant_inventory(&REQUIRED_MUTANTS[..2]).unwrap_err(),
        "missing red mutant: boundary-shift"
    );
}
