const TASK_3406: &str = include_str!("../examples/task_3406_place_text.rs");

#[test]
fn task_3406_command_uses_real_paste_not_character_typing_or_window_messages() {
    assert!(TASK_3406.contains("SetClipboardData"));
    assert!(TASK_3406.contains("SendInput"));
    assert!(TASK_3406.contains("VK_CONTROL"));
    assert!(TASK_3406.contains("u16::from(b'V')"));
    assert!(TASK_3406.contains("MOUSEEVENTF_LEFTDOWN"));
    assert!(TASK_3406.contains("CurrentHasKeyboardFocus"));
    assert!(!TASK_3406.contains("PostMessageW"));
    assert!(!TASK_3406.contains("KEYEVENTF_UNICODE"));
    assert!(!TASK_3406.contains("SetValue"));
}

#[test]
fn task_3406_command_restores_the_prior_clipboard_and_reports_zero_osl_entries() {
    assert!(TASK_3406.contains("snapshot_clipboard()"));
    assert!(TASK_3406.contains("restore_clipboard(&self.snapshot)"));
    assert!(TASK_3406.contains("clipboard_restored_exact={}"));
    assert!(TASK_3406.contains("osl_clipboard_entries=0"));
}

#[test]
fn task_3407_command_compares_accessibility_readback_character_by_character() {
    assert!(TASK_3406.contains("fn compare_readback(expected: &str, actual: &str)"));
    assert!(TASK_3406.contains("let readback = value_of(&composer).unwrap_or_default();"));
    assert!(TASK_3406.contains("println!(\"{}\", comparison.status_line());"));
    assert!(TASK_3406.contains("Self::Matched => \"matched\".to_owned()"));
    assert!(TASK_3406.contains("did-not-match first_differing_character_position={position}"));
    assert!(TASK_3406.contains("return ReadbackComparison::DidNotMatch"));
}

#[test]
fn task_3407_command_can_pause_for_manual_box_mutation_before_readback() {
    assert!(TASK_3406.contains("--wait-before-read-ms"));
    assert!(TASK_3406.contains("println!(\"waiting_before_read_ms={}\""));
    assert!(TASK_3406.contains("thread::sleep(Duration::from_millis(args.wait_before_read_ms));"));
}
