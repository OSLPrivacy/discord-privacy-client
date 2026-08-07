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
    assert!(TASK_3406.contains("clipboard_original_text_before={before_text:?}"));
    assert!(TASK_3406.contains("clipboard_original_text_after={after_text:?}"));
    assert!(TASK_3406.contains("ClipboardObserver::spawn"));
    assert!(TASK_3406.contains("--clipboard-observer"));
    assert!(TASK_3406.contains("observer_saw="));
    assert!(TASK_3406.contains("clipboard_second_program_saw={:?}"));
    assert!(TASK_3406.contains("count_private_canary_chars_reaching_clipboard"));
}
