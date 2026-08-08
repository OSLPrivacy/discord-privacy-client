#![cfg_attr(not(standalone_outlook_1286), cfg(feature = "core"))]
#![cfg_attr(standalone_outlook_1286, allow(dead_code))]

#[cfg(standalone_outlook_1286)]
mod services {
    #[derive(Debug, Clone, Eq, PartialEq)]
    pub struct MailboxFolderCandidate {
        pub folder_id: String,
        pub label: String,
    }

    impl MailboxFolderCandidate {
        pub fn new(folder_id: impl Into<String>, label: impl Into<String>) -> Self {
            Self {
                folder_id: folder_id.into(),
                label: label.into(),
            }
        }
    }

    #[derive(Debug, Clone, Eq, PartialEq)]
    pub struct MailboxMessageCandidate {
        pub folder_id: String,
        pub message_id: String,
        pub subject: String,
        pub time: i64,
        pub sender: String,
        pub body: String,
    }

    impl MailboxMessageCandidate {
        pub fn new(
            folder_id: impl Into<String>,
            message_id: impl Into<String>,
            subject: impl Into<String>,
            time: i64,
            sender: impl Into<String>,
            body: impl Into<String>,
        ) -> Self {
            Self {
                folder_id: folder_id.into(),
                message_id: message_id.into(),
                subject: subject.into(),
                time,
                sender: sender.into(),
                body: body.into(),
            }
        }
    }

    #[derive(Debug, Clone, Eq, PartialEq)]
    pub struct MailboxReaderSnapshot {
        folders: Vec<MailboxFolderCandidate>,
        messages: Vec<MailboxMessageCandidate>,
    }

    impl MailboxReaderSnapshot {
        pub fn new<F, M>(folders: F, messages: M) -> Self
        where
            F: IntoIterator<Item = MailboxFolderCandidate>,
            M: IntoIterator<Item = MailboxMessageCandidate>,
        {
            Self {
                folders: folders.into_iter().collect(),
                messages: messages.into_iter().collect(),
            }
        }
    }

    #[derive(Debug, Clone, Eq, PartialEq)]
    pub struct SharedMailboxFolder {
        pub folder_id: String,
        pub label: String,
        pub service_id: String,
        pub account_id: String,
    }

    #[derive(Debug, Clone, Eq, PartialEq)]
    pub struct SharedMailboxMessageSummary {
        pub folder_id: String,
        pub message_id: String,
        pub subject: String,
        pub time: i64,
        pub sender: String,
    }

    #[derive(Debug, Clone, Eq, PartialEq)]
    pub struct SharedMailboxMessage {
        pub folder_id: String,
        pub message_id: String,
        pub subject: String,
        pub time: i64,
        pub sender: String,
        pub body: String,
    }

    pub fn read_shared_mailbox_folders(
        _owner_osl_user_id: &str,
        service_id: &str,
        account_id: &str,
        snapshot: &MailboxReaderSnapshot,
    ) -> Result<Vec<SharedMailboxFolder>, String> {
        Ok(snapshot
            .folders
            .iter()
            .map(|folder| SharedMailboxFolder {
                folder_id: folder.folder_id.clone(),
                label: folder.label.clone(),
                service_id: service_id.to_owned(),
                account_id: account_id.to_owned(),
            })
            .collect())
    }

    pub fn read_shared_mailbox_messages(
        _owner_osl_user_id: &str,
        _service_id: &str,
        _account_id: &str,
        folder_id: &str,
        snapshot: &MailboxReaderSnapshot,
    ) -> Result<Vec<SharedMailboxMessageSummary>, String> {
        Ok(snapshot
            .messages
            .iter()
            .filter(|message| message.folder_id == folder_id)
            .map(|message| SharedMailboxMessageSummary {
                folder_id: message.folder_id.clone(),
                message_id: message.message_id.clone(),
                subject: message.subject.clone(),
                time: message.time,
                sender: message.sender.clone(),
            })
            .collect())
    }

    pub fn open_shared_mailbox_message(
        _owner_osl_user_id: &str,
        _service_id: &str,
        _account_id: &str,
        folder_id: &str,
        message_id: &str,
        snapshot: &MailboxReaderSnapshot,
    ) -> Result<SharedMailboxMessage, String> {
        snapshot
            .messages
            .iter()
            .find(|message| message.folder_id == folder_id && message.message_id == message_id)
            .map(|message| SharedMailboxMessage {
                folder_id: message.folder_id.clone(),
                message_id: message.message_id.clone(),
                subject: message.subject.clone(),
                time: message.time,
                sender: message.sender.clone(),
                body: message.body.clone(),
            })
            .ok_or_else(|| "message not found".to_owned())
    }
}

#[cfg(standalone_outlook_1286)]
#[path = "../src/native_outlook_adapter.rs"]
mod native_outlook_adapter;

#[cfg(standalone_outlook_1286)]
use native_outlook_adapter::{
    fake_outlook_desktop_task_1286_fixture, OUTLOOK_DESKTOP_TASK_1286_COVER_WORDS,
    OUTLOOK_DESKTOP_TASK_1286_MARKER,
};

#[cfg(not(standalone_outlook_1286))]
use osl_privacy_hub::native_outlook_adapter::{
    fake_outlook_desktop_task_1286_fixture, OUTLOOK_DESKTOP_TASK_1286_COVER_WORDS,
    OUTLOOK_DESKTOP_TASK_1286_MARKER,
};

#[test]
fn task_1286_fake_page_outlook_desktop_connection_places_reads_sends_and_refuses_send_removal() {
    let mut fixture = fake_outlook_desktop_task_1286_fixture();
    let controls = fixture.control_names();

    assert_eq!(fixture.sent_count(), 0);
    assert_eq!(controls, vec!["Place", "Read", "Send"]);
    println!("TASK1286_START_SENT_COUNT={}", fixture.sent_count());
    println!("TASK1286_CONTROLS={}", controls.join(","));

    let placed = fixture
        .place()
        .expect("Place control adds the cover message");
    assert_eq!(placed, OUTLOOK_DESKTOP_TASK_1286_COVER_WORDS);
    assert!(placed.contains(OUTLOOK_DESKTOP_TASK_1286_MARKER));
    assert_eq!(fixture.placed_message_count(), 1);
    println!(
        "TASK1286_PLACE_COUNT_AFTER={}",
        fixture.placed_message_count()
    );
    println!("TASK1286_PLACED_MESSAGE={placed}");

    let read = fixture
        .read()
        .expect("Read control returns the placed words");
    assert_eq!(read, OUTLOOK_DESKTOP_TASK_1286_COVER_WORDS);
    println!("TASK1286_READ_WORDS={read}");

    let before_send = fixture.sent_count();
    let after_send = fixture.send().expect("Send control sends one message");
    assert_eq!(before_send, 0);
    assert_eq!(after_send, 1);
    assert_eq!(fixture.sent_count(), 1);
    println!("TASK1286_SEND_COUNT_BEFORE={before_send}");
    println!("TASK1286_SEND_COUNT_AFTER={after_send}");

    let placed_before_remove = fixture.placed_message_count();
    let sent_before_remove = fixture.sent_count();
    let refusal = fixture
        .remove_control("Send")
        .expect_err("removing Send must be refused");
    assert_eq!(fixture.placed_message_count(), placed_before_remove);
    assert_eq!(fixture.sent_count(), sent_before_remove);
    println!("TASK1286_REMOVE_SEND_REFUSAL={refusal}");
    println!(
        "TASK1286_REMOVE_SEND_PLACED_COUNT_AFTER={}",
        fixture.placed_message_count()
    );
    println!(
        "TASK1286_REMOVE_SEND_SENT_COUNT_AFTER={}",
        fixture.sent_count()
    );
}
