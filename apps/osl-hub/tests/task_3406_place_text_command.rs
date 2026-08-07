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
fn task_3415_shared_job_accepts_native_uia_roots_before_chromium_fallback() {
    assert!(TASK_3406.contains("fn accessibility_root"));
    assert!(TASK_3406.contains("ElementFromHandle"));
    assert!(TASK_3406.contains("\"uia_native\""));
    assert!(TASK_3406.contains("wake_electron_accessibility"));
    assert!(TASK_3406.contains("\"msaa_client_after_wake\""));
}

#[test]
fn task_3415_shared_job_checks_empty_before_and_exact_mark_after() {
    assert!(TASK_3406.contains("before_readback={before_readback:?}"));
    assert!(TASK_3406.contains("verify_marked_placement(&before_readback, &readback, &args.text)"));
    assert!(TASK_3406.contains("readback did not equal placed mark"));
}

#[test]
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
