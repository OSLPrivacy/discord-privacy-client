#![cfg(feature = "core")]

use osl_privacy_hub::services::save_messaging_risk_agreement;
use osl_privacy_hub::whatsapp_message_deleter::{
    delete_marked_whatsapp_message_and_record, WhatsAppDeleteChoice, WhatsAppDeletionScope,
    WhatsAppMarkedDeleteTarget, WhatsAppMessageDeleteSurface,
    WHATSAPP_DELETE_FOR_EVERYONE_FALLBACK_DETAIL, WHATSAPP_DELETE_FOR_EVERYONE_LIMIT_SECONDS,
};
use osl_privacy_hub::whatsapp_message_reader::{
    read_whatsapp_messages_for_scrub, SharedWhatsAppMessage, WhatsAppBrowserMessage,
    WHATSAPP_SERVICE_ID,
};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const PASSWORD: &str = "task-3027-whatsapp-message-deleter";
const OWNER: &str = "task-3027-owner";
const ACCOUNT: &str = "whatsapp-scrub-account";
const PLACE: &str = "dm-scrub-w-del";
const SIGNED_IN_AUTHOR: &str = "whatsapp-scrub-owner";

struct AccountStorage {
    root: PathBuf,
}

impl AccountStorage {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "osl-task-3027-{}-{}",
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

fn open_ticked_whatsapp_account() -> (AccountStorage, String) {
    let storage = AccountStorage::new();
    let account_dir = storage.root.join("owner");
    fs::create_dir(&account_dir).expect("create account directory");
    activate(&account_dir);
    let owner = keystore::generate_identity(OWNER.to_owned())
        .user_id
        .clone();
    save_messaging_risk_agreement(&owner, WHATSAPP_SERVICE_ID, ACCOUNT)
        .expect("tick WhatsApp account");
    (storage, owner)
}

fn seeded_browser_rows() -> Vec<WhatsAppBrowserMessage> {
    (1..=3)
        .map(|number| {
            WhatsAppBrowserMessage::new(
                PLACE,
                format!("whatsapp-scrub-w-del-{number}"),
                format!("SCRUB-W-DEL-{number}"),
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
struct SeededWhatsAppSurface {
    copies: Vec<VisibleCopy>,
    delete_for_me_calls: usize,
    delete_for_everyone_calls: usize,
}

impl SeededWhatsAppSurface {
    fn seed(account_id: &str, messages: &[SharedWhatsAppMessage]) -> Self {
        Self {
            copies: messages
                .iter()
                .map(|message| VisibleCopy {
                    account_id: account_id.to_owned(),
                    place_id: message.place_id.clone(),
                    message_id: message.message_id.clone(),
                    text: message.text.clone(),
                })
                .collect(),
            ..Self::default()
        }
    }

    fn matching_count(&self, account_id: &str) -> usize {
        self.copies
            .iter()
            .filter(|copy| {
                copy.account_id == account_id
                    && copy.place_id == PLACE
                    && copy.text.contains("SCRUB-W-DEL")
            })
            .count()
    }
}

impl WhatsAppMessageDeleteSurface for SeededWhatsAppSurface {
    fn delete_for_me(
        &mut self,
        signed_in_account_id: &str,
        place_id: &str,
        message_id: &str,
    ) -> Result<(), String> {
        self.delete_for_me_calls += 1;
        self.copies.retain(|copy| {
            !(copy.account_id == signed_in_account_id
                && copy.place_id == place_id
                && copy.message_id == message_id)
        });
        Ok(())
    }

    fn delete_for_everyone(&mut self, place_id: &str, message_id: &str) -> Result<(), String> {
        self.delete_for_everyone_calls += 1;
        self.copies
            .retain(|copy| !(copy.place_id == place_id && copy.message_id == message_id));
        Ok(())
    }
}

#[test]
fn task_3027_three_matching_messages_become_two_with_default_delete_for_me() {
    let (_storage, owner) = open_ticked_whatsapp_account();
    let messages = read_whatsapp_messages_for_scrub(
        &owner,
        ACCOUNT,
        PLACE,
        true,
        SIGNED_IN_AUTHOR,
        seeded_browser_rows(),
    )
    .expect("read the three seeded WhatsApp rows");
    let mut surface = SeededWhatsAppSurface::seed(ACCOUNT, &messages);
    let before = surface.matching_count(ACCOUNT);

    let record = delete_marked_whatsapp_message_and_record(
        &mut surface,
        ACCOUNT,
        SIGNED_IN_AUTHOR,
        100,
        WhatsAppMarkedDeleteTarget::new(messages[1].clone()),
    );

    let after = surface.matching_count(ACCOUNT);
    println!("TASK3027_FIXTURE needle=SCRUB-W-DEL seeded=3 marked=1");
    println!("TASK3027_MATCHING_COUNT_BEFORE={before}");
    println!("TASK3027_MATCHING_COUNT_AFTER={after}");

    assert_eq!(messages.len(), 3);
    assert!(messages
        .iter()
        .all(|message| message.text.contains("SCRUB-W-DEL")));
    assert_eq!(before, 3);
    assert_eq!(after, 2);
    assert_eq!(surface.delete_for_me_calls, 1);
    assert_eq!(surface.delete_for_everyone_calls, 0);
    assert_eq!(record.outcomes.deleted_count, 1);
    assert_eq!(record.outcomes.failed_count, 0);
    assert_eq!(record.outcomes.needs_attention_count, 0);
    assert_eq!(record.service_actions.len(), 1);
    assert_eq!(
        record.service_actions[0].requested_scope,
        WhatsAppDeleteChoice::DeleteForMe
    );
    assert_eq!(
        record.service_actions[0].scope,
        WhatsAppDeletionScope::DeleteForMe
    );
}

#[test]
fn task_3027_expired_delete_for_everyone_falls_back_to_delete_for_me_and_records_it() {
    let (_storage, owner) = open_ticked_whatsapp_account();
    let sent_at = 10;
    let messages = read_whatsapp_messages_for_scrub(
        &owner,
        ACCOUNT,
        PLACE,
        true,
        SIGNED_IN_AUTHOR,
        [WhatsAppBrowserMessage::new(
            PLACE,
            "whatsapp-scrub-w-del-old",
            "SCRUB-W-DEL-OLD",
            sent_at,
            SIGNED_IN_AUTHOR,
        )],
    )
    .expect("read the old WhatsApp row");
    let mut surface = SeededWhatsAppSurface::seed(ACCOUNT, &messages);
    let now = sent_at + WHATSAPP_DELETE_FOR_EVERYONE_LIMIT_SECONDS as i64 + 1;

    let record = delete_marked_whatsapp_message_and_record(
        &mut surface,
        ACCOUNT,
        SIGNED_IN_AUTHOR,
        now,
        WhatsAppMarkedDeleteTarget::delete_for_everyone(messages[0].clone()),
    );
    let action = &record.service_actions[0];

    println!(
        "TASK3027_OLD_MESSAGE age_seconds={} limit_seconds={} delete_for_me_calls={} delete_for_everyone_calls={}",
        now - sent_at,
        WHATSAPP_DELETE_FOR_EVERYONE_LIMIT_SECONDS,
        surface.delete_for_me_calls,
        surface.delete_for_everyone_calls
    );
    println!("TASK3027_OLD_MESSAGE_FALLBACK_DETAIL={}", action.detail);

    assert_eq!(surface.matching_count(ACCOUNT), 0);
    assert_eq!(surface.delete_for_me_calls, 1);
    assert_eq!(surface.delete_for_everyone_calls, 0);
    assert_eq!(record.outcomes.deleted_count, 1);
    assert_eq!(record.outcomes.failed_count, 0);
    assert_eq!(record.outcomes.needs_attention_count, 0);
    assert_eq!(record.service_actions.len(), 1);
    assert_eq!(
        action.requested_scope,
        WhatsAppDeleteChoice::DeleteForEveryone
    );
    assert_eq!(action.scope, WhatsAppDeletionScope::DeleteForMe);
    assert_eq!(action.detail, WHATSAPP_DELETE_FOR_EVERYONE_FALLBACK_DETAIL);
}

#[test]
fn task_3027_delete_for_everyone_is_used_at_whatsapps_exact_time_limit() {
    let sent_at = 25;
    let message = SharedWhatsAppMessage {
        service_id: WHATSAPP_SERVICE_ID,
        account_id: ACCOUNT.to_owned(),
        place_id: PLACE.to_owned(),
        message_id: "whatsapp-scrub-w-del-limit".to_owned(),
        text: "SCRUB-W-DEL-LIMIT".to_owned(),
        time: sent_at,
        author_id: SIGNED_IN_AUTHOR.to_owned(),
        yours: true,
    };
    let mut surface = SeededWhatsAppSurface::seed(ACCOUNT, std::slice::from_ref(&message));

    let record = delete_marked_whatsapp_message_and_record(
        &mut surface,
        ACCOUNT,
        SIGNED_IN_AUTHOR,
        sent_at + WHATSAPP_DELETE_FOR_EVERYONE_LIMIT_SECONDS,
        WhatsAppMarkedDeleteTarget::delete_for_everyone(message),
    );
    let action = &record.service_actions[0];

    println!(
        "TASK3027_EXACT_LIMIT age_seconds={} delete_for_me_calls={} delete_for_everyone_calls={} scope={}",
        WHATSAPP_DELETE_FOR_EVERYONE_LIMIT_SECONDS,
        surface.delete_for_me_calls,
        surface.delete_for_everyone_calls,
        action.scope.as_str()
    );

    assert_eq!(surface.delete_for_me_calls, 0);
    assert_eq!(surface.delete_for_everyone_calls, 1);
    assert_eq!(record.outcomes.deleted_count, 1);
    assert_eq!(
        action.requested_scope,
        WhatsAppDeleteChoice::DeleteForEveryone
    );
    assert_eq!(action.scope, WhatsAppDeletionScope::DeleteForEveryone);
    assert_eq!(action.detail, "message deleted for everyone");
}
