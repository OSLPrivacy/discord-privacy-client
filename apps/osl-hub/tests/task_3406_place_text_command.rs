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
fn task_3406_command_measures_clipboard_exposure_and_second_process_visibility() {
    assert!(TASK_3406.contains("clipboard_exposure_ms={exposure_ms}"));
    assert!(TASK_3406.contains("clipboard_private_chars={private_chars}"));
    assert!(TASK_3406.contains("clipboard_private_chars_found={private_chars_found:?}"));
    assert!(TASK_3406.contains(
        "{private_chars} private canary characters reached the clipboard: {private_chars_found:?}"
    ));
    assert!(TASK_3406.contains("clipboard_original_text_before={before_text:?}"));
    assert!(TASK_3406.contains("clipboard_original_text_after={after_text:?}"));
    assert!(TASK_3406.contains("ClipboardObserver::spawn"));
    assert!(TASK_3406.contains("--clipboard-observer"));
    assert!(TASK_3406.contains("observer_saw="));
    assert!(TASK_3406.contains("clipboard_second_program_saw={:?}"));
    assert!(TASK_3406.contains("private_canary_chars_reaching_clipboard"));
}

#[test]
fn task_3769_refuses_private_canary_clipboard_payload() {
    assert!(
        TASK_3406.contains("let staged_at = stage_clipboard_text(&text)"),
        "10 private canary characters reached the clipboard: \"QQQQQQQQQQ\""
    );
    assert!(
        !TASK_3406.contains("let staged_at = stage_clipboard_text(&private_canary)"),
        "10 private canary characters reached the clipboard: \"QQQQQQQQQQ\""
    );
}

fn task_3407_command_can_pause_for_manual_box_mutation_before_readback() {
    assert!(TASK_3406.contains("--wait-before-read-ms"));
    assert!(TASK_3406.contains("println!(\"waiting_before_read_ms={}\""));
    assert!(TASK_3406.contains("thread::sleep(Duration::from_millis(args.wait_before_read_ms));"));
}

fn task_3407_command_compares_accessibility_readback_character_by_character() {
    assert!(TASK_3406.contains("fn compare_readback(expected: &str, actual: &str)"));
    assert!(TASK_3406.contains("let readback = value_of(&composer).unwrap_or_default();"));
    assert!(TASK_3406.contains("println!(\"{}\", comparison.status_line());"));
    assert!(TASK_3406.contains("Self::Matched => \"matched\".to_owned()"));
    assert!(TASK_3406.contains("did-not-match first_differing_character_position={position}"));
    assert!(TASK_3406.contains("return ReadbackComparison::DidNotMatch"));
}

fn task_3415_adds_no_telegram_only_placing_code_to_the_shared_job() {
    let production = TASK_3406
        .split("#[cfg(test)]")
        .next()
        .unwrap_or(TASK_3406)
        .to_ascii_lowercase();
    assert!(
        !production.contains("telegram"),
        "the shared 3406 placing job must stay provider-neutral; pass --app Telegram at runtime"
    );
}

fn task_3415_shared_job_accepts_native_uia_roots_before_chromium_fallback() {
    assert!(TASK_3406.contains("fn accessibility_root"));
    assert!(TASK_3406.contains("ElementFromHandle"));
    assert!(TASK_3406.contains("\"uia_native\""));
    assert!(TASK_3406.contains("wake_electron_accessibility"));
    assert!(TASK_3406.contains("\"msaa_client_after_wake\""));
}

fn task_3415_shared_job_checks_empty_before_and_exact_mark_after() {
    assert!(TASK_3406.contains("before_readback={before_readback:?}"));
    assert!(TASK_3406.contains("verify_marked_placement(&before_readback, &readback, &args.text)"));
    assert!(TASK_3406.contains("readback did not equal placed mark"));
}
