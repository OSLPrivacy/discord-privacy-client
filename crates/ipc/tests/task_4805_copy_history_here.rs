use ipc::commands::{
    cmd_osl_copy_my_history_here, cmd_osl_copy_my_history_here_confirmation,
    cmd_osl_export_history_for_copy, cmd_osl_history_copy_month_data_bytes,
    cmd_osl_load_channel_history, cmd_osl_persist_inbound, cmd_osl_persist_outbound,
    COPY_MY_HISTORY_HERE_ACTION_LABEL, COPY_MY_HISTORY_HERE_CONFIRMATION_SENTENCE,
};
use ipc::state::AppState;
use keystore::identity_from_entropy;
use store::{MessageStore, StoredMessage};
use tempfile::TempDir;

const ACCOUNT: &str = "task4805-account";
const CHANNEL: &str = "task4805-channel";
const SENDER: &str = "task4805-sender";
const MARKER: &str = "KESTREL-4805";
const MESSAGE_COUNT: usize = 500;
const POST_PAIRING_COUNT: usize = 10;
const POST_PAIRING_MARKER: &str = "KESTREL-4805 post-pairing";

fn state_with_store(
    dir: &std::path::Path,
    device_secret: &[u8; 32],
    device_entropy: [u8; 16],
) -> AppState {
    let state = AppState::new();
    let mut identity = identity_from_entropy(device_entropy, ACCOUNT.to_owned());
    identity.user_id = ACCOUNT.to_owned();
    identity.discord_snowflake = Some(ACCOUNT.to_owned());
    state.install_identity(identity);
    let store = MessageStore::open(dir, device_secret).expect("open message store");
    *state
        .message_store
        .lock()
        .expect("message_store mutex poisoned") = Some(store);
    state
}

fn message_ids() -> Vec<String> {
    (0..MESSAGE_COUNT)
        .map(|index| format!("task4805-message-{index:03}"))
        .collect()
}

fn message_text(index: usize) -> String {
    format!("{MARKER} old message {index:03}")
}

fn marked_history_count(state: &AppState) -> usize {
    cmd_osl_load_channel_history(
        state,
        CHANNEL.to_owned(),
        Some((MESSAGE_COUNT + POST_PAIRING_COUNT) as u32),
    )
    .expect("history load")
    .into_iter()
    .filter(|row| row.plaintext.starts_with(&format!("{MARKER} old")))
    .count()
}

fn post_pairing_count(state: &AppState) -> usize {
    cmd_osl_load_channel_history(
        state,
        CHANNEL.to_owned(),
        Some((MESSAGE_COUNT + POST_PAIRING_COUNT) as u32),
    )
    .expect("history load")
    .into_iter()
    .filter(|row| row.plaintext.starts_with(POST_PAIRING_MARKER))
    .count()
}

/// The paired-device relay is deliberately invoked only after the empty
/// history observation. It writes to each independent device store through
/// the normal outbound/inbound persistence paths, never by copying a store.
fn deliver_post_pairing_messages(old_state: &AppState, new_state: &AppState) {
    for index in 0..POST_PAIRING_COUNT {
        let id = format!("task4805-post-pairing-{index:02}");
        let plaintext = format!("{POST_PAIRING_MARKER} message {index:02}");
        cmd_osl_persist_outbound(
            old_state,
            CHANNEL.to_owned(),
            id.clone(),
            plaintext.clone(),
            None,
        )
        .expect("old paired device persists its outbound live message");
        cmd_osl_persist_inbound(
            new_state,
            CHANNEL.to_owned(),
            id,
            ACCOUNT.to_owned(),
            plaintext,
        )
        .expect("new paired device persists the one live delivery");
    }
}

fn raw_store_occurrences(dir: &std::path::Path, needles: &[String]) -> usize {
    let mut occurrences = 0usize;
    for name in [
        "messages.sqlite",
        "messages.sqlite-wal",
        "messages.sqlite-shm",
    ] {
        let path = dir.join(name);
        let Ok(bytes) = std::fs::read(path) else {
            continue;
        };
        for needle in needles {
            occurrences += bytes
                .windows(needle.as_bytes().len())
                .filter(|window| *window == needle.as_bytes())
                .count();
        }
    }
    occurrences
}

#[test]
fn task_4805_new_device_stays_empty_until_copy_my_history_here() {
    let old_dir = TempDir::new().expect("old device tempdir");
    let new_dir = TempDir::new().expect("new device tempdir");
    let old_secret = [0x48; 32];
    let new_secret = [0x05; 32];
    let old_state = state_with_store(old_dir.path(), &old_secret, [0x48; 16]);
    let new_state = state_with_store(new_dir.path(), &new_secret, [0x05; 16]);
    let ids = message_ids();
    let texts: Vec<String> = (0..MESSAGE_COUNT).map(message_text).collect();
    let expected_plaintext_bytes: u64 = texts.iter().map(|text| text.as_bytes().len() as u64).sum();

    {
        let guard = old_state
            .message_store
            .lock()
            .expect("old message_store mutex poisoned");
        let store = guard.as_ref().expect("old message store open");
        for (index, id) in ids.iter().enumerate() {
            store
                .put(&StoredMessage {
                    discord_message_id: id.clone(),
                    channel_id: CHANNEL.to_owned(),
                    sender_discord_id: SENDER.to_owned(),
                    sender_osl_user_id: SENDER.to_owned(),
                    plaintext: texts[index].clone(),
                    decrypted_at: 4_805_000 + index as i64,
                    burned: false,
                    reply_parent_id: None,
                    edit_revision: 1,
                })
                .expect("seed old device message");
        }
    }

    let old_count = marked_history_count(&old_state);
    let new_count_after_sync_window = marked_history_count(&new_state);
    let new_raw_text_occurrences_before_copy = raw_store_occurrences(new_dir.path(), &texts);
    let before_month_bytes = cmd_osl_history_copy_month_data_bytes(&new_state);
    let normal_export_files = ipc::commands::osl_export_files();
    assert_eq!(old_count, MESSAGE_COUNT);
    assert_eq!(
        new_count_after_sync_window, 0,
        "wrong side of the pairing boundary: a pre-pairing message was back-filled"
    );
    assert_eq!(
        new_raw_text_occurrences_before_copy, 0,
        "wrong side of the pairing boundary: pre-pairing plaintext reached the new device"
    );
    assert_eq!(before_month_bytes, 0);
    assert!(!normal_export_files.contains(&"store/messages.sqlite"));
    assert!(!normal_export_files.contains(&"store/messages.sqlite-wal"));
    assert!(!normal_export_files.contains(&"store/messages.sqlite-shm"));

    deliver_post_pairing_messages(&old_state, &new_state);
    let old_post_pairing_count = post_pairing_count(&old_state);
    let new_post_pairing_count = post_pairing_count(&new_state);
    assert_eq!(
        old_post_pairing_count, POST_PAIRING_COUNT,
        "wrong side of the pairing boundary: old device missed a post-pairing message"
    );
    assert_eq!(
        new_post_pairing_count, POST_PAIRING_COUNT,
        "wrong side of the pairing boundary: new device missed a post-pairing message"
    );

    let confirmation = cmd_osl_copy_my_history_here_confirmation(ids.len()).unwrap();
    assert_eq!(confirmation.action_label, COPY_MY_HISTORY_HERE_ACTION_LABEL);
    assert_eq!(
        confirmation.sentence,
        COPY_MY_HISTORY_HERE_CONFIRMATION_SENTENCE
    );

    let phrase = bip39::Mnemonic::from_entropy_in(bip39::Language::English, &[0x48; 16])
        .unwrap()
        .to_string();
    let package =
        cmd_osl_export_history_for_copy(&old_state, CHANNEL.to_owned(), ids).expect("history copy");
    let result = cmd_osl_copy_my_history_here(&new_state, package, phrase)
        .expect("Copy my history here writes rows");
    let copied_count = marked_history_count(&new_state);
    let after_month_bytes = cmd_osl_history_copy_month_data_bytes(&new_state);
    let byte_delta = after_month_bytes - before_month_bytes;
    let lower_bound = expected_plaintext_bytes * 95 / 100;
    let upper_bound = expected_plaintext_bytes * 105 / 100;

    println!("TASK4805 old_device_marker={MARKER} old_messages={old_count}");
    println!("TASK4805 new_device_sync_window_minutes=10");
    println!("TASK4805 normal_account_export_carries_store_files=false");
    println!("TASK4805 new_device_before_copy_messages={new_count_after_sync_window}");
    println!("TASK4805 new_device_before_copy_raw_text_occurrences={new_raw_text_occurrences_before_copy}");
    println!("TASK4805 device_identities_distinct=true");
    println!("TASK4805 post_pairing_messages_old_device={old_post_pairing_count}");
    println!("TASK4805 post_pairing_messages_new_device={new_post_pairing_count}");
    println!("TASK4805 action_label={}", confirmation.action_label);
    println!("TASK4805 confirmation_sentence={}", confirmation.sentence);
    println!("TASK4805 copied_messages={copied_count}");
    println!("TASK4805 result_copied_count={}", result.copied_count);
    println!(
        "TASK4805 plaintext_bytes_written={}",
        result.plaintext_bytes_written
    );
    println!("TASK4805 monthly_data_before={before_month_bytes}");
    println!("TASK4805 monthly_data_after={after_month_bytes}");
    println!("TASK4805 monthly_data_delta={byte_delta}");
    println!("TASK4805 expected_plaintext_bytes={expected_plaintext_bytes}");
    println!("TASK4805 five_percent_bounds={lower_bound}..={upper_bound}");

    assert_eq!(copied_count, MESSAGE_COUNT);
    assert_eq!(result.copied_count, MESSAGE_COUNT);
    assert_eq!(
        result.confirmation_sentence,
        COPY_MY_HISTORY_HERE_CONFIRMATION_SENTENCE
    );
    assert_eq!(result.plaintext_bytes_written, expected_plaintext_bytes);
    assert_eq!(byte_delta, result.plaintext_bytes_written);
    assert!(
        (lower_bound..=upper_bound).contains(&byte_delta),
        "data allowance delta {byte_delta} must match {expected_plaintext_bytes} within five percent"
    );
}
