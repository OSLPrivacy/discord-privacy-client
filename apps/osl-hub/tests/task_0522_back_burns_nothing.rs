use std::cell::RefCell;
use std::fs;
use std::path::{Path, PathBuf};

use osl_privacy_hub::burn_review_state::BurnReviewState;
use sha2::{Digest, Sha256};
use store::{MessageStore, StoredMessage};
use tempfile::TempDir;

const MESSAGE_NAME: &str = "FIRE-0522";
const BURN_MARK: &str = "BURN-0522";
const CHANNEL: &str = "chat:fire-0522";
const SELF_SENDER: &str = "self-0522";
const PEER_SENDER: &str = "peer-0522";

struct Scenario {
    _tmp: TempDir,
    store_dir: PathBuf,
    mark_path: PathBuf,
    store: RefCell<MessageStore>,
    review: BurnReviewState,
}

impl Scenario {
    fn fresh(label: &str) -> Self {
        let tmp = TempDir::new().expect("tempdir");
        let store_dir = tmp.path().join(format!("store-{label}"));
        let review_path = tmp.path().join(format!("review-{label}.json"));
        let mark_path = tmp.path().join(format!("mark-{label}.txt"));
        let store = MessageStore::open(&store_dir, &[0x52; 32]).expect("open store");
        let scenario = Self {
            _tmp: tmp,
            store_dir,
            mark_path,
            store: RefCell::new(store),
            review: BurnReviewState::load(review_path),
        };
        scenario.seed();
        scenario
            .review
            .save_command("your_side".to_owned(), CHANNEL.to_owned(), true)
            .expect("save review");
        scenario
    }

    fn seed(&self) {
        let store = self.store.borrow();
        for (message_id, sender, text) in [
            ("fire-0522-self", SELF_SENDER, "FIRE-0522 from this account"),
            (
                "fire-0522-peer",
                PEER_SENDER,
                "FIRE-0522 from the other account",
            ),
        ] {
            store
                .put(&StoredMessage {
                    discord_message_id: message_id.to_owned(),
                    channel_id: CHANNEL.to_owned(),
                    sender_discord_id: sender.to_owned(),
                    sender_osl_user_id: sender.to_owned(),
                    plaintext: text.to_owned(),
                    decrypted_at: if sender == SELF_SENDER { 2 } else { 1 },
                    burned: false,
                })
                .expect("seed two-sided FIRE-0522 row");
        }
    }

    fn count(&self) -> usize {
        self.store
            .borrow()
            .list_by_channel(CHANNEL, 10)
            .expect("read channel")
            .len()
    }

    fn readable_summary(&self) -> String {
        let mut rows = self
            .store
            .borrow()
            .list_by_channel(CHANNEL, 10)
            .expect("read channel");
        rows.sort_by(|a, b| a.discord_message_id.cmp(&b.discord_message_id));
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().all(|row| row.plaintext.contains(MESSAGE_NAME)));
        assert!(rows.iter().any(|row| row.sender_discord_id == SELF_SENDER));
        assert!(rows.iter().any(|row| row.sender_discord_id == PEER_SENDER));
        rows.iter()
            .map(|row| format!("{}:{}", row.discord_message_id, row.sender_discord_id))
            .collect::<Vec<_>>()
            .join(",")
    }

    fn fingerprint(&self) -> String {
        fingerprint_store_files(&self.store_dir)
    }

    fn burn_mark(&self) -> Option<String> {
        fs::read_to_string(&self.mark_path).ok()
    }
}

fn fingerprint_store_files(store_dir: &Path) -> String {
    let mut files: Vec<PathBuf> = fs::read_dir(store_dir)
        .expect("read store dir")
        .map(|entry| entry.expect("dir entry").path())
        .filter(|path| path.is_file())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("messages.sqlite"))
        })
        .collect();
    files.sort();

    let mut hash = Sha256::new();
    for path in files {
        let name = path.file_name().unwrap().to_string_lossy();
        hash.update(name.as_bytes());
        hash.update([0]);
        hash.update(fs::read(&path).expect("read store file"));
        hash.update([0xff]);
    }
    format!("{:x}", hash.finalize())
}

#[test]
fn task_0522_back_and_unknown_choices_burn_nothing() {
    let good = Scenario::fresh("confirm");
    let back = Scenario::fresh("back");
    let unknown = Scenario::fresh("unknown");

    let good_readable = good.readable_summary();
    let back_readable = back.readable_summary();
    let unknown_readable = unknown.readable_summary();
    println!("TASK0522_READABLE_CONFIRM={good_readable}");
    println!("TASK0522_READABLE_BACK={back_readable}");
    println!("TASK0522_READABLE_UNKNOWN={unknown_readable}");

    let good_before_count = good.count();
    let back_before_count = back.count();
    let unknown_before_count = unknown.count();
    println!(
        "TASK0522_COUNTS_BEFORE confirm={good_before_count} back={back_before_count} unknown={unknown_before_count}"
    );
    assert_eq!(good_before_count, 2);
    assert_eq!(back_before_count, 2);
    assert_eq!(unknown_before_count, 2);

    let back_fingerprint_before = back.fingerprint();
    let unknown_fingerprint_before = unknown.fingerprint();

    let good_result = good
        .review
        .final_choice_command_checked(
            "CONFIRM",
            MESSAGE_NAME.to_owned(),
            BURN_MARK.to_owned(),
            |message, mark| {
                assert_eq!(message, MESSAGE_NAME);
                fs::write(&good.mark_path, mark).expect("write burn mark");
                good.store
                    .borrow()
                    .delete_messages_in_channel(CHANNEL)
                    .expect("delete channel")
            },
            |message, mark| {
                assert_eq!(message, MESSAGE_NAME);
                assert_eq!(mark, BURN_MARK);
                0
            },
        )
        .expect("confirm burns reviewed message");
    let good_after_count = good.count();
    let good_mark = good.burn_mark().expect("good burn mark");
    println!(
        "TASK0522_CONFIRM status={} reviewed_message={} count_after={} burn_mark={}",
        good_result.status, good_result.reviewed_message, good_after_count, good_mark
    );
    assert_eq!(good_result.status, "burn confirmed");
    assert_eq!(good_result.reviewed_message, MESSAGE_NAME);
    assert_eq!(good_result.burn_mark, BURN_MARK);
    assert_eq!(good_result.local_removal_count, 2);
    assert_eq!(good_after_count, 0);
    assert_eq!(good_mark, BURN_MARK);

    let back_result = back
        .review
        .final_choice_command_checked(
            "BACK",
            MESSAGE_NAME.to_owned(),
            BURN_MARK.to_owned(),
            |_, _| {
                panic!("BACK must not issue the local burn");
            },
            |_, _| {
                panic!("BACK must not issue the remote burn");
            },
        )
        .expect("back cancels");
    let back_after_count = back.count();
    let back_fingerprint_after = back.fingerprint();
    println!(
        "TASK0522_BACK status={} count_after={} fingerprint_before={} fingerprint_after={}",
        back_result.status, back_after_count, back_fingerprint_before, back_fingerprint_after
    );
    assert_eq!(back_result.status, "burn cancelled");
    assert_eq!(back_result.local_removal_count, 0);
    assert_eq!(back_result.remote_removal_count, 0);
    assert_eq!(back_after_count, 2);
    assert_eq!(back_fingerprint_after, back_fingerprint_before);
    assert_eq!(back.burn_mark(), None);

    let unknown_refusal = unknown
        .review
        .final_choice_command_checked(
            "FORWARD",
            MESSAGE_NAME.to_owned(),
            BURN_MARK.to_owned(),
            |_, _| {
                panic!("unknown choice must not issue the local burn");
            },
            |_, _| {
                panic!("unknown choice must not issue the remote burn");
            },
        )
        .expect_err("unknown choice is refused");
    let unknown_after_count = unknown.count();
    let unknown_fingerprint_after = unknown.fingerprint();
    println!(
        "TASK0522_UNKNOWN refusal={unknown_refusal} count_after={} fingerprint_before={} fingerprint_after={}",
        unknown_after_count, unknown_fingerprint_before, unknown_fingerprint_after
    );
    assert_eq!(unknown_refusal, "unknown burn choice");
    assert_eq!(unknown_after_count, 2);
    assert_eq!(unknown_fingerprint_after, unknown_fingerprint_before);
    assert_eq!(unknown.burn_mark(), None);

    println!(
        "TASK0522_GOOD_STILL count={} burn_mark={}",
        good.count(),
        good.burn_mark().expect("good burn mark still present")
    );
    assert_eq!(good.count(), 0);
    assert_eq!(good.burn_mark().as_deref(), Some(BURN_MARK));
}
