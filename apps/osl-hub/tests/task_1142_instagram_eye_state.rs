#![cfg(feature = "core")]

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use ipc::commands::encrypt_osl_phase4_to_pubkeys;
use ipc::peer_map::legacy_entry;
use ipc::state::AppState;
use keystore::{generate_identity, Identity};
use osl_privacy_hub::instagram_direct_message::{
    dispatch_prepared_instagram_direct_message, InstagramDirectMessageDelivery,
    InstagramDirectMessageInbox, InstagramDirectMessageReceiveError,
    InstagramDirectMessageReceivingJob,
};
use osl_privacy_hub::instagram_eye_state::{
    run_instagram_eye_state_command, InstagramEye, InstagramEyeState, InstagramItemKind,
    INSTAGRAM_RECEIVING_JOB_SOURCE,
};
use osl_privacy_hub::instagram_send::InstagramSendButton;
use serde_json::Value;
use store::MessageStore;

const SENDER_ACCOUNT: &str = "instagram-eye-sender-1142";
const RECEIVER_ACCOUNT: &str = "instagram-eye-receiver-1142";
const SENDER_PROVIDER_ID: &str = "instagram-eye-provider-sender-1142";
const CHANNEL_ID: &str = "instagram-eye-channel-1142";
const FIXTURES: [(InstagramItemKind, &str, &str); 4] = [
    (
        InstagramItemKind::Message,
        "TASK1142-INSTAGRAM-MESSAGE",
        "Protected Instagram message text from receiving job 1142",
    ),
    (
        InstagramItemKind::Post,
        "TASK1142-INSTAGRAM-POST",
        "Protected Instagram post text from receiving job 1142",
    ),
    (
        InstagramItemKind::Comment,
        "TASK1142-INSTAGRAM-COMMENT",
        "Protected Instagram comment text from receiving job 1142",
    ),
    (
        InstagramItemKind::Story,
        "TASK1142-INSTAGRAM-STORY",
        "Protected Instagram story text from receiving job 1142",
    ),
];

struct Account {
    identity: Identity,
    state: AppState,
    _store_dir: tempfile::TempDir,
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
        }
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

struct InboxJob<'a>(&'a mut InstagramDirectMessageInbox);

impl InstagramDirectMessageReceivingJob for InboxJob<'_> {
    fn receive_direct_message_cover(
        &mut self,
        delivery: InstagramDirectMessageDelivery,
    ) -> Result<(), InstagramDirectMessageReceiveError> {
        self.0.receive(delivery)
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

fn delivery(
    sender: &Account,
    receiver: &Account,
    marker: &str,
    protected_text: &str,
) -> InstagramDirectMessageDelivery {
    let encrypted_cover = encrypt_osl_phase4_to_pubkeys(
        &sender.identity.x25519_secret,
        &[receiver.identity.x25519_public],
        protected_text,
    )
    .expect("sender encrypts protected Instagram text");
    let prepared = InstagramSendButton::for_selected_choice("Instant")
        .expect("Instagram Instant choice")
        .prepare_selected_cover(&encrypted_cover)
        .expect("Instagram cover prepares");
    InstagramDirectMessageDelivery::new(marker, SENDER_ACCOUNT, RECEIVER_ACCOUNT, prepared)
}

fn command(eye: &mut InstagramEye, marker: &str, state: &str) -> Value {
    let request = serde_json::json!({ "marker": marker, "state": state }).to_string();
    assert!(
        FIXTURES
            .iter()
            .all(|(_, _, protected_text)| !request.contains(protected_text)),
        "the direct state command must not carry stand-in protected text"
    );
    serde_json::from_str(&run_instagram_eye_state_command(eye, &request))
        .expect("direct Instagram eye-state command returns JSON")
}

#[test]
fn task_1142_direct_command_switches_received_instagram_items_between_both_states() {
    let sender = Account::new("task-1142-instagram-sender", [0x12; 32]);
    let receiver = Account::new("task-1142-instagram-receiver", [0x42; 32]);
    pin_sender_for_receiver(&receiver.state, &sender.identity);
    let mut inbox = InstagramDirectMessageInbox::for_account(RECEIVER_ACCOUNT);
    let stubbed = std::env::var_os("TASK1142_STUB_INSTAGRAM_RECEIVING_JOB").is_some();

    for (_, marker, protected_text) in FIXTURES {
        let delivery = delivery(&sender, &receiver, marker, protected_text);
        if stubbed {
            dispatch_prepared_instagram_direct_message(
                &mut NoopInstagramReceivingJob::default(),
                delivery,
            )
            .expect("stubbed receiving job reports its no-op call");
        } else {
            dispatch_prepared_instagram_direct_message(&mut InboxJob(&mut inbox), delivery)
                .expect("Instagram receiving job delivers the marked cover");
        }
    }

    println!("TASK1142 receiving_job_stubbed={stubbed}");
    println!(
        "TASK1142 receiving_job_cover_count={}",
        inbox.received_covers().len()
    );
    assert_eq!(
        inbox.received_covers().len(),
        4,
        "the real Instagram receiving job must supply four exact marked covers"
    );

    let exact_covers: Vec<String> = inbox
        .received_covers()
        .iter()
        .map(|delivery| delivery.cover.cover_text.clone())
        .collect();
    let mut eye = InstagramEye::default();
    for ((kind, marker, _), exact_cover) in FIXTURES.iter().zip(&exact_covers) {
        let initial = eye
            .import_from_receiving_job(
                &receiver.state,
                &inbox,
                marker,
                *kind,
                CHANNEL_ID,
                SENDER_PROVIDER_ID,
            )
            .expect("eye imports and opens exact receiving-job cover");
        assert_eq!(initial.eye_state, InstagramEyeState::Normal);
        assert_eq!(initial.shown_text, *exact_cover);
        assert_eq!(
            initial.protected_text_source,
            INSTAGRAM_RECEIVING_JOB_SOURCE
        );
    }
    assert_eq!(eye.item_count(), 4);

    let mut switches = 0usize;
    let mut exact_receiver_texts = 0usize;
    let mut maximum_protected_items_after_one_command = 0usize;
    let mut non_target_state_changes = 0usize;
    for (((kind, marker, protected_text), normal_cover), expected_index) in
        FIXTURES.iter().zip(&exact_covers).zip(1usize..)
    {
        let protected = command(&mut eye, marker, "protected");
        assert_eq!(protected["ok"], true);
        assert_eq!(protected["result"]["marker"], *marker);
        assert_eq!(protected["result"]["kind"], kind.name());
        assert_eq!(protected["result"]["before"], "normal");
        assert_eq!(protected["result"]["after"], "protected");
        assert_eq!(protected["result"]["changed"], true);
        assert_eq!(protected["result"]["shownText"], *protected_text);
        assert_eq!(
            protected["result"]["protectedTextSource"],
            INSTAGRAM_RECEIVING_JOB_SOURCE
        );
        assert_eq!(protected["result"]["receivingJobIndex"], expected_index);
        let protected_items = FIXTURES
            .iter()
            .filter(|(_, candidate, _)| {
                eye.item_state(candidate) == Some(InstagramEyeState::Protected)
            })
            .count();
        maximum_protected_items_after_one_command =
            maximum_protected_items_after_one_command.max(protected_items);
        non_target_state_changes += FIXTURES
            .iter()
            .filter(|(_, candidate, _)| {
                *candidate != *marker
                    && eye.item_state(candidate) != Some(InstagramEyeState::Normal)
            })
            .count();
        assert_eq!(protected_items, 1, "the command changes one marked item");
        assert_eq!(non_target_state_changes, 0);
        switches += 1;
        exact_receiver_texts += 1;

        let normal = command(&mut eye, marker, "normal");
        assert_eq!(normal["ok"], true);
        assert_eq!(normal["result"]["before"], "protected");
        assert_eq!(normal["result"]["after"], "normal");
        assert_eq!(normal["result"]["changed"], true);
        assert_eq!(normal["result"]["shownText"], *normal_cover);
        switches += 1;

        println!(
            "TASK1142 kind={} marker={} protected_text=\"{}\" source={} normal_cover_restored=true",
            kind.name(),
            marker,
            protected_text,
            INSTAGRAM_RECEIVING_JOB_SOURCE
        );
    }

    let noop_marker = "TASK1142-NOOP-RECEIVER-MARK";
    let noop_delivery = delivery(
        &sender,
        &receiver,
        noop_marker,
        "TASK1142 protected stand-in must never appear",
    );
    let mut noop_job = NoopInstagramReceivingJob::default();
    dispatch_prepared_instagram_direct_message(&mut noop_job, noop_delivery)
        .expect("no-op receiver reports its call");
    let missing = eye
        .import_from_receiving_job(
            &receiver.state,
            &inbox,
            noop_marker,
            InstagramItemKind::Message,
            CHANNEL_ID,
            SENDER_PROVIDER_ID,
        )
        .expect_err("no-op receiving job cannot populate protected eye text");
    println!("TASK1142 noop_receiving_job_calls={}", noop_job.calls);
    println!("TASK1142 noop_receiving_job_import_error={missing}");
    println!("TASK1142 direct_command_switch_count={switches}");
    println!("TASK1142 exact_receiver_protected_text_count={exact_receiver_texts}");
    println!(
        "TASK1142 maximum_protected_items_after_one_command={maximum_protected_items_after_one_command}"
    );
    println!("TASK1142 non_target_state_changes={non_target_state_changes}");
    assert_eq!(noop_job.calls, 1);
    assert_eq!(switches, 8);
    assert_eq!(exact_receiver_texts, 4);
    assert_eq!(maximum_protected_items_after_one_command, 1);
    assert_eq!(non_target_state_changes, 0);
}
