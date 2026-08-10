use keystore::{allowed_proving_services, required_outside_proof_count, validate_proving_service};

#[test]
fn gate_3118_allows_no_outside_public_name_proving_services() {
    println!(
        "TASK3123_ALLOWED_PROVING_SERVICES=[{}]",
        allowed_proving_services().join(",")
    );
    println!("TASK3123_PROOFS_NEEDED={}", required_outside_proof_count());
    assert!(allowed_proving_services().is_empty());
    assert_eq!(required_outside_proof_count(), 0);

    let error = validate_proving_service("discord").expect_err("discord must be rejected");
    println!("TASK3123_NOT_ALLOWED={error}");
    assert_eq!(error.service(), "discord");
    assert!(error.to_string().contains("not allowed"));
    assert!(error.to_string().contains("discord"));
}
