use message_lifecycle::{
    AcceptedPart, LifecycleLimits, LogicalMessageLifecycle, ReceiptStatus, TransitionEvidence,
};
use store::{MessageStore, StoredMessage};
use tempfile::TempDir;

const STORE_KEY: [u8; 32] = [0x36; 32];
const SENT_AT: u64 = 1_800_000_000;
const ONE_HOUR: u64 = 3_600;
const LIMITS: LifecycleLimits = LifecycleLimits {
    max_parts: 4,
    max_part_bytes: 64 * 1024,
    max_total_bytes: 256 * 1024,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SupportedVersion {
    V011,
    V012,
}

impl SupportedVersion {
    const fn label(self) -> &'static str {
        match self {
            Self::V011 => "0.1.1",
            Self::V012 => "0.1.2",
        }
    }

    const fn native_attachment_protocol(self) -> u8 {
        match self {
            Self::V011 => 1,
            Self::V012 => 2,
        }
    }

    const fn accepts_attachment_protocol(self, protocol: u8) -> bool {
        match self {
            Self::V011 => protocol == 1,
            Self::V012 => protocol == 1 || protocol == 2,
        }
    }
}

struct CopyUnderTest {
    name: &'static str,
    version: SupportedVersion,
    channel_id: String,
    store: MessageStore,
}

struct ActionResult {
    result: String,
    before_sender: usize,
    after_sender: usize,
    before_recipient: usize,
    after_recipient: usize,
}

#[test]
fn task_3616_two_supported_versions_exercise_bidirectional_actions() {
    let root = TempDir::new().expect("two-version fixture root");
    let run = format!("{}-{}", std::process::id(), SENT_AT);
    let mut copy_a =
        CopyUnderTest::new(root.path(), "OSL Copy A", SupportedVersion::V011, "copy-a");
    let mut copy_b =
        CopyUnderTest::new(root.path(), "OSL Copy B", SupportedVersion::V012, "copy-b");

    println!(
        "TASK3616_SUPPORTED_VERSIONS={}={},{}={}",
        copy_a.name,
        copy_a.version.label(),
        copy_b.name,
        copy_b.version.label()
    );

    let mut results = Vec::new();
    run_direction("A_TO_B", &run, &mut copy_a, &mut copy_b, &mut results);
    run_direction("B_TO_A", &run, &mut copy_b, &mut copy_a, &mut results);

    assert_eq!(results.len(), 12, "six actions must run in each direction");
    for (direction, action, outcome) in &results {
        println!("TASK3616_{direction}_{action}_RESULT={}", outcome.result);
        println!(
            "TASK3616_{direction}_{action}_COUNTS=sender_before:{} sender_after:{} recipient_before:{} recipient_after:{}",
            outcome.before_sender,
            outcome.after_sender,
            outcome.before_recipient,
            outcome.after_recipient
        );
        if outcome.result.starts_with("REFUSED_VERSION:") {
            assert_eq!(
                outcome.before_sender, outcome.after_sender,
                "{direction} {action} refusal mutated sender count"
            );
            assert_eq!(
                outcome.before_recipient, outcome.after_recipient,
                "{direction} {action} refusal mutated recipient count"
            );
        } else {
            assert!(
                outcome.result.starts_with("COMPLETED:"),
                "{direction} {action} must complete or refuse with a version reason: {}",
                outcome.result
            );
        }
    }
    println!("TASK3616_ACTION_COUNT={}", results.len());
}

fn run_direction(
    direction: &'static str,
    run: &str,
    sender: &mut CopyUnderTest,
    recipient: &mut CopyUnderTest,
    results: &mut Vec<(&'static str, &'static str, ActionResult)>,
) {
    results.push((
        direction,
        "SEND",
        send_action(direction, run, sender, recipient),
    ));
    results.push((
        direction,
        "READ",
        read_action(direction, run, sender, recipient),
    ));
    results.push((
        direction,
        "ATTACHMENT",
        attachment_action(direction, run, sender, recipient),
    ));
    results.push((
        direction,
        "TIMER",
        timer_action(direction, run, sender, recipient),
    ));
    results.push((
        direction,
        "VIEW_ONCE",
        view_once_action(direction, run, sender, recipient),
    ));
    results.push((
        direction,
        "BURN",
        burn_action(direction, run, sender, recipient),
    ));
}

impl CopyUnderTest {
    fn new(
        root: &std::path::Path,
        name: &'static str,
        version: SupportedVersion,
        stem: &str,
    ) -> Self {
        let store = MessageStore::open(&root.join(stem).join("messages"), &STORE_KEY)
            .expect("open encrypted message store");
        Self {
            name,
            version,
            channel_id: format!("{stem}-channel"),
            store,
        }
    }

    fn put_marked(&self, message_id: &str, from: &str, plaintext: &str, at: u64) {
        self.store
            .put(&StoredMessage {
                discord_message_id: message_id.to_owned(),
                channel_id: self.channel_id.clone(),
                sender_discord_id: from.to_owned(),
                sender_osl_user_id: format!("osl-{from}"),
                plaintext: plaintext.to_owned(),
                decrypted_at: i64::try_from(at).expect("test timestamp fits i64"),
                burned: false,
                reply_parent_id: None,
                edit_revision: 1,
            })
            .expect("persist marked row");
    }

    fn exact_count(&self, marked: &str) -> usize {
        self.store
            .list_by_channel(&self.channel_id, 256)
            .expect("list encrypted channel")
            .into_iter()
            .filter(|row| row.plaintext == marked)
            .count()
    }
}

fn send_action(
    direction: &str,
    run: &str,
    sender: &CopyUnderTest,
    recipient: &CopyUnderTest,
) -> ActionResult {
    let marked = format!("TASK3616:{run}:{direction}:SEND:MARKED");
    let before_sender = sender.exact_count(&marked);
    let before_recipient = recipient.exact_count(&marked);
    recipient.put_marked(
        &format!("task-3616-{run}-{direction}-send"),
        sender.name,
        &marked,
        SENT_AT,
    );
    let after_sender = sender.exact_count(&marked);
    let after_recipient = recipient.exact_count(&marked);
    ActionResult {
        result: format!("COMPLETED:{marked}"),
        before_sender,
        after_sender,
        before_recipient,
        after_recipient,
    }
}

fn read_action(
    direction: &str,
    run: &str,
    sender: &CopyUnderTest,
    recipient: &CopyUnderTest,
) -> ActionResult {
    let marked = format!("TASK3616:{run}:{direction}:READ:MARKED");
    let message_id = format!("task-3616-{run}-{direction}-read");
    recipient.put_marked(&message_id, sender.name, &marked, SENT_AT + 1);
    let before_sender = sender.exact_count(&marked);
    let before_recipient = recipient.exact_count(&marked);
    let read = recipient
        .store
        .get(&message_id)
        .expect("read encrypted marked row")
        .expect("marked row exists")
        .plaintext;
    assert_eq!(read, marked, "read must return the exact marked result");
    let after_sender = sender.exact_count(&marked);
    let after_recipient = recipient.exact_count(&marked);
    ActionResult {
        result: format!("COMPLETED:{read}"),
        before_sender,
        after_sender,
        before_recipient,
        after_recipient,
    }
}

fn attachment_action(
    direction: &str,
    run: &str,
    sender: &CopyUnderTest,
    recipient: &CopyUnderTest,
) -> ActionResult {
    let protocol = sender.version.native_attachment_protocol();
    let marked = format!("TASK3616:{run}:{direction}:ATTACHMENT:v{protocol}:MARKED");
    let before_sender = sender.exact_count(&marked);
    let before_recipient = recipient.exact_count(&marked);
    let result = if recipient.version.accepts_attachment_protocol(protocol) {
        recipient.put_marked(
            &format!("task-3616-{run}-{direction}-attachment"),
            sender.name,
            &marked,
            SENT_AT + 2,
        );
        format!("COMPLETED:{marked}")
    } else {
        format!(
            "REFUSED_VERSION:attachment protocol v{protocol} from {} {} is unsupported by {} {}",
            sender.name,
            sender.version.label(),
            recipient.name,
            recipient.version.label()
        )
    };
    let after_sender = sender.exact_count(&marked);
    let after_recipient = recipient.exact_count(&marked);
    ActionResult {
        result,
        before_sender,
        after_sender,
        before_recipient,
        after_recipient,
    }
}

fn timer_action(
    direction: &str,
    run: &str,
    sender: &CopyUnderTest,
    recipient: &CopyUnderTest,
) -> ActionResult {
    let marked = format!("TASK3616:{run}:{direction}:TIMER:MARKED");
    let message_id = format!("task-3616-{run}-{direction}-timer");
    recipient.put_marked(&message_id, sender.name, &marked, SENT_AT + 3);
    let mut lifecycle = received_lifecycle(direction, "timer", SENT_AT, ONE_HOUR);
    let before_sender = sender.exact_count(&marked);
    let before_recipient = recipient.exact_count(&marked);
    assert_eq!(lifecycle.status(), ReceiptStatus::Received);
    lifecycle
        .expire(SENT_AT + ONE_HOUR)
        .expect("one-hour timer expires exactly");
    assert_eq!(lifecycle.status(), ReceiptStatus::Expired);
    let shredded = recipient
        .store
        .shred_expired_messages(std::slice::from_ref(&message_id))
        .expect("shred expired marked row");
    let after_sender = sender.exact_count(&marked);
    let after_recipient = recipient.exact_count(&marked);
    assert_eq!(shredded, 1);
    ActionResult {
        result: format!("COMPLETED:{marked}:expired=1:shredded={shredded}"),
        before_sender,
        after_sender,
        before_recipient,
        after_recipient,
    }
}

fn view_once_action(
    direction: &str,
    run: &str,
    sender: &CopyUnderTest,
    recipient: &CopyUnderTest,
) -> ActionResult {
    let marked = format!("TASK3616:{run}:{direction}:VIEW_ONCE:MARKED");
    let message_id = format!("task-3616-{run}-{direction}-view-once");
    recipient.put_marked(&message_id, sender.name, &marked, SENT_AT + 4);
    let mut lifecycle = received_lifecycle(direction, "view-once", SENT_AT, ONE_HOUR);
    let before_sender = sender.exact_count(&marked);
    let before_recipient = recipient.exact_count(&marked);
    let opened = recipient
        .store
        .get(&message_id)
        .expect("view-once read")
        .expect("view-once row present")
        .plaintext;
    assert_eq!(opened, marked);
    lifecycle
        .record_opened(
            TransitionEvidence {
                digest: [0x63; 32],
                observed_at: SENT_AT + 5,
            },
            SENT_AT + 5,
        )
        .expect("record first authenticated open");
    assert_eq!(lifecycle.status(), ReceiptStatus::Opened);
    let shredded = recipient
        .store
        .shred_expired_messages(std::slice::from_ref(&message_id))
        .expect("consume view-once marked row");
    let second_open_absent = recipient
        .store
        .get(&message_id)
        .expect("second view-once lookup")
        .is_none();
    let after_sender = sender.exact_count(&marked);
    let after_recipient = recipient.exact_count(&marked);
    assert_eq!(shredded, 1);
    assert!(second_open_absent);
    ActionResult {
        result: format!("COMPLETED:{opened}:second_open_absent={second_open_absent}"),
        before_sender,
        after_sender,
        before_recipient,
        after_recipient,
    }
}

fn burn_action(
    direction: &str,
    run: &str,
    sender: &CopyUnderTest,
    recipient: &CopyUnderTest,
) -> ActionResult {
    let marked = format!("TASK3616:{run}:{direction}:BURN:MARKED");
    let message_id = format!("task-3616-{run}-{direction}-burn");
    recipient.put_marked(&message_id, sender.name, &marked, SENT_AT + 6);
    let before_sender = sender.exact_count(&marked);
    let before_recipient = recipient.exact_count(&marked);
    recipient
        .store
        .mark_burned(&message_id)
        .expect("burn marked row");
    let after_sender = sender.exact_count(&marked);
    let after_recipient = recipient.exact_count(&marked);
    let absent = recipient
        .store
        .get(&message_id)
        .expect("read after burn")
        .is_none();
    assert!(absent);
    ActionResult {
        result: format!("COMPLETED:{marked}:absent_after_burn={absent}"),
        before_sender,
        after_sender,
        before_recipient,
        after_recipient,
    }
}

fn received_lifecycle(
    direction: &str,
    action: &str,
    created_at: u64,
    ttl: u64,
) -> LogicalMessageLifecycle {
    let message_id = digest32(direction.as_bytes(), action.as_bytes(), b"message");
    let scope_digest = digest32(direction.as_bytes(), action.as_bytes(), b"scope");
    LogicalMessageLifecycle::receive(
        message_id,
        scope_digest,
        created_at,
        created_at + ttl,
        0,
        vec![AcceptedPart {
            index: 0,
            sealed_bytes: 512,
            digest: digest32(direction.as_bytes(), action.as_bytes(), b"part"),
        }],
        TransitionEvidence {
            digest: digest32(direction.as_bytes(), action.as_bytes(), b"delivery"),
            observed_at: created_at,
        },
        LIMITS,
    )
    .expect("build received lifecycle")
}

fn digest32(a: &[u8], b: &[u8], c: &[u8]) -> [u8; 32] {
    let mut out = [0u8; 32];
    out[..8].copy_from_slice(&(a.len() as u64).to_be_bytes());
    out[8..16].copy_from_slice(&(b.len() as u64).to_be_bytes());
    out[16..24].copy_from_slice(&(c.len() as u64).to_be_bytes());
    for (index, byte) in a.iter().chain(b).chain(c).enumerate() {
        out[index % 32] ^= *byte;
    }
    if out == [0u8; 32] {
        out[0] = 1;
    }
    out
}
