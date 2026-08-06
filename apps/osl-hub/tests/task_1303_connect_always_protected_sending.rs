#![cfg(feature = "core")]

#[path = "sealed_relay_e2e.rs"]
mod sealed_relay_e2e;

fn occurrences(haystack: &str, needle: &str) -> usize {
    haystack.match_indices(needle).count()
}

fn source_between<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
    let start_index = source
        .find(start)
        .unwrap_or_else(|| panic!("missing source start marker: {start}"));
    let end_index = source[start_index..]
        .find(end)
        .map(|offset| start_index + offset)
        .unwrap_or_else(|| panic!("missing source end marker after {start}: {end}"));
    &source[start_index..end_index]
}

#[test]
fn task_1303_direct_send_from_typing_box_creates_one_protected_message() {
    let view_source = include_str!("../../osl-hub-ui/src/osl-chats-view.ts");
    assert!(view_source.contains("export function submitsOslChatDraft("));
    assert!(view_source.contains("if (event.key !== \"Enter\") return false;"));
    assert!(view_source.contains("if (event.isComposing) return false;"));
    assert!(view_source
        .contains("return !event.altKey && !event.ctrlKey && !event.metaKey && !event.shiftKey;"));

    let main_source = include_str!("../../osl-hub-ui/src/main.ts");
    let typing_box_binding = source_between(
        main_source,
        "const oslChatDraftInput = document.querySelector<HTMLTextAreaElement>(\"#osl-chat-draft\");",
        "document.querySelector<HTMLInputElement>(\"#osl-chat-view-once\")",
    );
    assert!(typing_box_binding.contains("oslChatDraftInput?.addEventListener(\"input\""));
    assert!(typing_box_binding.contains("setOslChatDraft(oslChatDraftInput.value, false);"));
    assert!(typing_box_binding.contains("oslChatDraftInput?.addEventListener(\"keydown\""));
    assert!(typing_box_binding.contains("if (!submitsOslChatDraft(event)) return;"));
    assert!(typing_box_binding.contains("event.preventDefault();"));
    assert!(typing_box_binding.contains("form.requestSubmit(send)"));

    let send_source = source_between(
        main_source,
        "async function sendOslChat(event: SubmitEvent): Promise<void> {",
        "function resetOslChatUiState",
    );
    assert!(send_source.contains("const draft = oslChatDraft;"));
    assert!(send_source.contains("prepareOslChatText(draft, oslChatViewOnce)"));
    assert_eq!(occurrences(send_source, "messageId: sent.messageId"), 1);
    assert!(send_source.contains("body: draft"));

    println!("TASK1303_TYPING_BOX_SELECTOR=#osl-chat-draft");
    println!("TASK1303_ENTER_ACTION=submitsOslChatDraft(event)->form.requestSubmit(send)");
    println!("TASK1303_BOX_TEXT_SOURCE=oslChatDraftInput.value");
    println!(
        "TASK1303_PROTECTED_PREPARE_CALLS={}",
        occurrences(send_source, "prepareOslChatText(draft, oslChatViewOnce)")
    );
    println!(
        "TASK1303_OUTGOING_MESSAGE_PUSHES={}",
        occurrences(send_source, "messageId: sent.messageId")
    );

    sealed_relay_e2e::task_1303_direct_send_creates_one_protected_message_from_box_text();
}
