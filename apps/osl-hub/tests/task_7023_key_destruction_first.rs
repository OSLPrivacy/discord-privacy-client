use osl_privacy_hub::message_expiry::{
    cmd_record_timed_delete_for_two_local_copies_at_paths, destroy_decryption_before_carrier,
    expire_timed_delete_records_at_path, ExpiryCarrierDelete, MessageDecryptionCapability,
    TimedDeleteProtection, TimedDeleteRecord, TimedExpiryMode,
};
use stego::{encode_mode1, ConversationCipher};
use store::{MessageStore, StoredMessage};

const KEY: [u8; 32] = [0x23; 32];
const SALT: [u8; 32] = [0x70; 32];
const SENDER: &str = "task7023-sender-device";
const RECEIVER: &str = "task7023-offline-receiver-device";
const CHANNEL: &str = "task7023-real-chat-channel";
const MESSAGE_ID: &str = "task7023-mode1-message";
const MARK: &[u8] = b"TASK7023 exact marked plaintext";

/// A real Mode 1 carrier owned by a provider rather than by OSL.  The only
/// operation OSL may request is deletion; direct `read_outside_osl` deliberately
/// bypasses every OSL capability to prove retained provider bytes are unchanged.
struct RealChatCarrier {
    bytes: String,
    original_bytes: String,
    delete_attempts: usize,
    force_delete_failure: bool,
}

impl RealChatCarrier {
    fn new(bytes: String) -> Self {
        Self {
            original_bytes: bytes.clone(),
            bytes,
            delete_attempts: 0,
            force_delete_failure: false,
        }
    }

    fn read_outside_osl(&self) -> Option<&str> {
        (!self.bytes.is_empty()).then_some(self.bytes.as_str())
    }
}

impl ExpiryCarrierDelete for RealChatCarrier {
    fn delete_carrier_message(&mut self) -> Result<(), String> {
        self.delete_attempts += 1;
        if self.force_delete_failure {
            return Err("forced carrier delete failure".to_owned());
        }
        self.bytes.clear();
        Ok(())
    }
}

fn put_marked_row(store: &MessageStore) {
    store
        .put(&StoredMessage {
            discord_message_id: MESSAGE_ID.to_owned(),
            channel_id: CHANNEL.to_owned(),
            sender_discord_id: SENDER.to_owned(),
            sender_osl_user_id: SENDER.to_owned(),
            plaintext: String::from_utf8(MARK.to_vec()).expect("mark is UTF-8"),
            decrypted_at: 1_972_023_000,
            burned: false,
            reply_parent_id: None,
            edit_revision: 1,
        })
        .expect("seed marked local cache row");
}

fn marked_count(store: &MessageStore) -> usize {
    store
        .list_by_channel(CHANNEL, 10)
        .expect("read local history")
        .iter()
        .filter(|row| row.discord_message_id == MESSAGE_ID && row.plaintext.as_bytes() == MARK)
        .count()
}

fn record() -> TimedDeleteRecord {
    TimedDeleteRecord {
        app_id: "osl-chat".to_owned(),
        conversation_id: CHANNEL.to_owned(),
        message_locator: MESSAGE_ID.to_owned(),
        sent_at_unix_seconds: 1_972_023_000,
        delete_at_unix_seconds: 1_972_023_060,
        protection: TimedDeleteProtection::Protected,
    }
}

#[test]
fn task_7023_key_destruction_is_the_floor_for_both_modes_and_offline_receiver() {
    let root = tempfile::tempdir().expect("temporary task root");
    let sender_ledger = root.path().join("sender/timed_delete_records.json");
    let receiver_ledger = root.path().join("receiver/timed_delete_records.json");
    std::fs::create_dir_all(sender_ledger.parent().expect("sender parent")).unwrap();
    std::fs::create_dir_all(receiver_ledger.parent().expect("receiver parent")).unwrap();
    let sender_store_dir = root.path().join("sender/store");
    let receiver_store_dir = root.path().join("receiver/store");
    let sender_store = MessageStore::open(&sender_store_dir, &KEY).expect("open sender cache");
    let receiver_store =
        MessageStore::open(&receiver_store_dir, &KEY).expect("open receiver cache before offline");
    put_marked_row(&sender_store);
    put_marked_row(&receiver_store);
    let fanout = cmd_record_timed_delete_for_two_local_copies_at_paths(
        SENDER,
        &sender_ledger,
        RECEIVER,
        &receiver_ledger,
        &KEY,
        record(),
    )
    .expect("existing 1342 both-copy timer fanout");
    assert_eq!(
        fanout.record_count, 2,
        "both devices must receive expiry work"
    );

    let cipher = ConversationCipher::from_salt(&SALT);
    let cover = encode_mode1(&cipher, MARK).expect("real Mode 1 chat carrier");
    let mut carrier = RealChatCarrier::new(cover);
    let mut sender = MessageDecryptionCapability::new(SENDER, SALT);
    let mut receiver = MessageDecryptionCapability::new(RECEIVER, SALT);
    receiver.set_offline(true);

    assert_eq!(sender.open_mode1_carrier(&carrier.bytes).unwrap(), MARK);
    assert_eq!(receiver.open_mode1_carrier(&carrier.bytes).unwrap(), MARK);
    assert_eq!(marked_count(&sender_store), 1);
    assert_eq!(marked_count(&receiver_store), 1);
    println!(
        "TASK7023_BEFORE mode=key_only sender_readable=true receiver_readable=true sender_count=1 receiver_count=1 provider_present={} provider_bytes_unchanged=true",
        carrier.read_outside_osl().is_some()
    );

    // The receiver UI is now offline.  Its capability is still destroyed by
    // the both-sides delivery, before we touch either local carrier-cache row.
    drop(receiver_store);
    let key_only = destroy_decryption_before_carrier(
        TimedExpiryMode::KeyOnly,
        &mut sender,
        &mut receiver,
        &mut carrier,
    )
    .expect("key-only destruction reaches both devices");
    assert_eq!(
        key_only.destroyed_device_names,
        [SENDER.to_owned(), RECEIVER.to_owned()]
    );
    assert_eq!(key_only.offline_device_names, vec![RECEIVER.to_owned()]);
    assert!(!key_only.carrier_delete_attempted);
    assert!(
        !sender.can_decrypt(),
        "{SENDER} remained readable after expiry"
    );
    assert!(
        !receiver.can_decrypt(),
        "{RECEIVER} remained readable after expiry"
    );
    assert!(sender.open_mode1_carrier(&carrier.bytes).is_err());
    assert!(receiver.open_mode1_carrier(&carrier.bytes).is_err());
    assert_eq!(carrier.delete_attempts, 0);
    assert_eq!(
        carrier.read_outside_osl(),
        Some(carrier.original_bytes.as_str())
    );

    // Existing 1342 expiry processes the local device now.  The closed receiver
    // follows 1354's reopen pattern: open the persisted copy only after expiry,
    // then apply the same exact timed record.
    let sender_expiry = expire_timed_delete_records_at_path(
        &sender_ledger,
        &KEY,
        record().delete_at_unix_seconds,
        &sender_store,
    )
    .expect("expire sender cache after capability destruction");
    let reopened_receiver =
        MessageStore::open(&receiver_store_dir, &KEY).expect("start offline receiver after expiry");
    let receiver_expiry = expire_timed_delete_records_at_path(
        &receiver_ledger,
        &KEY,
        record().delete_at_unix_seconds,
        &reopened_receiver,
    )
    .expect("expire receiver cache through 1354 reopen path");
    assert_eq!(sender_expiry.shredded_cache_rows, 1);
    assert_eq!(receiver_expiry.shredded_cache_rows, 1);
    assert_eq!(
        marked_count(&sender_store),
        0,
        "{SENDER} retained readable cache row"
    );
    assert_eq!(
        marked_count(&reopened_receiver),
        0,
        "{RECEIVER} retained readable cache row"
    );
    println!(
        "TASK7023_KEY_ONLY_AFTER sender_readable={} receiver_readable={} sender_shredded={} receiver_shredded={} receiver_offline_at_expiry={} provider_present={} provider_byte_unchanged={} carrier_delete_attempts={}",
        sender.can_decrypt(), receiver.can_decrypt(), sender_expiry.shredded_cache_rows,
        receiver_expiry.shredded_cache_rows, receiver.is_offline(),
        carrier.read_outside_osl().is_some(), carrier.read_outside_osl() == Some(carrier.original_bytes.as_str()),
        carrier.delete_attempts
    );

    // Optional mode cannot reverse the floor.  Force its carrier delete to
    // fail and prove both keys were still destroyed while provider bytes remain.
    let mut mode_two_carrier = RealChatCarrier::new(
        encode_mode1(&ConversationCipher::from_salt(&SALT), MARK).expect("mode 1 carrier"),
    );
    mode_two_carrier.force_delete_failure = true;
    let mut mode_two_sender = MessageDecryptionCapability::new(SENDER, SALT);
    let mut mode_two_receiver = MessageDecryptionCapability::new(RECEIVER, SALT);
    let mode_two = destroy_decryption_before_carrier(
        TimedExpiryMode::KeyAndCarrierMessage,
        &mut mode_two_sender,
        &mut mode_two_receiver,
        &mut mode_two_carrier,
    )
    .expect("carrier failure is reported after destruction, not as a rollback");
    assert!(mode_two.carrier_delete_attempted);
    assert_eq!(
        mode_two.carrier_delete_error.as_deref(),
        Some("forced carrier delete failure")
    );
    assert!(
        !mode_two_sender.can_decrypt(),
        "{SENDER} key survived carrier failure"
    );
    assert!(
        !mode_two_receiver.can_decrypt(),
        "{RECEIVER} key survived carrier failure"
    );
    assert_eq!(
        mode_two_carrier.read_outside_osl(),
        Some(mode_two_carrier.original_bytes.as_str())
    );
    println!(
        "TASK7023_FORCED_CARRIER_FAILURE delete_attempts={} delete_error={} sender_readable={} receiver_readable={} provider_present={} provider_byte_unchanged={}",
        mode_two_carrier.delete_attempts,
        mode_two.carrier_delete_error.as_deref().unwrap_or("missing"),
        mode_two_sender.can_decrypt(), mode_two_receiver.can_decrypt(),
        mode_two_carrier.read_outside_osl().is_some(),
        mode_two_carrier.read_outside_osl() == Some(mode_two_carrier.original_bytes.as_str())
    );

    // A secure-erase confirmation failure names the affected device, but both
    // capabilities have already been removed and no carrier request was made.
    let mut failure_carrier = RealChatCarrier::new(
        encode_mode1(&ConversationCipher::from_salt(&SALT), MARK).expect("mode 1 carrier"),
    );
    let mut failed_sender = MessageDecryptionCapability::new(SENDER, SALT);
    let mut failed_receiver = MessageDecryptionCapability::new(RECEIVER, SALT);
    failed_receiver.fail_destruction_confirmation("injected durable erase receipt failure");
    let failure = destroy_decryption_before_carrier(
        TimedExpiryMode::KeyAndCarrierMessage,
        &mut failed_sender,
        &mut failed_receiver,
        &mut failure_carrier,
    )
    .expect_err("destroy confirmation failure must be reported");
    assert_eq!(failure.failures.len(), 1);
    assert_eq!(failure.failures[0].device_name, RECEIVER);
    assert!(
        !failed_sender.can_decrypt(),
        "{SENDER} readable after {RECEIVER} failure"
    );
    assert!(
        !failed_receiver.can_decrypt(),
        "{RECEIVER} readable after its failure"
    );
    assert_eq!(failure_carrier.delete_attempts, 0);
    println!(
        "TASK7023_KEY_FAILURE failure_device={} sender_readable={} receiver_readable={} carrier_delete_attempts={}",
        failure.failures[0].device_name, failed_sender.can_decrypt(), failed_receiver.can_decrypt(),
        failure_carrier.delete_attempts
    );
}
