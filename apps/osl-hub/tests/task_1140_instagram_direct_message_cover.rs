#![cfg(feature = "core")]

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use ipc::commands::{cmd_osl_decrypt_message_v2, encrypt_osl_phase4_to_pubkeys};
use ipc::peer_map::legacy_entry;
use ipc::state::AppState;
use keystore::{generate_identity, Identity};
use osl_privacy_hub::instagram_direct_message::{
    dispatch_prepared_instagram_direct_message, InstagramDirectMessageDelivery,
    InstagramDirectMessageInbox, InstagramDirectMessageReceiveError,
    InstagramDirectMessageReceivingJob,
};
use osl_privacy_hub::instagram_send::InstagramSendButton;
use store::MessageStore;

const SENDER_ACCOUNT: &str = "instagram-dm-sender-1140";
const RECEIVER_ACCOUNT: &str = "instagram-dm-receiver-1140";
const SENDER_PROVIDER_ID: &str = "instagram-provider-sender-1140";
const CHANNEL_ID: &str = "instagram-direct-message-1140";
const COVER_MARK: &str = "TASK1140-INSTAGRAM-DM-MARKED-COVER";
const PRIVATE_WORDS: &str = "task 1140 exact private words for the second Instagram account";
const NO_COVER_RECEIVED: &str = "Instagram receiving job did not deliver a cover to this account";

struct Account {
    identity: Identity,
    state: AppState,
    _store_dir: tempfile::TempDir,
    private_messages_read: Vec<String>,
}

impl Account {
    fn new(name: &str, store_key: [u8; 32]) -> Self {
        let identity = generate_identity(name.to_owned());
        let state = AppState::new();
        state.install_identity(identity.clone());
        let store_dir = tempfile::tempdir().expect("create isolated receiver store");
        let store = MessageStore::open(store_dir.path(), &store_key).expect("open receiver store");
        *state.message_store.lock().expect("message-store mutex") = Some(store);
        Self {
            identity,
            state,
            _store_dir: store_dir,
            private_messages_read: Vec::new(),
        }
    }

    fn open_last_received_cover(
        &mut self,
        inbox: &InstagramDirectMessageInbox,
    ) -> Result<String, String> {
        let delivery = inbox
            .received_covers()
            .last()
            .ok_or_else(|| NO_COVER_RECEIVED.to_owned())?;
        let words = cmd_osl_decrypt_message_v2(
            &self.state,
            Some(delivery.message_mark.clone()),
            CHANNEL_ID.to_owned(),
            SENDER_PROVIDER_ID.to_owned(),
            delivery.cover.cover_text.clone(),
            None,
            None,
        )?;
        self.private_messages_read.push(words.clone());
        Ok(words)
    }
}

fn pin_sender_for_receiver(receiver: &AppState, sender: &Identity) {
    let mut entry = legacy_entry(sender.user_id.clone());
    entry.discord_id = Some(SENDER_PROVIDER_ID.to_owned());
    entry.pubkey = Some(STANDARD.encode(sender.x25519_public.as_bytes()));
    entry.ik_mlkem768_pub = Some(STANDARD.encode(sender.mlkem_public_bytes));
    receiver
        .peer_map
        .lock()
        .expect("peer-map mutex")
        .insert(SENDER_PROVIDER_ID.to_owned(), entry);
    receiver
        .sender_pubkey_cache
        .insert(sender.user_id.clone(), sender.x25519_public);
}

struct InboxJob<'a> {
    inbox: &'a mut InstagramDirectMessageInbox,
}

impl InstagramDirectMessageReceivingJob for InboxJob<'_> {
    fn receive_direct_message_cover(
        &mut self,
        delivery: InstagramDirectMessageDelivery,
    ) -> Result<(), InstagramDirectMessageReceiveError> {
        self.inbox.receive(delivery)
    }
}

#[derive(Default)]
struct NoopInstagramReceivingJob {
    calls: usize,
}

impl InstagramDirectMessageReceivingJob for NoopInstagramReceivingJob {
    fn receive_direct_message_cover(
        &mut self,
        _: InstagramDirectMessageDelivery,
    ) -> Result<(), InstagramDirectMessageReceiveError> {
        self.calls += 1;
        Ok(())
    }
}

fn marked_delivery(sender: &Account, receiver: &Account) -> InstagramDirectMessageDelivery {
    let encrypted_cover = encrypt_osl_phase4_to_pubkeys(
        &sender.identity.x25519_secret,
        &[receiver.identity.x25519_public],
        PRIVATE_WORDS,
    )
    .expect("sender encrypts private words into an OSL cover");
    assert!(encrypted_cover.starts_with("DPC0::"));

    let button = InstagramSendButton::for_selected_choice("Instant")
        .expect("Instagram sender uses the rendered Instant choice");
    let prepared = button
        .prepare_selected_cover(&encrypted_cover)
        .expect("the exact OSL cover prepares for Instagram");
    InstagramDirectMessageDelivery::new(COVER_MARK, SENDER_ACCOUNT, RECEIVER_ACCOUNT, prepared)
}

#[test]
fn task_1140_second_instagram_account_receives_marked_cover_and_reads_one_private_message() {
    let sender = Account::new("task-1140-instagram-sender", [0x11; 32]);
    let mut receiver = Account::new("task-1140-instagram-receiver", [0x40; 32]);
    pin_sender_for_receiver(&receiver.state, &sender.identity);

    let delivery = marked_delivery(&sender, &receiver);
    let exact_marked_cover = delivery.cover.cover_text.clone();
    let mut receiver_inbox = InstagramDirectMessageInbox::for_account(RECEIVER_ACCOUNT);
    let private_reads_before = receiver.private_messages_read.len();
    println!("TASK1140 second_account_private_messages_read_before={private_reads_before}");
    assert_eq!(private_reads_before, 0);

    dispatch_prepared_instagram_direct_message(
        &mut InboxJob {
            inbox: &mut receiver_inbox,
        },
        delivery.clone(),
    )
    .expect("the Instagram receiving job delivers the sender cover to the second account");

    assert_eq!(receiver_inbox.received_covers().len(), 1);
    let received = &receiver_inbox.received_covers()[0];
    println!(
        "TASK1140 second_account_received_cover_mark={}",
        received.message_mark
    );
    println!(
        "TASK1140 second_account_received_cover={}",
        received.cover.cover_text
    );
    println!(
        "TASK1140 second_account_received_cover_count={}",
        receiver_inbox.received_covers().len()
    );
    assert_eq!(received.message_mark, COVER_MARK);
    assert_eq!(received.cover.cover_text, exact_marked_cover);
    assert_eq!(receiver_inbox.failures().len(), 0);

    let read_back = receiver
        .open_last_received_cover(&receiver_inbox)
        .expect("second account opens the delivered OSL cover");
    let private_reads_after = receiver.private_messages_read.len();
    println!("TASK1140 second_account_private_words_read_back={read_back}");
    println!("TASK1140 second_account_private_messages_read_after={private_reads_after}");
    assert_eq!(read_back, PRIVATE_WORDS);
    assert_eq!(private_reads_after, 1);

    let mut sender_inbox = InstagramDirectMessageInbox::for_account(SENDER_ACCOUNT);
    let self_delivery = InstagramDirectMessageDelivery::new(
        COVER_MARK,
        SENDER_ACCOUNT,
        SENDER_ACCOUNT,
        delivery.cover.clone(),
    );
    let self_arrival = dispatch_prepared_instagram_direct_message(
        &mut InboxJob {
            inbox: &mut sender_inbox,
        },
        self_delivery,
    )
    .expect_err("a cover arriving on its own Instagram account is a failure");
    println!("TASK1140 self_arrival_failure={self_arrival}");
    println!(
        "TASK1140 self_arrival_failure_count={}",
        sender_inbox.failures().len()
    );
    assert!(matches!(
        self_arrival,
        InstagramDirectMessageReceiveError::SelfArrival { .. }
    ));
    assert_eq!(sender_inbox.received_covers().len(), 0);
    assert_eq!(sender_inbox.failures().len(), 1);

    let stubbed_inbox = InstagramDirectMessageInbox::for_account(RECEIVER_ACCOUNT);
    let mut no_op_job = NoopInstagramReceivingJob::default();
    dispatch_prepared_instagram_direct_message(&mut no_op_job, delivery)
        .expect("a deliberately stubbed job reports its no-op call");
    let stubbed_check = receiver.open_last_received_cover(&stubbed_inbox);
    let stubbed_failure =
        stubbed_check.expect_err("the check must fail when the receiving job does nothing");
    println!("TASK1140 noop_receiving_job_calls={}", no_op_job.calls);
    println!("TASK1140 noop_receiving_job_check_failed=true error={stubbed_failure}");
    assert_eq!(no_op_job.calls, 1);
    assert_eq!(stubbed_inbox.received_covers().len(), 0);
    assert_eq!(receiver.private_messages_read.len(), 1);
    assert_eq!(stubbed_failure, NO_COVER_RECEIVED);
}
