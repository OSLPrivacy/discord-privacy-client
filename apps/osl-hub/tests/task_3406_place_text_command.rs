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
fn task_3419_command_refuses_higher_permission_app_before_clipboard_stage() {
    assert!(TASK_3406.contains("GetTokenInformation"));
    assert!(TASK_3406.contains("TokenIntegrityLevel"));
    assert!(TASK_3406.contains("permission_check app={}"));
    assert!(TASK_3406.contains("{app} has more permission than OSL"));
    assert!(TASK_3406.contains("placed_count=0"));
    assert!(TASK_3406.contains("placed_count=1"));

    let permission_gate = TASK_3406
        .find("refuse_if_app_has_more_permission_than_osl(&args.app, discord.hwnd)?;")
        .expect("permission gate is called");
    let clipboard_stage = TASK_3406
        .find("stage_clipboard_text(&args.text)")
        .expect("clipboard staging is called");
    assert!(
        permission_gate < clipboard_stage,
        "the higher-permission refusal must run before OSL stages the clipboard"
    );
}
