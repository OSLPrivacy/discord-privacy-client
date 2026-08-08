//! TASK 1460 - "Test these rules" is a find-only preview.
//!
//! Finish line: a matching fixture appears in preview and service connection
//! deletion count remains zero.
//!
//! The connection here is a real implementation of the same
//! `ipc::bad_message_preview::ServiceConnection` trait the preview takes, so
//! the preview *could* delete through it -- `delete_message` is one call away
//! on the value it is handed. The test counts every call the preview makes on
//! both methods: reads must happen, deletions must not.

use std::sync::atomic::{AtomicUsize, Ordering};

use ipc::bad_message_preview::{
    preview_bad_message_rules, PreviewMessage, ServiceConnection, PREVIEW_TREATMENT,
};
use ipc::bad_message_rules::BadMessageRule;
use ipc::commands::{
    cmd_osl_list_bad_message_rules, cmd_osl_preview_bad_message_rules, cmd_osl_save_bad_message_rule,
};
use ipc::state::AppState;
use tempfile::tempdir;

struct FileKeyGuard;

impl FileKeyGuard {
    fn install() -> Self {
        ipc::main_password::set_file_storage_key(Some([0x60; 32]));
        Self
    }
}

impl Drop for FileKeyGuard {
    fn drop(&mut self) {
        ipc::main_password::set_file_storage_key(None);
    }
}

/// A live-looking account connection that counts what the preview asks of it.
struct CountingServiceConnection {
    messages: Vec<PreviewMessage>,
    read_calls: AtomicUsize,
    deletion_calls: AtomicUsize,
}

impl CountingServiceConnection {
    fn new(messages: Vec<PreviewMessage>) -> Self {
        Self {
            messages,
            read_calls: AtomicUsize::new(0),
            deletion_calls: AtomicUsize::new(0),
        }
    }

    fn read_call_count(&self) -> usize {
        self.read_calls.load(Ordering::SeqCst)
    }

    fn deletion_call_count(&self) -> usize {
        self.deletion_calls.load(Ordering::SeqCst)
    }
}

impl ServiceConnection for CountingServiceConnection {
    fn read_messages(&self) -> Result<Vec<PreviewMessage>, String> {
        self.read_calls.fetch_add(1, Ordering::SeqCst);
        Ok(self.messages.clone())
    }

    fn delete_message(&self, message_id: &str) -> Result<(), String> {
        self.deletion_calls.fetch_add(1, Ordering::SeqCst);
        println!("TASK1460 DELETE CALLED on {message_id} -- preview is not find-only");
        Ok(())
    }
}

fn fixtures() -> Vec<PreviewMessage> {
    vec![
        PreviewMessage::new("msg-1", "general", "lunch at one, nothing private here"),
        PreviewMessage::new(
            "msg-2",
            "project-room",
            "reminder: the MAPLE-1460 rollout slips a week",
        ),
        PreviewMessage::new("msg-3", "general", "see you tomorrow"),
    ]
}

#[test]
fn task_1460_preview_marks_the_matching_fixture_with_zero_deletions() {
    let _file_key = FileKeyGuard::install();
    let dir = tempdir().unwrap();
    let state = AppState::new();

    // The rules under test are the saved bad-message rules from the gate.
    let saved = cmd_osl_save_bad_message_rule(
        &state,
        "private words".to_string(),
        "MAPLE-1460".to_string(),
        Some(dir.path().to_path_buf()),
    )
    .unwrap();
    let listed = cmd_osl_list_bad_message_rules(&state).unwrap();
    println!(
        "TASK1460 saved rule_name={} private_word={} saved_rule_count={}",
        saved.rule_name,
        saved.private_word,
        listed.len()
    );
    assert_eq!(listed.len(), 1);

    // Direct preview against a countable service connection.
    let connection = CountingServiceConnection::new(fixtures());
    let rules = vec![BadMessageRule {
        rule_name: saved.rule_name.clone(),
        private_word: saved.private_word.clone(),
    }];
    let preview = preview_bad_message_rules(&rules, &connection).unwrap();

    println!(
        "TASK1460 preview scanned={} match_count={} deleted_count={} connection_read_calls={} connection_deletion_calls={}",
        preview.scanned_message_count,
        preview.match_count(),
        preview.deleted_count,
        connection.read_call_count(),
        connection.deletion_call_count()
    );
    for hit in &preview.matches {
        println!(
            "TASK1460 preview row message_id={} channel={} rule={} matched_word={} treatment={}",
            hit.message_id, hit.channel, hit.rule_name, hit.matched_word, hit.treatment
        );
    }

    // A matching fixture appears in preview.
    assert_eq!(preview.scanned_message_count, 3);
    assert_eq!(preview.match_count(), 1);
    assert_eq!(preview.matches[0].message_id, "msg-2");
    assert_eq!(preview.matches[0].channel, "project-room");
    assert_eq!(preview.matches[0].rule_name, "private words");
    assert_eq!(preview.matches[0].matched_word, "MAPLE-1460");
    assert_eq!(preview.matches[0].treatment, PREVIEW_TREATMENT);
    // The two non-matching fixtures are not marked.
    assert!(!preview
        .matches
        .iter()
        .any(|hit| hit.message_id == "msg-1" || hit.message_id == "msg-3"));

    // Service connection deletion count remains zero, and the preview really
    // did go through the connection to find its match.
    assert_eq!(connection.deletion_call_count(), 0);
    assert_eq!(preview.deleted_count, 0);
    assert!(connection.read_call_count() > 0);

    // Same finish line through the command surface the rules page will call.
    let dto = cmd_osl_preview_bad_message_rules(&state, fixtures()).unwrap();
    println!(
        "TASK1460 command scanned={} match_count={} first_match={} deleted_count={} deletion_call_count={} read_call_count={}",
        dto.scanned_message_count,
        dto.match_count,
        dto.matches[0].message_id,
        dto.deleted_count,
        dto.deletion_call_count,
        dto.read_call_count
    );
    assert_eq!(dto.scanned_message_count, 3);
    assert_eq!(dto.match_count, 1);
    assert_eq!(dto.matches[0].message_id, "msg-2");
    assert_eq!(dto.matches[0].matched_word, "MAPLE-1460");
    assert_eq!(dto.deleted_count, 0);
    assert_eq!(dto.deletion_call_count, 0);
    assert!(dto.read_call_count > 0);

    // Nothing about the account changed: the saved rules are untouched and the
    // fixture set the preview read is still whole.
    let after = cmd_osl_list_bad_message_rules(&state).unwrap();
    println!(
        "TASK1460 after preview saved_rule_count={} fixture_count={}",
        after.len(),
        fixtures().len()
    );
    assert_eq!(after.len(), 1);
    assert_eq!(fixtures().len(), 3);
}

#[test]
fn task_1460_preview_with_no_matching_fixture_still_deletes_nothing() {
    let _file_key = FileKeyGuard::install();
    let connection = CountingServiceConnection::new(vec![
        PreviewMessage::new("msg-1", "general", "lunch at one"),
        PreviewMessage::new("msg-3", "general", "see you tomorrow"),
    ]);
    let preview = preview_bad_message_rules(
        &[BadMessageRule {
            rule_name: "private words".to_string(),
            private_word: "MAPLE-1460".to_string(),
        }],
        &connection,
    )
    .unwrap();
    println!(
        "TASK1460 no-match preview scanned={} match_count={} connection_deletion_calls={}",
        preview.scanned_message_count,
        preview.match_count(),
        connection.deletion_call_count()
    );
    assert_eq!(preview.scanned_message_count, 2);
    assert_eq!(preview.match_count(), 0);
    assert_eq!(connection.deletion_call_count(), 0);
}
