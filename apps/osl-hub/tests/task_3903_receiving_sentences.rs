#![cfg(feature = "core")]

use std::collections::{BTreeMap, BTreeSet};

const INVENTORY: &str = include_str!("../../../docs/qa/receiving-screen-sentences.md");
const BROKER_RS: &str = include_str!("../src/broker.rs");
const MAIN_RS: &str = include_str!("../src/main.rs");

#[test]
fn task_3903_receiving_screen_sentences_are_inventory_complete() {
    let inventory = inventory_sentences();
    assert!(
        inventory.len() >= 6,
        "receiving screen-sentence inventory must list at least 6 sentences, got {}",
        inventory.len()
    );
    assert!(
        INVENTORY.contains("NotAToken")
            && INVENTORY.contains("PointerBlobGone")
            && INVENTORY.contains("Rejected")
            && INVENTORY.contains("This encrypted message could not be opened")
            && INVENTORY.contains("That is a rule, not a bug"),
        "inventory must name the three causes that deliberately share the generic refusal sentence"
    );

    let found = receiving_code_sentences();
    let missing: Vec<_> = found
        .keys()
        .filter(|sentence| !inventory.contains(*sentence))
        .cloned()
        .collect();
    assert!(
        missing.is_empty(),
        "receiving code has {} sentence(s) not on the saved list: {}",
        missing.len(),
        missing.join(" | ")
    );

    println!(
        "TASK3903 inventory_sentences={} receiving_code_sentences={} unlisted_sentences=0",
        inventory.len(),
        found.len()
    );
}

fn inventory_sentences() -> BTreeSet<String> {
    let mut sentences = BTreeSet::new();
    let lines: Vec<&str> = INVENTORY.lines().collect();
    for (index, line) in lines.iter().enumerate() {
        let Some(rest) = line.trim().strip_prefix("- Sentence: ") else {
            continue;
        };
        let sentence = quoted(rest).unwrap_or_else(|| panic!("bad sentence line: {line}"));
        let has_cause = lines
            .iter()
            .skip(index + 1)
            .find(|next| !next.trim().is_empty())
            .is_some_and(|next| next.trim().starts_with("Cause: "));
        assert!(
            has_cause,
            "sentence lacks the required adjacent cause: {sentence}"
        );
        sentences.insert(sentence);
    }
    sentences
}

fn receiving_code_sentences() -> BTreeMap<String, BTreeSet<&'static str>> {
    let spans = [
        (
            "broker:persist_osl_chat_inbound",
            BROKER_RS,
            "fn persist_osl_chat_inbound(",
            "/// The bilateral secret",
        ),
        (
            "broker:open_peer_prose_text",
            BROKER_RS,
            "pub fn open_peer_prose_text(",
            "fn whatsapp_qa_peer_context(",
        ),
        (
            "broker:peer_pointer_failure_messages",
            BROKER_RS,
            "impl PeerProsePointerFailure {",
            "/// Classify what `prose_token_recv`",
        ),
        (
            "broker:drain_native_discord_overlay_text",
            BROKER_RS,
            "pub fn drain_native_discord_overlay_text(",
            "/// Copy for the one case",
        ),
        (
            "broker:drain_osl_chat_text",
            BROKER_RS,
            "pub fn drain_osl_chat_text(",
            "/// Fetch the active peer",
        ),
        (
            "broker:retained_control_inbox_refusal",
            BROKER_RS,
            "fn retained_control_inbox_refusal(",
            "fn retained_attachment_control_inbox_refusal(",
        ),
        (
            "broker:drain_peer_inbox_text",
            BROKER_RS,
            "fn drain_peer_inbox_text(",
            "pub fn load_osl_chat_history(",
        ),
        (
            "broker:load_osl_chat_history",
            BROKER_RS,
            "pub fn load_osl_chat_history(",
            "pub fn add_osl_chat_reaction(",
        ),
        (
            "broker:require_decrypt_display_enabled",
            BROKER_RS,
            "fn require_decrypt_display_enabled(",
            "fn apply_successful_open_policy(",
        ),
        (
            "main:open_native_discord_overlay_text",
            MAIN_RS,
            "async fn open_native_discord_overlay_text(",
            "#[tauri::command]\nasync fn reveal_native_discord_overlay_view_once",
        ),
        (
            "main:reveal_native_discord_overlay_view_once",
            MAIN_RS,
            "async fn reveal_native_discord_overlay_view_once(",
            "#[tauri::command]\nasync fn prepare_osl_chat_text",
        ),
        (
            "main:open_osl_chat_text",
            MAIN_RS,
            "async fn open_osl_chat_text(",
            "#[tauri::command]\nasync fn list_osl_chat_history",
        ),
        (
            "main:list_osl_chat_history",
            MAIN_RS,
            "async fn list_osl_chat_history(",
            "#[tauri::command]\nasync fn select_osl_chat_attachment",
        ),
    ];

    let mut found: BTreeMap<String, BTreeSet<&'static str>> = BTreeMap::new();
    for (name, source, start, end) in spans {
        let slice = source_slice(source, start, end);
        for literal in string_literals(slice) {
            if sentence_like(&literal) {
                found.entry(literal).or_default().insert(name);
            }
        }
    }
    found
}

fn source_slice<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
    let start_at = source
        .find(start)
        .unwrap_or_else(|| panic!("missing start anchor: {start}"));
    let end_at = source[start_at + start.len()..]
        .find(end)
        .map(|offset| start_at + start.len() + offset)
        .unwrap_or_else(|| panic!("missing end anchor after {start}: {end}"));
    &source[start_at..end_at]
}

fn quoted(line: &str) -> Option<String> {
    let first = line.find('"')?;
    let last = line.rfind('"')?;
    (last > first).then(|| line[first + 1..last].replace("\\\"", "\""))
}

fn sentence_like(value: &str) -> bool {
    value.len() >= 10
        && value.contains(' ')
        && value.chars().any(char::is_alphabetic)
        && value
            .chars()
            .next()
            .is_some_and(|first| first.is_ascii_uppercase())
        && !value.starts_with("DPC0::")
}

fn string_literals(source: &str) -> Vec<String> {
    #[derive(Clone, Copy, Eq, PartialEq)]
    enum State {
        Code,
        String,
        LineComment,
        BlockComment,
    }

    let mut out = Vec::new();
    let mut state = State::Code;
    let mut chars = source.char_indices().peekable();
    let mut current = String::new();
    while let Some((_, ch)) = chars.next() {
        match state {
            State::Code => match ch {
                '"' => {
                    current.clear();
                    state = State::String;
                }
                '/' if chars.peek().is_some_and(|(_, next)| *next == '/') => {
                    chars.next();
                    state = State::LineComment;
                }
                '/' if chars.peek().is_some_and(|(_, next)| *next == '*') => {
                    chars.next();
                    state = State::BlockComment;
                }
                _ => {}
            },
            State::String => match ch {
                '\\' => {
                    if let Some((_, escaped)) = chars.next() {
                        match escaped {
                            '"' => current.push('"'),
                            '\\' => current.push('\\'),
                            'n' => current.push('\n'),
                            'r' => current.push('\r'),
                            't' => current.push('\t'),
                            other => {
                                current.push('\\');
                                current.push(other);
                            }
                        }
                    }
                }
                '"' => {
                    out.push(current.clone());
                    state = State::Code;
                }
                _ => current.push(ch),
            },
            State::LineComment => {
                if ch == '\n' {
                    state = State::Code;
                }
            }
            State::BlockComment => {
                if ch == '*' && chars.peek().is_some_and(|(_, next)| *next == '/') {
                    chars.next();
                    state = State::Code;
                }
            }
        }
    }
    out
}
