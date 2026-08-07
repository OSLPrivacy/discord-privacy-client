const TASK_3406: &str = include_str!("../../examples/task_3406_place_text.rs");

#[test]
fn command_reports_matched_only_after_accessibility_readback() {
    assert!(TASK_3406.contains("let readback = value_of(&composer).unwrap_or_default();"));
    assert!(TASK_3406.contains("let comparison = compare_readback(&args.text, &readback);"));
    assert!(TASK_3406.contains("println!(\"{}\", comparison.status_line());"));
    assert!(TASK_3406.contains("Self::Matched => \"matched\".to_owned()"));
}

#[test]
fn command_reports_first_differing_character_position_on_mismatch() {
    assert!(TASK_3406.contains("fn compare_readback(expected: &str, actual: &str)"));
    assert!(TASK_3406.contains("for position in 1.."));
    assert!(TASK_3406.contains("return ReadbackComparison::DidNotMatch"));
    assert!(TASK_3406.contains("did-not-match first_differing_character_position={position}"));
    assert!(TASK_3406.contains("expected={} actual={}"));
}

#[test]
fn command_can_wait_after_place_before_read_for_manual_mutation_check() {
    assert!(TASK_3406.contains("--wait-before-read-ms"));
    assert!(TASK_3406.contains("println!(\"waiting_before_read_ms={}\""));
    assert!(TASK_3406.contains("thread::sleep(Duration::from_millis(args.wait_before_read_ms));"));
}
