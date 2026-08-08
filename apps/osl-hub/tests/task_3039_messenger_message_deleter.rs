#![cfg(feature = "core")]

use osl_privacy_hub::messenger_message_deleter::{
    delete_marked_messenger_message_and_record, MessengerDeletePlaceKind, MessengerDeletionScope,
    MessengerMarkedDeleteTarget, MessengerMessageDeleteSurface,
    MESSENGER_COMMUNITY_ACCOUNT_ONLY_DETAIL,
};
use osl_privacy_hub::messenger_message_reader::{
    read_messenger_messages_for_scrub, MessengerBrowserMessage, SharedMessengerMessage,
    MESSENGER_SERVICE_ID,
};
use osl_privacy_hub::services::save_messaging_risk_agreement;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const PASSWORD: &str = "task-3039-messenger-message-deleter";
const OWNER: &str = "task-3039-owner";
const ACCOUNT: &str = "messenger-scrub-account";
const PEER_ACCOUNT: &str = "messenger-community-peer";
const PLACE: &str = "community-scrub-m-del";
const SIGNED_IN_AUTHOR: &str = "messenger-scrub-owner";

struct AccountStorage {
    root: PathBuf,
}

impl AccountStorage {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "osl-task-3039-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock after epoch")
                .as_nanos()
        ));
        fs::create_dir(&root).expect("create isolated root");
        keystore::set_base_dir_override(Some(root.clone()));
        keystore::set_active_account_dir(None);
        ipc::main_password::set_file_storage_key(None);
        ipc::main_password::set_main_password(&root, PASSWORD).expect("set main password");
        Self { root }
    }
}

impl Drop for AccountStorage {
    fn drop(&mut self) {
        keystore::set_active_account_dir(None);
        keystore::set_base_dir_override(None);
        ipc::main_password::set_file_storage_key(None);
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn activate(dir: &Path) {
    keystore::set_active_account_dir(Some(dir.to_owned()));
}

fn seeded_browser_rows() -> Vec<MessengerBrowserMessage> {
    (1..=3)
        .map(|number| {
            MessengerBrowserMessage::new(
                PLACE,
                format!("messenger-scrub-m-del-{number}"),
                format!("SCRUB-M-DEL-{number}"),
                number,
                SIGNED_IN_AUTHOR,
            )
        })
        .collect()
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct VisibleCopy {
    account_id: String,
    place_id: String,
    message_id: String,
    text: String,
}

#[derive(Default)]
struct SeededMessengerSurface {
    copies: Vec<VisibleCopy>,
    place_kinds: BTreeMap<String, MessengerDeletePlaceKind>,
    remove_for_everyone_calls: usize,
    account_copy_calls: usize,
}

impl SeededMessengerSurface {
    fn seed(account_ids: &[&str], messages: &[SharedMessengerMessage]) -> Self {
        let copies = account_ids
            .iter()
            .flat_map(|account_id| {
                messages.iter().map(move |message| VisibleCopy {
                    account_id: (*account_id).to_owned(),
                    place_id: message.place_id.clone(),
                    message_id: message.message_id.clone(),
                    text: message.text.clone(),
                })
            })
            .collect();
        Self {
            copies,
            place_kinds: BTreeMap::from([(PLACE.to_owned(), MessengerDeletePlaceKind::Community)]),
            ..Self::default()
        }
    }

    fn matching_count(&self, account_id: &str) -> usize {
        self.copies
            .iter()
            .filter(|copy| {
                copy.account_id == account_id
                    && copy.place_id == PLACE
                    && copy.text.contains("SCRUB-M-DEL")
            })
            .count()
    }
}

impl MessengerMessageDeleteSurface for SeededMessengerSurface {
    fn place_kind(&self, place_id: &str) -> Result<MessengerDeletePlaceKind, String> {
        self.place_kinds
            .get(place_id)
            .copied()
            .ok_or_else(|| "Messenger place kind is unknown".to_owned())
    }

    fn remove_for_everyone(&mut self, place_id: &str, message_id: &str) -> Result<(), String> {
        self.remove_for_everyone_calls += 1;
        self.copies
            .retain(|copy| !(copy.place_id == place_id && copy.message_id == message_id));
        Ok(())
    }

    fn remove_signed_in_account_copy(
        &mut self,
        signed_in_account_id: &str,
        place_id: &str,
        message_id: &str,
    ) -> Result<(), String> {
        self.account_copy_calls += 1;
        self.copies.retain(|copy| {
            !(copy.account_id == signed_in_account_id
                && copy.place_id == place_id
                && copy.message_id == message_id)
        });
        Ok(())
    }
}

#[test]
fn task_3039_three_matching_messages_become_two_and_community_delete_is_account_only() {
    let storage = AccountStorage::new();
    let account_dir = storage.root.join("owner");
    fs::create_dir(&account_dir).expect("create account directory");
    activate(&account_dir);
    let owner = keystore::generate_identity(OWNER.to_owned())
        .user_id
        .clone();
    save_messaging_risk_agreement(&owner, MESSENGER_SERVICE_ID, ACCOUNT)
        .expect("tick Messenger account");

    let messages = read_messenger_messages_for_scrub(
        &owner,
        ACCOUNT,
        PLACE,
        true,
        SIGNED_IN_AUTHOR,
        seeded_browser_rows(),
    )
    .expect("read seeded Messenger community");
    let mut surface = SeededMessengerSurface::seed(&[ACCOUNT, PEER_ACCOUNT], &messages);
    let before = surface.matching_count(ACCOUNT);
    let peer_before = surface.matching_count(PEER_ACCOUNT);

    let record = delete_marked_messenger_message_and_record(
        &mut surface,
        ACCOUNT,
        SIGNED_IN_AUTHOR,
        MessengerMarkedDeleteTarget::new(messages[1].clone()),
    );

    let after = surface.matching_count(ACCOUNT);
    let peer_after = surface.matching_count(PEER_ACCOUNT);
    let action = &record.service_actions[0];

    println!("TASK3039_FIXTURE needle=SCRUB-M-DEL seeded=3 marked=1");
    println!("TASK3039_MATCHING_COUNT_BEFORE={before}");
    println!("TASK3039_MATCHING_COUNT_AFTER={after}");
    println!(
        "TASK3039_COMMUNITY signed_in_before={} signed_in_after={} peer_before={} peer_after={} everyone_calls={} account_copy_calls={}",
        before,
        after,
        peer_before,
        peer_after,
        surface.remove_for_everyone_calls,
        surface.account_copy_calls
    );
    println!(
        "TASK3039_RECORD deleted={} scope={} detail={}",
        record.outcomes.deleted_count,
        action.scope.as_str(),
        action.detail
    );

    assert_eq!(before, 3);
    assert_eq!(after, 2);
    assert_eq!(messages.len(), 3);
    assert!(messages
        .iter()
        .all(|message| message.text.contains("SCRUB-M-DEL")));
    assert_eq!(peer_before, 3);
    assert_eq!(peer_after, 3, "a peer's community copy must remain");
    assert_eq!(surface.remove_for_everyone_calls, 0);
    assert_eq!(surface.account_copy_calls, 1);
    assert_eq!(record.outcomes.deleted_count, 1);
    assert_eq!(record.outcomes.failed_count, 0);
    assert_eq!(record.outcomes.needs_attention_count, 0);
    assert_eq!(record.service_actions.len(), 1);
    assert_eq!(
        action.scope,
        MessengerDeletionScope::SignedInAccountCopyOnly
    );
    assert_eq!(action.detail, MESSENGER_COMMUNITY_ACCOUNT_ONLY_DETAIL);
}

#[test]
fn task_3039_community_message_from_another_author_is_refused_before_any_removal() {
    let message = SharedMessengerMessage {
        service_id: MESSENGER_SERVICE_ID,
        account_id: ACCOUNT.to_owned(),
        place_id: PLACE.to_owned(),
        message_id: "messenger-scrub-m-del-theirs".to_owned(),
        text: "SCRUB-M-DEL-THEIRS".to_owned(),
        time: 4,
        author_id: "messenger-community-peer-author".to_owned(),
        yours: false,
    };
    let mut surface = SeededMessengerSurface::seed(&[ACCOUNT, PEER_ACCOUNT], &[message.clone()]);

    let record = delete_marked_messenger_message_and_record(
        &mut surface,
        ACCOUNT,
        SIGNED_IN_AUTHOR,
        MessengerMarkedDeleteTarget::new(message),
    );

    let reason = record.outcomes.needs_attention[0]
        .reason
        .as_deref()
        .expect("refusal reason");
    println!(
        "TASK3039_NOT_YOURS needs_attention={} reason={reason}",
        record.outcomes.needs_attention_count
    );

    assert_eq!(record.outcomes.deleted_count, 0);
    assert_eq!(record.outcomes.needs_attention_count, 1);
    assert!(reason.contains("not yours"));
    assert!(record.service_actions.is_empty());
    assert_eq!(surface.matching_count(ACCOUNT), 1);
    assert_eq!(surface.matching_count(PEER_ACCOUNT), 1);
    assert_eq!(surface.remove_for_everyone_calls, 0);
    assert_eq!(surface.account_copy_calls, 0);
}
