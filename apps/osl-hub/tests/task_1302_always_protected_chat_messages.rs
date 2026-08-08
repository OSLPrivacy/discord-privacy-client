#![cfg(feature = "core")]

fn tauri_command_body<'a>(source: &'a str, name: &str) -> &'a str {
    let signature = format!("fn {name}(");
    let start = source
        .find(&signature)
        .unwrap_or_else(|| panic!("missing command signature for {name}"));
    let rest = &source[start..];
    match rest[1..].find("\n#[tauri::command]") {
        Some(end) => &rest[..end + 1],
        None => rest,
    }
}

fn occurrences(haystack: &str, needle: &str) -> usize {
    haystack.match_indices(needle).count()
}

#[test]
fn message_command_has_one_protected_send_path_and_no_unprotected_flag() {
    let main_rs = include_str!("../src/main.rs");
    let command = tauri_command_body(main_rs, "prepare_osl_chat_text");
    let protected_paths = [
        "broker::prepare_osl_chat_text(",
        "broker::prepare_osl_chat_text_with_route_clients(",
    ];
    let protected_send_path_count: usize = protected_paths
        .iter()
        .map(|path| occurrences(command, path))
        .sum();
    let unprotected_flags = [
        "unprotected",
        "sendMode",
        "send_mode",
        "protectedMode",
        "protected_mode",
        "isProtected",
        "is_protected",
    ];
    let unprotected_flag_count: usize = unprotected_flags
        .iter()
        .map(|flag| occurrences(command, flag))
        .sum();

    println!("TASK1302_MESSAGE_COMMAND=prepare_osl_chat_text");
    println!(
        "TASK1302_PROTECTED_SEND_PATH_COUNT={protected_send_path_count} paths={protected_paths:?}"
    );
    println!(
        "TASK1302_UNPROTECTED_FLAG_COUNT={unprotected_flag_count} flags={unprotected_flags:?}"
    );

    assert_eq!(
        protected_send_path_count, 1,
        "the message command must expose exactly one protected broker send path"
    );
    assert_eq!(
        unprotected_flag_count, 0,
        "the message command must not expose an unprotected/protection-mode flag"
    );
}
