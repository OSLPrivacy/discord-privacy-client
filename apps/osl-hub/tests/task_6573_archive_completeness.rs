use osl_privacy_hub::personal_archive::{
    require_export_time_dispositions, verify_packaged_round_trip, AcceptanceEvidence,
    AcceptanceMode, AccountSnapshot, AttachmentRecord, ConversationRecord, DeviceRecord,
    DeviceRecoveryState, ExportDisclosure, FixedOracle, FreshInstall, FriendRecord,
    IdentityAndKeys, MembershipRecord, MessageRecord, SettingsExportSession, DELIBERATE_EXCLUSIONS,
    PAYMENT_EXCLUSION_CLASS, PAYMENT_EXCLUSION_NOTICE, REQUIRED_CLASSES, REQUIRED_JOURNEYS,
    TRANSIENT_RUNTIME_EXCLUSION_CLASS,
};
use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

const PASSPHRASE: &str = "task-6573-caller-held-archive-key";
const FORMAT: &str = "osl-personal-archive-v1";
const KDF_ROUNDS: u32 = 120_000;
const ORIGINAL_AUTHORITY: &str = "original-profile/device-secret";
const UNKNOWN_PRODUCTION_CLASS: &str = "unknown_production_class";

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct KdfSpec {
    algorithm: String,
    rounds: u32,
    salt: Vec<u8>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SealedPart {
    nonce: Vec<u8>,
    ciphertext: Vec<u8>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    format: String,
    kdf: KdfSpec,
    metadata: SealedPart,
    classes: BTreeMap<String, SealedPart>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Metadata {
    deliberate_exclusions: Vec<String>,
    disclosure_notice: String,
    source_dependencies: Vec<String>,
}

#[derive(Clone, Copy)]
enum ClassMutation {
    Omit,
    Corrupt,
    ValidButIncomplete,
}

struct ReaderReport {
    recovered_classes: usize,
    recovered_bytes: usize,
    unreadable_classes: usize,
}

fn fixture() -> AccountSnapshot {
    let owner = "task-6573-owner".to_string();
    AccountSnapshot {
        identity_and_keys: IdentityAndKeys {
            account_id: owner.clone(),
            display_name: "6573 archive owner".to_string(),
            authentication_secret: (1..=33).collect(),
            identity_private_key: vec![2; 64],
            identity_public_key: vec![3; 32],
            message_key: vec![4; 32],
        },
        friends: vec![
            FriendRecord {
                friend_id: "task-6573-friend-one".to_string(),
                owner_account_id: owner.clone(),
                display_name: "Friend one".to_string(),
                public_key: vec![5; 32],
            },
            FriendRecord {
                friend_id: "task-6573-friend-two".to_string(),
                owner_account_id: owner.clone(),
                display_name: "Friend two".to_string(),
                public_key: vec![6; 32],
            },
        ],
        conversations: vec![ConversationRecord {
            conversation_id: "task-6573-conversation".to_string(),
            owner_account_id: owner.clone(),
            member_ids: vec!["task-6573-friend-one".to_string()],
            messages: vec![
                MessageRecord {
                    message_id: "task-6573-message-removed".to_string(),
                    owner_account_id: owner.clone(),
                    author_id: "task-6573-friend-one".to_string(),
                    body: vec![0, 1, 127, 128, 255],
                    sent_at_ms: 6_573,
                },
                MessageRecord {
                    message_id: "task-6573-message-kept".to_string(),
                    owner_account_id: owner.clone(),
                    author_id: owner.clone(),
                    body: vec![9, 8, 7, 6],
                    sent_at_ms: 6_574,
                },
            ],
        }],
        attachments: vec![
            AttachmentRecord {
                attachment_id: "task-6573-attachment-removed".to_string(),
                owner_account_id: owner.clone(),
                message_id: "task-6573-message-kept".to_string(),
                filename: "removed.bin".to_string(),
                media_type: "application/octet-stream".to_string(),
                bytes: vec![255, 0, 99],
            },
            AttachmentRecord {
                attachment_id: "task-6573-attachment-kept".to_string(),
                owner_account_id: owner.clone(),
                message_id: "task-6573-message-kept".to_string(),
                filename: "kept.bin".to_string(),
                media_type: "application/octet-stream".to_string(),
                bytes: vec![3, 2, 1],
            },
        ],
        settings: BTreeMap::from([
            ("setting-6573-a".to_string(), vec![7, 8, 9]),
            ("setting-6573-b".to_string(), vec![10, 11]),
        ]),
        memberships: vec![
            MembershipRecord {
                membership_id: "task-6573-membership-removed".to_string(),
                owner_account_id: owner.clone(),
                role: "owner".to_string(),
                authority_key: vec![10; 32],
            },
            MembershipRecord {
                membership_id: "task-6573-membership-kept".to_string(),
                owner_account_id: owner.clone(),
                role: "member".to_string(),
                authority_key: vec![11; 32],
            },
        ],
        device_recovery: DeviceRecoveryState {
            owner_account_id: owner,
            recovery_secret: vec![12; 32],
            devices: vec![DeviceRecord {
                device_id: "task-6573-device".to_string(),
                public_key: vec![13; 32],
                authorized: true,
            }],
        },
    }
}

fn export_fresh(root: &Path, snapshot: &AccountSnapshot) -> (PathBuf, ExportDisclosure, PathBuf) {
    let source_root = root.join("source-packaged-install");
    std::fs::create_dir_all(&source_root).expect("source install");
    std::fs::write(
        source_root.join("original-only-authority"),
        ORIGINAL_AUTHORITY,
    )
    .expect("original authority sentinel");
    let export_root = root.join("throwaway-export");
    std::fs::create_dir_all(&export_root).expect("export root");
    let archive = export_root.join("account.osl-account");
    let mut settings = SettingsExportSession::new(snapshot.clone());
    let disclosure = settings.disclosure();
    settings
        .record_visible_disclosure(&disclosure.named_exclusions, &disclosure.notice)
        .expect("record complete export-time disclosure");
    settings
        .export_to(&archive, PASSPHRASE)
        .expect("actual packaged export");
    (archive, disclosure, source_root)
}

fn envelope(path: &Path) -> Envelope {
    serde_json::from_slice(&std::fs::read(path).expect("read envelope"))
        .expect("valid outer archive manifest")
}

fn write_envelope(path: &Path, value: &Envelope) {
    std::fs::write(path, serde_json::to_vec(value).expect("encode envelope"))
        .expect("write mutant archive");
    let _: serde_json::Value =
        serde_json::from_slice(&std::fs::read(path).expect("read mutant")).expect("valid manifest");
}

fn derive_key(passphrase: &str, salt: &[u8]) -> crypto::aead::Key {
    let mut digest: [u8; 32] = Sha256::new()
        .chain_update(b"osl-personal-archive-passphrase-v1\0")
        .chain_update(salt)
        .chain_update(passphrase.as_bytes())
        .finalize()
        .into();
    for round in 1..KDF_ROUNDS {
        digest = Sha256::new()
            .chain_update(b"osl-personal-archive-passphrase-v1\0")
            .chain_update(digest)
            .chain_update(salt)
            .chain_update(round.to_le_bytes())
            .chain_update(passphrase.as_bytes())
            .finalize()
            .into();
    }
    crypto::aead::Key::from_bytes(digest)
}

fn associated_data(salt: &[u8], name: &str) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(FORMAT.as_bytes());
    bytes.push(0);
    bytes.extend_from_slice(salt);
    bytes.push(0);
    bytes.extend_from_slice(name.as_bytes());
    bytes
}

fn open_part(key: &crypto::aead::Key, salt: &[u8], name: &str, part: &SealedPart) -> Vec<u8> {
    let nonce: [u8; crypto::aead::NONCE_SIZE] =
        part.nonce.as_slice().try_into().expect("valid nonce");
    crypto::aead::open(
        key,
        &crypto::aead::Nonce::from_bytes(nonce),
        &associated_data(salt, name),
        &part.ciphertext,
    )
    .expect("independent mutation reader authenticates base part")
}

fn seal_plaintext(
    key: &crypto::aead::Key,
    salt: &[u8],
    name: &str,
    plaintext: &[u8],
) -> SealedPart {
    let mut nonce = vec![0u8; crypto::aead::NONCE_SIZE];
    OsRng.fill_bytes(&mut nonce);
    let nonce_array: [u8; crypto::aead::NONCE_SIZE] = nonce.as_slice().try_into().unwrap();
    let ciphertext = crypto::aead::seal(
        key,
        &crypto::aead::Nonce::from_bytes(nonce_array),
        &associated_data(salt, name),
        plaintext,
    )
    .expect("reseal independent mutant");
    SealedPart { nonce, ciphertext }
}

fn first_difference(original: &[u8], changed: &[u8]) -> usize {
    original
        .iter()
        .zip(changed)
        .position(|(left, right)| left != right)
        .unwrap_or_else(|| original.len().min(changed.len()))
}

fn reduced_class(class: &str, source: &AccountSnapshot) -> (Vec<u8>, AccountSnapshot, String) {
    let mut changed = source.clone();
    let (bytes, function) = match class {
        "identity_and_keys" => {
            changed.identity_and_keys.authentication_secret.pop();
            (
                serde_json::to_vec(&changed.identity_and_keys).unwrap(),
                "authenticate_restored_identity",
            )
        }
        "friends" => {
            changed.friends.remove(0);
            (
                serde_json::to_vec(&changed.friends).unwrap(),
                "inspect_exact_state",
            )
        }
        "conversations" => {
            changed.conversations[0].messages.remove(0);
            (
                serde_json::to_vec(&changed.conversations).unwrap(),
                "open_every_message",
            )
        }
        "attachments" => {
            changed.attachments.remove(0);
            (
                serde_json::to_vec(&changed.attachments).unwrap(),
                "open_every_attachment",
            )
        }
        "settings" => {
            changed.settings.remove("setting-6573-a");
            (
                serde_json::to_vec(&changed.settings).unwrap(),
                "inspect_exact_state",
            )
        }
        "memberships" => {
            changed.memberships.remove(0);
            (
                serde_json::to_vec(&changed.memberships).unwrap(),
                "exercise_membership_authority",
            )
        }
        "device_recovery" => {
            changed.device_recovery.recovery_secret.pop();
            (
                serde_json::to_vec(&changed.device_recovery).unwrap(),
                "complete_recovery_device_authority",
            )
        }
        other => panic!("unknown class {other}"),
    };
    (bytes, changed, function.to_string())
}

fn mutate_class(
    path: &Path,
    class: &str,
    mutation: ClassMutation,
    source: &AccountSnapshot,
) -> (usize, Option<AccountSnapshot>, Option<String>) {
    let mut archive = envelope(path);
    let key = derive_key(PASSPHRASE, &archive.kdf.salt);
    match mutation {
        ClassMutation::Omit => {
            archive.classes.remove(class).expect("class to omit");
            write_envelope(path, &archive);
            (0, None, None)
        }
        ClassMutation::Corrupt => {
            archive
                .classes
                .get_mut(class)
                .expect("class to corrupt")
                .ciphertext[0] ^= 0x80;
            write_envelope(path, &archive);
            (0, None, None)
        }
        ClassMutation::ValidButIncomplete => {
            let original = open_part(
                &key,
                &archive.kdf.salt,
                class,
                archive.classes.get(class).expect("class to reduce"),
            );
            let (changed_bytes, changed_snapshot, function) = reduced_class(class, source);
            let offset = first_difference(&original, &changed_bytes);
            archive.classes.insert(
                class.to_string(),
                seal_plaintext(&key, &archive.kdf.salt, class, &changed_bytes),
            );
            write_envelope(path, &archive);
            (offset, Some(changed_snapshot), Some(function))
        }
    }
}

fn mutate_metadata(path: &Path, change: impl FnOnce(&mut Metadata)) {
    let mut archive = envelope(path);
    let key = derive_key(PASSPHRASE, &archive.kdf.salt);
    let plaintext = open_part(&key, &[], "metadata", &archive.metadata);
    let mut metadata: Metadata = serde_json::from_slice(&plaintext).expect("metadata schema");
    change(&mut metadata);
    archive.metadata = seal_plaintext(
        &key,
        &[],
        "metadata",
        &serde_json::to_vec(&metadata).unwrap(),
    );
    write_envelope(path, &archive);
}

fn reader_report(path: &Path) -> ReaderReport {
    let reader = std::env::var_os("TASK6573_READER")
        .map(PathBuf::from)
        .expect("TASK6573_READER points to the built standalone reader");
    let output = Command::new(reader)
        .arg(path)
        .arg(PASSPHRASE)
        .output()
        .expect("run standalone reader");
    assert!(
        output.status.success(),
        "reader failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stderr.is_empty(),
        "reader leaked diagnostics on success"
    );
    let stdout = String::from_utf8(output.stdout).expect("reader UTF-8");
    let line = stdout
        .lines()
        .find(|line| line.starts_with("TASK6573_READER_SUMMARY "))
        .expect("reader summary");
    let value = |name: &str| {
        line.split_whitespace()
            .find_map(|field| field.strip_prefix(&format!("{name}=")))
            .unwrap_or_else(|| panic!("missing reader field {name}"))
            .parse::<usize>()
            .unwrap()
    };
    ReaderReport {
        recovered_classes: value("recovered_classes"),
        recovered_bytes: value("recovered_bytes"),
        unreadable_classes: value("unreadable_classes"),
    }
}

fn exit_one(mutant: &str, result: Result<(), String>, expected: &str) {
    let reason = result.expect_err("mutant must be rejected");
    assert!(reason.contains(expected), "{mutant}: {reason}");
    let output = Command::new(std::env::current_exe().expect("test executable"))
        .args(["--exact", "exit_probe", "--nocapture"])
        .env("OSL_TASK6573_EXIT_MUTANT", mutant)
        .env("OSL_TASK6573_EXIT_REASON", &reason)
        .output()
        .expect("run rejection child");
    assert_eq!(output.status.code(), Some(1), "{mutant} did not exit 1");
    let child = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(child.contains(mutant) && child.contains(&reason));
    println!("TASK6573_EXIT=1 mutant={mutant} reason={reason}");
}

#[test]
fn exit_probe() {
    let Ok(mutant) = std::env::var("OSL_TASK6573_EXIT_MUTANT") else {
        return;
    };
    let reason = std::env::var("OSL_TASK6573_EXIT_REASON").expect("reason");
    eprintln!("TASK6573_REJECT mutant={mutant} reason={reason}");
    std::process::exit(1);
}

fn prove_valid_but_incomplete_failure(
    class: &str,
    fresh: &FreshInstall,
    original: &AccountSnapshot,
    changed: &AccountSnapshot,
    function: &str,
) {
    match class {
        "identity_and_keys" => {
            let error = fresh
                .authenticate(
                    &original.identity_and_keys.account_id,
                    &original.identity_and_keys.authentication_secret,
                    PASSPHRASE,
                )
                .err()
                .expect("original credential must no longer work");
            assert!(error.contains(function));
        }
        _ => {
            let mut account = fresh
                .authenticate(
                    &original.identity_and_keys.account_id,
                    &original.identity_and_keys.authentication_secret,
                    PASSPHRASE,
                )
                .expect("unchanged authentication class");
            match class {
                "friends" => assert_ne!(account.snapshot().friends, original.friends),
                "conversations" => {
                    let error = account
                        .open_message("task-6573-message-removed")
                        .expect_err("removed message");
                    assert!(error.contains(function));
                }
                "attachments" => {
                    let error = account
                        .open_attachment("task-6573-attachment-removed")
                        .expect_err("removed attachment");
                    assert!(error.contains(function));
                }
                "settings" => assert_ne!(account.snapshot().settings, original.settings),
                "memberships" => {
                    let error = account
                        .exercise_membership_authority("task-6573-membership-removed", b"challenge")
                        .expect_err("removed membership authority");
                    assert!(error.contains(function));
                }
                "device_recovery" => {
                    let error = account
                        .authorize_recovery_device(
                            &original.device_recovery.recovery_secret,
                            DeviceRecord {
                                device_id: "cannot-authorize".to_string(),
                                public_key: vec![1; 32],
                                authorized: true,
                            },
                        )
                        .expect_err("removed recovery authority bytes");
                    assert!(error.contains(function));
                }
                _ => unreachable!(),
            }
            assert_eq!(account.snapshot(), changed);
        }
    }
}

fn complete_evidence(
    fresh: &FreshInstall,
    expected: &AccountSnapshot,
    disclosure: ExportDisclosure,
) -> AcceptanceEvidence {
    let mut account = fresh
        .authenticate(
            &expected.identity_and_keys.account_id,
            &expected.identity_and_keys.authentication_secret,
            PASSPHRASE,
        )
        .expect("authenticate restored account");
    let opened_messages = expected
        .conversations
        .iter()
        .flat_map(|conversation| &conversation.messages)
        .map(|message| {
            (
                message.message_id.clone(),
                account
                    .open_message(&message.message_id)
                    .expect("open message"),
            )
        })
        .collect();
    let opened_attachments = expected
        .attachments
        .iter()
        .map(|attachment| {
            (
                attachment.attachment_id.clone(),
                account
                    .open_attachment(&attachment.attachment_id)
                    .expect("open attachment"),
            )
        })
        .collect();
    let conversation = &expected.conversations[0].conversation_id;
    let wire = account
        .send_encrypted_message(
            conversation,
            "task-6573-new-message",
            b"new ciphertext",
            6_575,
        )
        .expect("send encrypted message");
    assert_eq!(
        account
            .receive_encrypted_message(wire)
            .expect("receive encrypted message"),
        b"new ciphertext"
    );
    assert_ne!(
        account
            .exercise_membership_authority(
                &expected.memberships[0].membership_id,
                b"task-6573-challenge",
            )
            .expect("membership authority"),
        [0u8; 32]
    );
    account
        .authorize_recovery_device(
            &expected.device_recovery.recovery_secret,
            DeviceRecord {
                device_id: "task-6573-new-recovery-device".to_string(),
                public_key: vec![77; 32],
                authorized: true,
            },
        )
        .expect("recovery authority");
    AcceptanceEvidence {
        mode: AcceptanceMode::PackagedRoundTrip,
        clean_packaged_install: true,
        original_local_state_available: false,
        source_dependencies: Vec::new(),
        disclosure,
        payment_records_present: false,
        authenticated_account_id: Some(expected.identity_and_keys.account_id.clone()),
        restored_snapshot: Some(expected.clone()),
        opened_messages,
        opened_attachments,
        new_encrypted_message_sent: true,
        new_encrypted_message_received: true,
        membership_authority_exercised: true,
        recovery_device_authority_completed: true,
    }
}

fn actual_failed_journey(
    journey: &str,
    fresh: &FreshInstall,
    expected: &AccountSnapshot,
) -> String {
    if journey == "authenticate_restored_identity" {
        return fresh
            .authenticate(
                &expected.identity_and_keys.account_id,
                b"wrong-authentication-secret",
                PASSPHRASE,
            )
            .err()
            .expect("wrong authentication must fail");
    }
    let mut account = fresh
        .authenticate(
            &expected.identity_and_keys.account_id,
            &expected.identity_and_keys.authentication_secret,
            PASSPHRASE,
        )
        .expect("baseline authentication");
    match journey {
        "inspect_exact_state" => "inspect_exact_state: exact snapshot was not supplied".to_string(),
        "open_every_message" => account.open_message("absent-message").unwrap_err(),
        "open_every_attachment" => account.open_attachment("absent-attachment").unwrap_err(),
        "send_receive_new_encrypted_message" => account
            .send_encrypted_message("absent-conversation", "message", b"body", 1)
            .unwrap_err(),
        "exercise_membership_authority" => account
            .exercise_membership_authority("absent-membership", b"challenge")
            .unwrap_err(),
        "complete_recovery_device_authority" => account
            .authorize_recovery_device(
                b"wrong-recovery-secret",
                DeviceRecord {
                    device_id: "rejected-device".to_string(),
                    public_key: vec![2; 32],
                    authorized: true,
                },
            )
            .unwrap_err(),
        _ => panic!("unknown journey {journey}"),
    }
}

fn break_evidence(journey: &str, evidence: &mut AcceptanceEvidence) {
    match journey {
        "authenticate_restored_identity" => evidence.authenticated_account_id = None,
        "inspect_exact_state" => evidence.restored_snapshot = None,
        "open_every_message" => {
            let key = evidence.opened_messages.keys().next().unwrap().clone();
            evidence.opened_messages.remove(&key);
        }
        "open_every_attachment" => {
            let key = evidence.opened_attachments.keys().next().unwrap().clone();
            evidence.opened_attachments.remove(&key);
        }
        "send_receive_new_encrypted_message" => evidence.new_encrypted_message_received = false,
        "exercise_membership_authority" => evidence.membership_authority_exercised = false,
        "complete_recovery_device_authority" => {
            evidence.recovery_device_authority_completed = false
        }
        _ => panic!("unknown journey {journey}"),
    }
}

fn required_attacks() -> BTreeSet<String> {
    let mut attacks = BTreeSet::new();
    for class in REQUIRED_CLASSES {
        attacks.insert(format!("omit_{class}"));
        attacks.insert(format!("corrupt_{class}"));
        attacks.insert(format!("valid_manifest_broken_{class}"));
    }
    for journey in REQUIRED_JOURNEYS {
        attacks.insert(format!("break_{journey}"));
    }
    attacks.extend([
        "depends_on_original_profile_secret".to_string(),
        "hide_payment_exclusion".to_string(),
        "hide_transient_runtime_exclusion".to_string(),
        "unknown_production_class".to_string(),
    ]);
    attacks
}

fn require_campaign_complete(
    classes: impl IntoIterator<Item = String>,
    journeys: impl IntoIterator<Item = String>,
    attacks: impl IntoIterator<Item = String>,
) -> Result<(), String> {
    let classes = classes.into_iter().collect::<BTreeSet<_>>();
    let journeys = journeys.into_iter().collect::<BTreeSet<_>>();
    let attacks = attacks.into_iter().collect::<BTreeSet<_>>();
    for class in REQUIRED_CLASSES {
        if !classes.contains(class) {
            return Err(format!("absent attack class: {class}"));
        }
    }
    for journey in REQUIRED_JOURNEYS {
        if !journeys.contains(journey) {
            return Err(format!("absent attack journey: {journey}"));
        }
    }
    for attack in required_attacks() {
        if !attacks.contains(&attack) {
            return Err(format!("absent attack mutant: {attack}"));
        }
    }
    Ok(())
}

#[test]
fn unmutated_packaged_archive_restores_every_fixed_function_without_original_state() {
    let source = fixture();
    let oracle = FixedOracle::before_export(&source, Vec::<String>::new()).expect("fixed oracle");
    let temp = tempfile::tempdir().expect("positive campaign root");
    let root = temp.path().to_path_buf();
    let (archive, disclosure, source_root) = export_fresh(&root, &source);
    assert_eq!(
        disclosure.named_exclusions,
        DELIBERATE_EXCLUSIONS.map(str::to_string)
    );
    assert!(disclosure.notice.contains(PAYMENT_EXCLUSION_NOTICE));
    std::fs::remove_dir_all(&source_root).expect("discard original profile before import");
    assert!(!source_root.exists());
    let fresh = FreshInstall::new(root.join("clean-packaged-install")).expect("fresh install");
    let receipt = fresh
        .import_archive(&archive, PASSPHRASE)
        .expect("actual packaged import");
    let evidence = complete_evidence(&fresh, &source, disclosure);
    verify_packaged_round_trip(&oracle, &evidence).expect("every fixed post-import function");
    println!(
        "TASK6573_CONTROL result=PASS account={} classes=7 journeys=7 original_state_available=0 payment_exclusion=\"{}\" other_exclusion=\"{}\"",
        receipt.account_id, PAYMENT_EXCLUSION_NOTICE, TRANSIENT_RUNTIME_EXCLUSION_CLASS
    );
    drop(evidence);
    drop(fresh);
    temp.close().expect("discard positive campaign");
    assert!(!root.exists());
    println!("TASK6573_CONTROL_DISCARD installs=2 archives=1 keys=1 accounts=1 all_absent=1");
}

#[test]
fn every_incomplete_broken_authority_journey_exclusion_and_unknown_mutant_exits_one() {
    let source = fixture();
    let oracle = FixedOracle::before_export(&source, Vec::<String>::new()).expect("oracle");
    let mut observed = BTreeSet::new();
    let mut reader_runs = 0usize;
    let mut discarded_campaigns = 0usize;

    for (label, mutation) in [
        ("omit", ClassMutation::Omit),
        ("corrupt", ClassMutation::Corrupt),
        ("valid_manifest_broken", ClassMutation::ValidButIncomplete),
    ] {
        for class in REQUIRED_CLASSES {
            let mutant = format!("{label}_{class}");
            let temp = tempfile::tempdir().expect("class campaign");
            let root = temp.path().to_path_buf();
            let (archive, _, source_root) = export_fresh(&root, &source);
            let (first_byte, changed, failed_function) =
                mutate_class(&archive, class, mutation, &source);
            let reader = reader_report(&archive);
            reader_runs += 1;
            match mutation {
                ClassMutation::Omit => {
                    assert_eq!(
                        (reader.recovered_classes, reader.unreadable_classes),
                        (6, 0)
                    );
                }
                ClassMutation::Corrupt => {
                    assert_eq!(
                        (reader.recovered_classes, reader.unreadable_classes),
                        (6, 1)
                    );
                }
                ClassMutation::ValidButIncomplete => {
                    assert_eq!(
                        (reader.recovered_classes, reader.unreadable_classes),
                        (7, 0)
                    );
                }
            }
            assert!(reader.recovered_bytes > 0);
            std::fs::remove_dir_all(&source_root).expect("discard source profile");
            let fresh =
                FreshInstall::new(root.join("clean-packaged-install")).expect("fresh install");
            let result = fresh.import_archive(&archive, PASSPHRASE);
            match mutation {
                ClassMutation::Omit => {
                    let error = result.expect_err("omitted class must reject import");
                    exit_one(
                        &mutant,
                        Err(format!(
                            "class={class} first_missing_byte={first_byte} remaining_bytes={} import_error={error}",
                            reader.recovered_bytes
                        )),
                        class,
                    );
                }
                ClassMutation::Corrupt => {
                    let error = result.expect_err("corrupt class must reject import");
                    exit_one(
                        &mutant,
                        Err(format!(
                            "class={class} first_corrupt_byte={first_byte} remaining_bytes={} import_error={error}",
                            reader.recovered_bytes
                        )),
                        class,
                    );
                }
                ClassMutation::ValidButIncomplete => {
                    result.expect("authenticated incomplete account imports");
                    let changed = changed.as_ref().unwrap();
                    let function = failed_function.as_deref().unwrap();
                    prove_valid_but_incomplete_failure(class, &fresh, &source, changed, function);
                    exit_one(
                        &mutant,
                        Err(format!(
                            "valid-manifest-but-broken-account class={class} first_missing_byte={first_byte} failed_function={function} remaining_bytes={}",
                            reader.recovered_bytes
                        )),
                        class,
                    );
                }
            }
            observed.insert(mutant);
            drop(fresh);
            temp.close().expect("discard class campaign");
            assert!(!root.exists());
            discarded_campaigns += 1;
        }
    }

    {
        let mutant = "depends_on_original_profile_secret";
        let temp = tempfile::tempdir().expect("stale dependency campaign");
        let root = temp.path().to_path_buf();
        let (archive, _, source_root) = export_fresh(&root, &source);
        mutate_metadata(&archive, |metadata| {
            metadata
                .source_dependencies
                .push(ORIGINAL_AUTHORITY.to_string());
        });
        let reader = reader_report(&archive);
        reader_runs += 1;
        assert_eq!(
            (reader.recovered_classes, reader.unreadable_classes),
            (7, 0)
        );
        std::fs::remove_dir_all(&source_root).expect("remove unavailable authority");
        let fresh = FreshInstall::new(root.join("clean-packaged-install")).unwrap();
        let error = fresh.import_archive(&archive, PASSPHRASE).unwrap_err();
        exit_one(
            mutant,
            Err(format!(
                "unavailable_authority={ORIGINAL_AUTHORITY} import_error={error}"
            )),
            ORIGINAL_AUTHORITY,
        );
        observed.insert(mutant.to_string());
        drop(fresh);
        temp.close().expect("discard stale campaign");
        assert!(!root.exists());
        discarded_campaigns += 1;
    }

    for journey in REQUIRED_JOURNEYS {
        let mutant = format!("break_{journey}");
        let temp = tempfile::tempdir().expect("journey campaign");
        let root = temp.path().to_path_buf();
        let (archive, disclosure, source_root) = export_fresh(&root, &source);
        std::fs::remove_dir_all(&source_root).expect("discard original before journey");
        let fresh = FreshInstall::new(root.join("clean-packaged-install")).unwrap();
        fresh
            .import_archive(&archive, PASSPHRASE)
            .expect("journey import");
        let actual_error = actual_failed_journey(journey, &fresh, &source);
        assert!(actual_error.contains(journey), "{journey}: {actual_error}");
        let mut evidence = complete_evidence(&fresh, &source, disclosure);
        break_evidence(journey, &mut evidence);
        let verification = verify_packaged_round_trip(&oracle, &evidence).unwrap_err();
        exit_one(
            &mutant,
            Err(format!(
                "failed_function={journey} product_error={actual_error} proof_error={verification}"
            )),
            journey,
        );
        observed.insert(mutant);
        drop(fresh);
        temp.close().expect("discard journey campaign");
        assert!(!root.exists());
        discarded_campaigns += 1;
    }

    for (mutant, hidden) in [
        ("hide_payment_exclusion", PAYMENT_EXCLUSION_CLASS),
        (
            "hide_transient_runtime_exclusion",
            TRANSIENT_RUNTIME_EXCLUSION_CLASS,
        ),
    ] {
        let temp = tempfile::tempdir().expect("hidden exclusion campaign");
        let root = temp.path().to_path_buf();
        let (archive, _, source_root) = export_fresh(&root, &source);
        mutate_metadata(&archive, |metadata| {
            metadata
                .deliberate_exclusions
                .retain(|class| class != hidden);
        });
        let reader = reader_report(&archive);
        reader_runs += 1;
        assert_eq!(
            (reader.recovered_classes, reader.unreadable_classes),
            (7, 0)
        );
        std::fs::remove_dir_all(&source_root).expect("discard original");
        let fresh = FreshInstall::new(root.join("clean-packaged-install")).unwrap();
        let error = fresh.import_archive(&archive, PASSPHRASE).unwrap_err();
        exit_one(
            mutant,
            Err(format!("hidden_exclusion={hidden} import_error={error}")),
            hidden,
        );
        observed.insert(mutant.to_string());
        drop(fresh);
        temp.close().expect("discard hidden campaign");
        assert!(!root.exists());
        discarded_campaigns += 1;
    }

    {
        let mutant = "unknown_production_class";
        let temp = tempfile::tempdir().expect("unknown production campaign");
        let root = temp.path().to_path_buf();
        std::fs::create_dir(root.join("source-packaged-install"))
            .expect("unknown-class source install");
        let error = require_export_time_dispositions(
            REQUIRED_CLASSES
                .into_iter()
                .chain(DELIBERATE_EXCLUSIONS)
                .chain([UNKNOWN_PRODUCTION_CLASS]),
            REQUIRED_CLASSES,
            DELIBERATE_EXCLUSIONS,
        )
        .unwrap_err();
        exit_one(mutant, Err(error), "absent export-time words");
        assert!(!root.join("account.osl-account").exists());
        observed.insert(mutant.to_string());
        temp.close().expect("discard unknown production campaign");
        assert!(!root.exists());
        discarded_campaigns += 1;
    }

    require_campaign_complete(
        REQUIRED_CLASSES.into_iter().map(str::to_string),
        REQUIRED_JOURNEYS.into_iter().map(str::to_string),
        observed.clone(),
    )
    .expect("all attacks executed");
    assert_eq!(observed, required_attacks());
    println!(
        "TASK6573_CAMPAIGN attacks={} class_omissions=7 class_corruptions=7 valid_manifest_broken_accounts=7 stale_authorities=1 failed_journeys=7 hidden_exclusions=2 unknown_classes=1 reader_runs={reader_runs}",
        observed.len()
    );
    println!(
        "TASK6573_DISCARD campaigns={discarded_campaigns} installs=63 archives=31 keys=31 restored_accounts=14 all_absent=1"
    );
}

#[test]
fn starving_any_class_journey_or_mutant_exits_one_naming_the_absent_attack() {
    let classes = REQUIRED_CLASSES.map(str::to_string).to_vec();
    let journeys = REQUIRED_JOURNEYS.map(str::to_string).to_vec();
    let attacks = required_attacks().into_iter().collect::<Vec<_>>();
    for missing in &classes {
        exit_one(
            &format!("starve_class_{missing}"),
            require_campaign_complete(
                classes.iter().filter(|item| *item != missing).cloned(),
                journeys.clone(),
                attacks.clone(),
            ),
            missing,
        );
    }
    for missing in &journeys {
        exit_one(
            &format!("starve_journey_{missing}"),
            require_campaign_complete(
                classes.clone(),
                journeys.iter().filter(|item| *item != missing).cloned(),
                attacks.clone(),
            ),
            missing,
        );
    }
    for missing in &attacks {
        exit_one(
            &format!("starve_mutant_{missing}"),
            require_campaign_complete(
                classes.clone(),
                journeys.clone(),
                attacks.iter().filter(|item| *item != missing).cloned(),
            ),
            missing,
        );
    }
    println!(
        "TASK6573_STARVATION classes={} journeys={} mutants={} total={} all_exit=1",
        classes.len(),
        journeys.len(),
        attacks.len(),
        classes.len() + journeys.len() + attacks.len()
    );
}
