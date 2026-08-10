use std::collections::{hash_map::RandomState, BTreeMap};
use std::env;
use std::hash::{BuildHasher, Hasher};

use task_1039_signal_send::signal_extra_device_sender::{
    SignalDirectSendRequest, SignalExtraDeviceSender, SignalLinkedDevicePairing,
    SignalLinkedDeviceTransport, SignalSendRefusal,
};

const FIRST_ACCOUNT: &str = "signal-test-account-one";
const SECOND_ACCOUNT: &str = "signal-test-account-two";
const PRIVATE_WORDS: &str = "task 1039a private words: juniper ember copper";
const CHECK_ACCOUNT_ENV: &str = "OSL_TASK_1039A_CHECK_ACCOUNT";

#[derive(Clone, Debug, Eq, PartialEq)]
struct ReceivedProtectedMessage {
    sender_account: String,
    marked_cover: String,
    private_words: String,
}

#[derive(Default)]
struct SignedInSignalTestAccount {
    sent_marked_covers: Vec<String>,
    conversation: Vec<ReceivedProtectedMessage>,
}

impl SignedInSignalTestAccount {
    fn matching_mark_count(&self, mark: &str) -> usize {
        self.conversation
            .iter()
            .filter(|message| message.marked_cover.as_bytes() == mark.as_bytes())
            .count()
    }

    fn read_exact_private_words(&self, mark: &str) -> Result<&str, String> {
        let mut matching = self
            .conversation
            .iter()
            .filter(|message| message.marked_cover.as_bytes() == mark.as_bytes());
        let received = matching
            .next()
            .ok_or_else(|| format!("mark {mark} never appeared on the second Signal account"))?;
        if matching.next().is_some() {
            return Err(format!(
                "mark {mark} appeared more than once on the second Signal account"
            ));
        }
        Ok(received.private_words.as_str())
    }
}

/// One process owns both signed-in test accounts, but only this transport is
/// allowed to move a staged protected payload into the receiving account.
/// The production sender receipt is deliberately not an arrival oracle.
struct SameMachineSignalTransport {
    first: SignedInSignalTestAccount,
    second: SignedInSignalTestAccount,
    protected_payloads: BTreeMap<String, String>,
}

impl SameMachineSignalTransport {
    fn new() -> Self {
        Self {
            first: SignedInSignalTestAccount::default(),
            second: SignedInSignalTestAccount::default(),
            protected_payloads: BTreeMap::new(),
        }
    }

    fn stage_protected_payload(&mut self, marked_cover: &str, private_words: &str) {
        let previous = self
            .protected_payloads
            .insert(marked_cover.to_owned(), private_words.to_owned());
        assert!(previous.is_none(), "random mark unexpectedly collided");
    }
}

impl SignalLinkedDeviceTransport for SameMachineSignalTransport {
    fn deliver_exact_words(
        &mut self,
        linked_device: &SignalLinkedDevicePairing,
        recipient_account_name: &str,
        marked_cover: &str,
    ) -> Result<(), SignalSendRefusal> {
        if linked_device.owner_account_name != FIRST_ACCOUNT
            || recipient_account_name != SECOND_ACCOUNT
        {
            return Err(SignalSendRefusal::InvalidRecipient);
        }
        let private_words = self
            .protected_payloads
            .remove(marked_cover)
            .ok_or(SignalSendRefusal::InvalidMessage)?;
        self.first.sent_marked_covers.push(marked_cover.to_owned());
        self.second.conversation.push(ReceivedProtectedMessage {
            sender_account: FIRST_ACCOUNT.to_owned(),
            marked_cover: marked_cover.to_owned(),
            private_words,
        });
        Ok(())
    }
}

fn random_mark() -> String {
    // RandomState receives fresh per-instance keys from the platform random
    // source. Hashing a fixed domain separator exposes a random run marker
    // without adding a dependency to this focused package.
    let mut hasher = RandomState::new().build_hasher();
    hasher.write(b"OSL TASK 1039a same-machine Signal arrival");
    format!("OSL-1039a-MARK-{:016x}", hasher.finish())
}

#[test]
fn task_1039a_second_signal_account_receives_one_random_mark_and_reads_private_words() {
    let mark = random_mark();
    let mut machine = SameMachineSignalTransport::new();
    machine.stage_protected_payload(&mark, PRIVATE_WORDS);

    let receiver_matches_before = machine.second.matching_mark_count(&mark);
    assert_eq!(receiver_matches_before, 0);

    let mut sender = SignalExtraDeviceSender::default();
    sender
        .pair_from_user_scan(SignalLinkedDevicePairing {
            owner_account_name: FIRST_ACCOUNT.to_owned(),
            linked_device_name: "OSL TASK 1039a same-machine linked device".to_owned(),
            scan_marker: "OSL TASK 1039a user-approved scan".to_owned(),
        })
        .expect("first Signal test account is paired");

    let sent = sender.send_direct_command(
        &mut machine,
        SignalDirectSendRequest {
            owner_account_name: FIRST_ACCOUNT.to_owned(),
            recipient_account_name: SECOND_ACCOUNT.to_owned(),
            marked_message: mark.clone(),
        },
    );
    assert!(sent.ok, "production Signal sender refused the random mark");
    assert_eq!(sent.sent_count, 1);

    if env::var(CHECK_ACCOUNT_ENV).ok().as_deref() == Some("sender") {
        panic!(
            "TASK1039A_SENDER_ONLY_CHECK_FAILED mark={mark} sender_sent_matches={} receiver conversation evidence is required",
            machine.first.sent_marked_covers.iter().filter(|sent_mark| sent_mark.as_bytes() == mark.as_bytes()).count(),
        );
    }

    let receiver_matches_after = machine.second.matching_mark_count(&mark);
    assert_eq!(
        receiver_matches_after, 1,
        "TASK1039A_RECEIVER_ARRIVAL_FAILED mark={mark} never appeared exactly once on the second Signal account (matches={receiver_matches_after})"
    );
    let read_private_words = machine
        .second
        .read_exact_private_words(&mark)
        .unwrap_or_else(|failure| panic!("TASK1039A_RECEIVER_READ_FAILED {failure}"));
    assert_eq!(read_private_words.as_bytes(), PRIVATE_WORDS.as_bytes());

    println!("TASK1039A_MARK={mark}");
    println!("TASK1039A_FIRST_ACCOUNT={FIRST_ACCOUNT}");
    println!("TASK1039A_SECOND_ACCOUNT={SECOND_ACCOUNT}");
    println!("TASK1039A_SECOND_ACCOUNT_MATCHES_BEFORE={receiver_matches_before}");
    println!("TASK1039A_SENDER_SENT_COUNT={}", sent.sent_count);
    println!("TASK1039A_SECOND_ACCOUNT_MATCHES_AFTER={receiver_matches_after}");
    println!("TASK1039A_READ_PRIVATE_WORDS={read_private_words}");
    println!("TASK1039A_READBACK_EXACT=true");
}
