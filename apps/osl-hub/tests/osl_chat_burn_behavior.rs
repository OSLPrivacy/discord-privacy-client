use ipc::scope::Scope;
use osl_privacy_hub::broker::{
    activate_owned_osl_chat_context, burn_osl_chat_history, filter_osl_chat_history_visibility,
    OslChatBurnChoice,
};
use osl_privacy_hub::core_bridge::HubCoreState;
use osl_privacy_hub::security::ManualPeerBinding;
use store::{MessageStore, StoredMessage};
use tempfile::TempDir;
use uuid::Uuid;

fn seed_row(
    store: &MessageStore,
    channel_id: &str,
    message_id: &str,
    sender_osl_user_id: &str,
    plaintext: &str,
    seq: i64,
) {
    store
        .put(&StoredMessage {
            discord_message_id: message_id.to_owned(),
            channel_id: channel_id.to_owned(),
            sender_discord_id: sender_osl_user_id.to_owned(),
            sender_osl_user_id: sender_osl_user_id.to_owned(),
            plaintext: plaintext.to_owned(),
            decrypted_at: 1_800_000_000 + seq,
            burned: false,
        })
        .expect("seed chat history row");
}

fn choice_label(choice: OslChatBurnChoice) -> &'static str {
    match choice {
        OslChatBurnChoice::YourSide => "Your Side",
        OslChatBurnChoice::TheirSide => "Their Side",
        OslChatBurnChoice::BothSides => "Both Sides",
    }
}

fn choice_slug(choice: OslChatBurnChoice) -> &'static str {
    match choice {
        OslChatBurnChoice::YourSide => "your-side",
        OslChatBurnChoice::TheirSide => "their-side",
        OslChatBurnChoice::BothSides => "both-sides",
    }
}

fn expected_counts(choice: OslChatBurnChoice) -> (usize, usize, usize, usize) {
    match choice {
        OslChatBurnChoice::YourSide => (2, 0, 3, 2),
        OslChatBurnChoice::TheirSide => (0, 2, 3, 2),
        OslChatBurnChoice::BothSides => (2, 2, 1, 4),
    }
}

#[test]
fn task1352_osl_chat_burn_choices_remove_only_their_stated_records() {
    for choice in [
        OslChatBurnChoice::YourSide,
        OslChatBurnChoice::TheirSide,
        OslChatBurnChoice::BothSides,
    ] {
        let copy = choice_label(choice);
        let slug = choice_slug(choice);
        let alice = keystore::generate_identity(format!("task1352-{slug}-alice"));
        let bob = keystore::generate_identity(format!("task1352-{slug}-bob"));
        let other_osl_user_id = format!("OSLUSER-task1352-other-{}", Uuid::new_v4());
        let marked = format!("TASK1352 {copy} marked {}", Uuid::new_v4());
        let temp = TempDir::new().expect("temp history dir");
        let store = MessageStore::open(temp.path(), alice.x25519_secret.as_bytes())
            .expect("open sealed message store");
        let core = HubCoreState::default();
        *core.osl.identity.lock().expect("identity lock") = Some(alice.clone());
        *core.osl.message_store.lock().expect("store lock") = Some(store);
        let broker = osl_privacy_hub::broker::HubBrokerState::default();
        let activated = activate_owned_osl_chat_context(
            &broker,
            &alice.user_id,
            ManualPeerBinding {
                person_id: format!("task1352-{slug}-bob-person"),
                peer_osl_user_id: bob.user_id.clone(),
                peer_x25519_public: *bob.x25519_public.as_bytes(),
                peer_mlkem768_public: bob.mlkem_public_bytes,
            },
        )
        .expect("activate OSL chat context");
        let scope = Scope::try_from(activated.scope.clone()).expect("valid chat scope");
        let channel_id = scope.storage_key();
        let store_guard = core.osl.message_store.lock().expect("store lock");
        let store = store_guard.as_ref().expect("store installed");
        let your_marked = if matches!(
            choice,
            OslChatBurnChoice::YourSide | OslChatBurnChoice::BothSides
        ) {
            marked.as_str()
        } else {
            "your unmarked one"
        };
        let their_marked = if matches!(
            choice,
            OslChatBurnChoice::TheirSide | OslChatBurnChoice::BothSides
        ) {
            marked.as_str()
        } else {
            "their unmarked one"
        };
        seed_row(
            store,
            &channel_id,
            "task1352-self-1",
            &alice.user_id,
            your_marked,
            1,
        );
        seed_row(
            store,
            &channel_id,
            "task1352-self-2",
            &alice.user_id,
            "your second",
            2,
        );
        seed_row(
            store,
            &channel_id,
            "task1352-peer-1",
            &bob.user_id,
            their_marked,
            3,
        );
        seed_row(
            store,
            &channel_id,
            "task1352-peer-2",
            &bob.user_id,
            "their second",
            4,
        );
        seed_row(
            store,
            &channel_id,
            "task1352-other-1",
            &other_osl_user_id,
            "other unmarked one",
            5,
        );

        let before = store.list_by_channel(&channel_id, 10).expect("read before");
        assert_eq!(before.len(), 5);
        assert!(
            before.iter().any(|row| row.plaintext == marked),
            "{copy} must first read its random marked message"
        );
        println!("TASK1352 before copy={copy} count=5 marked=\"{marked}\" present=true");
        let hidden = filter_osl_chat_history_visibility(
            before
                .clone()
                .into_iter()
                .map(ipc::commands::StoredMessageDto::from)
                .collect(),
            &alice.user_id,
            true,
        );
        let store_count_after_hide = store
            .count_live_by_channel(&channel_id, None)
            .expect("count after hiding");
        println!(
            "TASK1352 hide_others copy={copy} visible_count={} store_count_after_hide={store_count_after_hide}",
            hidden.len()
        );
        assert_eq!(hidden.len(), 2);
        assert_eq!(store_count_after_hide, 5);
        drop(store_guard);

        let result = burn_osl_chat_history(&core, &broker, choice, false).expect("burn choice");

        let store_guard = core.osl.message_store.lock().expect("store lock");
        let store = store_guard.as_ref().expect("store installed");
        let after = store.list_by_channel(&channel_id, 10).expect("read after");
        let after_count = after.len();
        let (your_destroyed, their_destroyed, expected_after, expected_rows) =
            expected_counts(choice);
        println!(
            "TASK1352 after copy={copy} count={after_count} rows_destroyed={} your_rows_destroyed={} their_rows_destroyed={} others_rows_destroyed={} local_cleanup_complete={}",
            result.rows_destroyed,
            result.your_rows_destroyed,
            result.their_rows_destroyed,
            result.others_rows_destroyed,
            result.local_cleanup_complete
        );
        assert_eq!(result.messages_before, 5);
        assert_eq!(result.messages_after, expected_after);
        assert_eq!(after_count, expected_after);
        assert_eq!(result.rows_destroyed, expected_rows);
        assert_eq!(result.your_rows_destroyed, your_destroyed);
        assert_eq!(result.their_rows_destroyed, their_destroyed);
        assert_eq!(result.others_rows_destroyed, 0);
        assert!(result.local_cleanup_complete);
        assert!(!result.recipient_copies_deleted);
        assert!(
            after.iter().all(|row| row.plaintext != marked),
            "{copy} burn must remove the marked target row"
        );
        assert!(
            after
                .iter()
                .any(|row| row.sender_osl_user_id == other_osl_user_id),
            "{copy} burn must not remove other sender rows"
        );
    }
}
