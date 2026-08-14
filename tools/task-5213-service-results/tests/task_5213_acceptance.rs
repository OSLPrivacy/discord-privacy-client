#[test]
fn task_5213_acceptance() {
    let report = task_5213_service_results::check(None, None, None).expect("5213 acceptance");
    println!("{report}");
    assert!(report.contains("english_schema_fields=0"));
    assert!(report.contains("changed_keys=3 changed_results=3"));
}
