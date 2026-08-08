//! Two-account X DM cover/reveal proof.
//!
//! This is deliberately a two-account machine fixture, not a parallel send
//! model: Alice sends through the shipping X web adapter's placement and
//! commit seam, while Bob's independent receiving job must decode the exact
//! marked cover before it records any private words.

use osl_privacy_hub::adapters::*;
use osl_privacy_hub::web_surface_adapter::x::{
    XConversationHeader, XSurfaceDriver, XSurfaceSnapshot, XTranscriptRow, XWebBackend,
};
use osl_privacy_hub::web_surface_adapter::WebSurfaceAdapter;
use std::collections::BTreeMap;
use std::env;
use std::sync::{Arc, Mutex};
use stego::{decode_mode1, encode_mode1, ConversationCipher};

const ALICE: &str = "task-1110-alice-x-test";
const BOB: &str = "task-1110-bob-x-test";
const PRIVATE_WORDS: &str = "private X words: amber, cypress, quartz.";
const COVER_MARK: &str = "TASK1110-X-DM-MARK::";
const SCOPE: &str = "task-1110-x-dm-scope";

#[derive(Default)]
struct XAccount {
    snapshot: Option<XSurfaceSnapshot>,
    incoming_covers: Vec<String>,
    private_messages_read: Vec<String>,
}

struct XMachine {
    accounts: Mutex<BTreeMap<&'static str, XAccount>>,
}

impl XMachine {
    fn two_test_accounts() -> Arc<Self> {
        let machine = Arc::new(Self {
            accounts: Mutex::new(BTreeMap::new()),
        });
        let mut accounts = machine.accounts.lock().expect("X machine lock");
        accounts.insert(
            ALICE,
            XAccount {
                snapshot: Some(snapshot(ALICE, BOB)),
                ..XAccount::default()
            },
        );
        accounts.insert(
            BOB,
            XAccount {
                snapshot: Some(snapshot(BOB, ALICE)),
                ..XAccount::default()
            },
        );
        drop(accounts);
        machine
    }

    fn snapshot(&self, account: &'static str) -> XSurfaceSnapshot {
        self.accounts
            .lock()
            .expect("X machine lock")
            .get(account)
            .and_then(|account| account.snapshot.clone())
            .expect("test X account snapshot")
    }

    fn private_read_count(&self, account: &'static str) -> usize {
        self.accounts
            .lock()
            .expect("X machine lock")
            .get(account)
            .expect("test X account")
            .private_messages_read
            .len()
    }
}

struct XTestAccountDriver {
    machine: Arc<XMachine>,
    account: &'static str,
    recipient: &'static str,
}

impl XTestAccountDriver {
    fn for_account(machine: Arc<XMachine>, account: &'static str, recipient: &'static str) -> Self {
        Self {
            machine,
            account,
            recipient,
        }
    }
}

impl XSurfaceDriver for XTestAccountDriver {
    fn capabilities(&self) -> CapabilitySet {
        [
            adapter_profile::Capability::PlaceProtectedPayload,
            adapter_profile::Capability::SendProtectedPayload,
        ]
        .into_iter()
        .collect()
    }

    fn is_current_generation(&self, generation: u64) -> bool {
        generation == 1_110
    }

    fn wake_accessibility(&self) -> Result<(), AdapterRefusal> {
        Ok(())
    }

    fn snapshot(&self) -> Result<XSurfaceSnapshot, AdapterRefusal> {
        Ok(self.machine.snapshot(self.account))
    }

    fn place_with_vm_attestation(&self, carrier: &str) -> Result<(), AdapterRefusal> {
        let mut accounts = self.machine.accounts.lock().expect("X machine lock");
        accounts
            .get_mut(self.account)
            .expect("sender account")
            .snapshot
            .as_mut()
            .expect("sender snapshot")
            .composer_text = carrier.to_owned();
        Ok(())
    }

    fn commit_with_vm_attestation(&self) -> Result<SendOutcome, AdapterRefusal> {
        let mut accounts = self.machine.accounts.lock().expect("X machine lock");
        let carrier = {
            let sender = accounts.get_mut(self.account).expect("sender account");
            let snapshot = sender.snapshot.as_mut().expect("sender snapshot");
            let carrier = std::mem::take(&mut snapshot.composer_text);
            snapshot.rows.push(XTranscriptRow {
                rect: Bounds {
                    x: 1,
                    y: 20,
                    width: 300,
                    height: 24,
                },
                carrier: Some(carrier.clone()),
            });
            carrier
        };
        accounts
            .get_mut(self.recipient)
            .expect("recipient account")
            .incoming_covers
            .push(carrier);
        Ok(SendOutcome::Sent)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ReceivingJob {
    Active,
    StubbedToDoNothing,
}

impl ReceivingJob {
    fn selected_for_this_run() -> Self {
        if env::var_os("OSL_X_RECEIVING_JOB_STUB").is_some() {
            Self::StubbedToDoNothing
        } else {
            Self::Active
        }
    }
}

#[derive(Debug)]
struct ReceiveCheck {
    arriving_covers: Vec<String>,
    private_words_read: Vec<String>,
    failures: Vec<&'static str>,
}

/// The receiving-job boundary.  In particular, a cover in Bob's X inbox is
/// only transport evidence; it becomes a private-message read after this job
/// authenticates the marker and decodes it.
fn receive_x_direct_messages(
    machine: &XMachine,
    recipient: &'static str,
    cipher: &ConversationCipher,
    job: ReceivingJob,
) -> ReceiveCheck {
    let mut accounts = machine.accounts.lock().expect("X machine lock");
    let account = accounts.get_mut(recipient).expect("recipient account");
    let arriving_covers = account.incoming_covers.clone();
    let before = account.private_messages_read.len();

    if job == ReceivingJob::Active {
        for cover in &arriving_covers {
            let Some(encoded_cover) = cover.strip_prefix(COVER_MARK) else {
                continue;
            };
            let Ok(private_words) = decode_mode1(cipher, encoded_cover) else {
                continue;
            };
            let Ok(private_words) = String::from_utf8(private_words) else {
                continue;
            };
            account.private_messages_read.push(private_words);
        }
    }

    let private_words_read = account.private_messages_read[before..].to_vec();
    let failures = if !arriving_covers.is_empty() && private_words_read.is_empty() {
        // A cover arriving by itself is explicitly a red result.  It cannot
        // stand in for the required reveal on the second account.
        vec!["cover_arrived_without_private_words"]
    } else {
        Vec::new()
    };
    ReceiveCheck {
        arriving_covers,
        private_words_read,
        failures,
    }
}

fn profile() -> adapter_profile::ProfilePayload {
    adapter_profile::ProfilePayload {
        domain: "osl/adapter-profile/v1".into(),
        schema_version: 1,
        adapter_id: "x.web.task-1110".into(),
        app: adapter_profile::AppDescriptor {
            stable_id: "x".into(),
            display_name: "X".into(),
            service_family: "messaging".into(),
            min_app_version: None,
        },
        revision: adapter_profile::ProfileRevision {
            number: 1,
            label: "task-1110".into(),
        },
        issued_at_unix_seconds: 1,
        expires_at_unix_seconds: u64::MAX,
        support: adapter_profile::SupportLevel::Supported,
        authority: adapter_profile::AuthorityRequirements {
            user_consent_required: true,
            account_binding_required: true,
            release_authority_required: true,
            harmless_canary_required: true,
        },
        selectors: vec![],
        fallbacks: vec![],
        canary: adapter_profile::HarmlessCanary {
            selector: adapter_profile::SelectorKind::AppRoot,
            expected_text: "Messages".into(),
            max_age_seconds: 1,
        },
    }
}

fn snapshot(account: &str, recipient: &str) -> XSurfaceSnapshot {
    XSurfaceSnapshot {
        generation: 1_110,
        composer: NodeRef::for_claimed_node(1),
        transcript: Some(NodeRef::for_claimed_node(2)),
        bounds: Bounds {
            x: 0,
            y: 0,
            width: 640,
            height: 480,
        },
        bound_at_ms: 1_110,
        scope_binding_hash: SCOPE.into(),
        composer_text: String::new(),
        composer_is_password_field: false,
        focused: true,
        occluded: false,
        read_was_complete: true,
        header: XConversationHeader {
            account_label: Some(account.into()),
            conversation_label: Some(format!("DM with {recipient}")),
            recipient_labels: vec![recipient.into()],
        },
        transcript_epoch: 1_110,
        rows: Vec::new(),
    }
}

#[test]
fn task_1110_second_x_account_reads_exact_private_words_from_marked_dm_cover() {
    let machine = XMachine::two_test_accounts();
    let cipher = ConversationCipher::from_salt(b"osl/task-1110/two-x-test-accounts/v1");
    let marked_cover = format!(
        "{COVER_MARK}{}",
        encode_mode1(&cipher, PRIVATE_WORDS.as_bytes()).expect("private words fit the cover")
    );
    let sender = WebSurfaceAdapter::new(
        AdapterAppId::X,
        profile(),
        XWebBackend::new(XTestAccountDriver::for_account(machine.clone(), ALICE, BOB)),
    );
    let target = SurfaceTarget {
        app: AdapterAppId::X,
        surface: SurfaceKind::FixedOfficialWebOrigin,
        generation: 1_110,
    };
    let binding = sender.locate(&target).expect("Alice's X DM is bound");
    let private_reads_before = machine.private_read_count(BOB);
    assert_eq!(
        private_reads_before, 0,
        "Bob starts with no private-message reads"
    );

    let carrier = Carrier(marked_cover.clone());
    let placed = sender.place(
        &binding,
        &PlacementAuthorization::for_scope(SCOPE),
        &carrier,
    );
    assert_eq!(
        placed.status,
        PlacementStatus::Placed,
        "marked cover is placed, not sent"
    );
    let sent = sender.commit(&binding, &SendAuthorization::for_scope(SCOPE), &placed);
    assert_eq!(
        sent.outcome,
        SendOutcome::Sent,
        "Alice sends exactly one X DM cover"
    );

    let job = ReceivingJob::selected_for_this_run();
    let received = receive_x_direct_messages(&machine, BOB, &cipher, job);
    let private_reads_after = machine.private_read_count(BOB);

    println!("TASK1110 sender_account={ALICE} receiver_account={BOB}");
    println!("TASK1110 marked_cover_exact={marked_cover}");
    println!("TASK1110 private_messages_read_before={private_reads_before}");
    println!(
        "TASK1110 cover_arrival_count={}",
        received.arriving_covers.len()
    );
    println!(
        "TASK1110 cover_arrived_exact={}",
        received
            .arriving_covers
            .first()
            .map(String::as_str)
            .unwrap_or("<none>")
    );
    println!("TASK1110 receiving_job={job:?}");
    println!("TASK1110 private_messages_read_after={private_reads_after}");
    println!(
        "TASK1110 private_words_exact={}",
        received
            .private_words_read
            .first()
            .map(String::as_str)
            .unwrap_or("<none>")
    );
    println!("TASK1110 failures={:?}", received.failures);

    assert_eq!(
        received.arriving_covers,
        vec![marked_cover],
        "Bob receives the exact marked cover"
    );
    assert!(
        received.failures.is_empty(),
        "a cover without a reveal is recorded as a failure: {:?}",
        received.failures
    );
    assert_eq!(
        private_reads_after, 1,
        "Bob reads exactly one private message after the receiving job"
    );
    assert_eq!(
        received.private_words_read,
        vec![PRIVATE_WORDS],
        "Bob reads back the exact private words"
    );
}
