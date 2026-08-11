use osl_privacy_hub::personal_archive::{
    verify_packaged_round_trip, AcceptanceEvidence, AcceptanceMode, AccountSnapshot,
    AttachmentRecord, ConversationRecord, DeviceRecord, DeviceRecoveryState, FixedOracle,
    FreshInstall, FriendRecord, IdentityAndKeys, MembershipRecord, MessageRecord,
    SettingsExportSession, DELIBERATE_EXCLUSIONS, PAYMENT_EXCLUSION_NOTICE,
};
use rand::{rngs::OsRng, RngCore};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

fn random_bytes(len: usize) -> Vec<u8> {
    let mut bytes = vec![0u8; len];
    OsRng.fill_bytes(&mut bytes);
    bytes
}

fn token(label: &str) -> String {
    let bytes = random_bytes(12);
    let suffix: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
    format!("{label}-{suffix}")
}

fn varied_count(minimum: usize, spread: u32) -> usize {
    minimum + (OsRng.next_u32() % spread) as usize
}

fn boundary_bytes(boundaries: &[usize]) -> Vec<u8> {
    let index = (OsRng.next_u32() as usize) % boundaries.len();
    let jitter = (OsRng.next_u32() % 9) as usize;
    random_bytes(boundaries[index] + jitter)
}

struct GeneratedCase {
    snapshot: AccountSnapshot,
    authentication_secret: Vec<u8>,
    recovery_secret: Vec<u8>,
    foreign_canaries: BTreeSet<String>,
    message_bytes: usize,
    attachment_bytes: usize,
}

/// This generator is intentionally outside the product module. It chooses
/// counts and opaque bytes from the OS RNG and inserts foreign-owner canaries
/// before the product sees the account.
fn independent_case() -> GeneratedCase {
    let owner = token("owner");
    let foreign_owner = token("foreign-owner");
    let authentication_secret = random_bytes(33);
    let recovery_secret = random_bytes(47);
    let friend_count = varied_count(2, 5);
    let conversation_count = varied_count(2, 4);
    let membership_count = varied_count(2, 5);
    let device_count = varied_count(1, 4);
    let setting_count = varied_count(3, 6);

    let mut friends = Vec::new();
    for _ in 0..friend_count {
        friends.push(FriendRecord {
            friend_id: token("friend"),
            owner_account_id: owner.clone(),
            display_name: token("display"),
            public_key: random_bytes(32),
        });
    }

    let mut conversations = Vec::new();
    let mut message_ids = Vec::new();
    let mut message_bytes = 0usize;
    for _ in 0..conversation_count {
        let mut messages = Vec::new();
        for ordinal in 0..varied_count(2, 6) {
            let body = boundary_bytes(&[1, 31, 32, 255, 256, 4_095, 4_096]);
            message_bytes += body.len();
            let message_id = token("message");
            message_ids.push(message_id.clone());
            messages.push(MessageRecord {
                message_id,
                owner_account_id: owner.clone(),
                author_id: if ordinal % 2 == 0 {
                    owner.clone()
                } else {
                    friends[ordinal % friends.len()].friend_id.clone()
                },
                body,
                sent_at_ms: OsRng.next_u64(),
            });
        }
        conversations.push(ConversationRecord {
            conversation_id: token("conversation"),
            owner_account_id: owner.clone(),
            member_ids: friends
                .iter()
                .take(varied_count(1, friend_count as u32).min(friend_count))
                .map(|friend| friend.friend_id.clone())
                .collect(),
            messages,
        });
    }

    let attachment_count =
        varied_count(2, (message_ids.len().max(3) - 1) as u32).min(message_ids.len());
    let mut attachments = Vec::new();
    let mut attachment_bytes = 0usize;
    for ordinal in 0..attachment_count {
        let bytes = boundary_bytes(&[1, 255, 256, 4_095, 4_096, 8_193]);
        attachment_bytes += bytes.len();
        attachments.push(AttachmentRecord {
            attachment_id: token("attachment"),
            owner_account_id: owner.clone(),
            message_id: message_ids[ordinal].clone(),
            filename: format!("{}.bin", token("file")),
            media_type: "application/octet-stream".to_string(),
            bytes,
        });
    }

    let settings = (0..setting_count)
        .map(|_| (token("setting"), boundary_bytes(&[1, 31, 32, 255, 256])))
        .collect();
    let memberships = (0..membership_count)
        .map(|ordinal| MembershipRecord {
            membership_id: token("membership"),
            owner_account_id: owner.clone(),
            role: if ordinal == 0 { "owner" } else { "member" }.to_string(),
            authority_key: random_bytes(32),
        })
        .collect();
    let devices = (0..device_count)
        .map(|_| DeviceRecord {
            device_id: token("device"),
            public_key: random_bytes(32),
            authorized: true,
        })
        .collect();

    let mut foreign_canaries = BTreeSet::new();
    let foreign_friend = token("foreign-friend-canary");
    let foreign_message = token("foreign-message-canary");
    let foreign_conversation = token("foreign-conversation-canary");
    let foreign_attachment = token("foreign-attachment-canary");
    let foreign_membership = token("foreign-membership-canary");
    foreign_canaries.extend([
        foreign_friend.clone(),
        foreign_message.clone(),
        foreign_conversation.clone(),
        foreign_attachment.clone(),
        foreign_membership.clone(),
    ]);
    friends.push(FriendRecord {
        friend_id: foreign_friend,
        owner_account_id: foreign_owner.clone(),
        display_name: token("foreign-display"),
        public_key: random_bytes(32),
    });
    conversations.push(ConversationRecord {
        conversation_id: foreign_conversation,
        owner_account_id: foreign_owner.clone(),
        member_ids: Vec::new(),
        messages: vec![MessageRecord {
            message_id: foreign_message.clone(),
            owner_account_id: foreign_owner.clone(),
            author_id: foreign_owner.clone(),
            body: random_bytes(67),
            sent_at_ms: OsRng.next_u64(),
        }],
    });
    attachments.push(AttachmentRecord {
        attachment_id: foreign_attachment,
        owner_account_id: foreign_owner.clone(),
        message_id: foreign_message,
        filename: "foreign-canary.bin".to_string(),
        media_type: "application/octet-stream".to_string(),
        bytes: random_bytes(71),
    });
    let mut memberships: Vec<MembershipRecord> = memberships;
    memberships.push(MembershipRecord {
        membership_id: foreign_membership,
        owner_account_id: foreign_owner,
        role: "foreign-owner".to_string(),
        authority_key: random_bytes(32),
    });

    GeneratedCase {
        snapshot: AccountSnapshot {
            identity_and_keys: IdentityAndKeys {
                account_id: owner.clone(),
                display_name: token("account-display"),
                authentication_secret: authentication_secret.clone(),
                identity_private_key: random_bytes(64),
                identity_public_key: random_bytes(32),
                message_key: random_bytes(32),
            },
            friends,
            conversations,
            attachments,
            settings,
            memberships,
            device_recovery: DeviceRecoveryState {
                owner_account_id: owner,
                recovery_secret: recovery_secret.clone(),
                devices,
            },
        },
        authentication_secret,
        recovery_secret,
        foreign_canaries,
        message_bytes,
        attachment_bytes,
    }
}

#[test]
fn fresh_packaged_install_is_the_personal_export_oracle() {
    let generated = independent_case();
    let expected_snapshot = generated.snapshot.owned_only();
    let source_profile = tempfile::tempdir().expect("source profile");
    let oracle_dir = tempfile::tempdir().expect("oracle directory");
    let transfer_dir = tempfile::tempdir().expect("transfer directory");
    let fresh_root = tempfile::tempdir().expect("fresh packaged install root");
    let archive_path = transfer_dir.path().join("account.osl-account");
    let oracle_path = oracle_dir.path().join("fixed-before-export.json");
    let passphrase = token("archive-passphrase");

    // A standalone generator fixes and fsyncs the complete oracle before the
    // Settings disclosure is reached and before export can begin.
    let oracle = FixedOracle::before_export(
        &generated.snapshot,
        generated.foreign_canaries.iter().cloned(),
    )
    .expect("generator fixes complete oracle");
    let oracle_bytes = serde_json::to_vec_pretty(&oracle).expect("serialize fixed oracle");
    std::fs::write(&oracle_path, &oracle_bytes).expect("write oracle before export");
    std::fs::File::open(&oracle_path)
        .expect("open oracle")
        .sync_all()
        .expect("fsync fixed oracle");
    let oracle_sha256 = format!("{:x}", Sha256::digest(&oracle_bytes));
    std::fs::write(
        source_profile.path().join("original-only-canary"),
        random_bytes(53),
    )
    .expect("seed unavailable original profile state");

    let mut settings = SettingsExportSession::new(generated.snapshot.clone());
    let disclosure = settings.disclosure();
    assert_eq!(
        disclosure.named_exclusions,
        DELIBERATE_EXCLUSIONS.map(str::to_string)
    );
    assert!(disclosure.notice.contains(PAYMENT_EXCLUSION_NOTICE));
    println!(
        "TASK6572_DISCLOSURE excluded={:?} notice={}",
        disclosure.named_exclusions, disclosure.notice
    );
    settings
        .record_visible_disclosure(&disclosure.named_exclusions, &disclosure.notice)
        .expect("packaged Settings records every named exclusion");
    let export = settings
        .export_to(&archive_path, &passphrase)
        .expect("packaged Settings exports archive");
    drop(settings);

    let source_path = source_profile.path().to_path_buf();
    source_profile
        .close()
        .expect("discard original profile before import");
    assert!(!source_path.exists(), "original profile is unavailable");

    let fresh_install = FreshInstall::new(fresh_root.path()).expect("install clean packaged OSL");
    assert!(!fresh_install.state_file().exists());
    let import = fresh_install
        .import_archive(&archive_path, &passphrase)
        .expect("import through packaged product backend");
    let mut account = fresh_install
        .authenticate(
            &expected_snapshot.identity_and_keys.account_id,
            &generated.authentication_secret,
            &passphrase,
        )
        .expect("authenticate as restored identity");
    assert_eq!(account.snapshot(), &expected_snapshot);

    let mut opened_messages = BTreeMap::new();
    for message in expected_snapshot
        .conversations
        .iter()
        .flat_map(|conversation| &conversation.messages)
    {
        opened_messages.insert(
            message.message_id.clone(),
            account
                .open_message(&message.message_id)
                .expect("open every restored message"),
        );
    }
    let mut opened_attachments = BTreeMap::new();
    for attachment in &expected_snapshot.attachments {
        opened_attachments.insert(
            attachment.attachment_id.clone(),
            account
                .open_attachment(&attachment.attachment_id)
                .expect("open every restored attachment"),
        );
    }

    let conversation_id = expected_snapshot.conversations[0].conversation_id.clone();
    let new_message_id = token("post-import-message");
    let new_plaintext = boundary_bytes(&[257, 4_097]);
    let wire = account
        .send_encrypted_message(
            &conversation_id,
            &new_message_id,
            &new_plaintext,
            OsRng.next_u64(),
        )
        .expect("send newly encrypted message");
    assert!(!wire
        .ciphertext
        .windows(new_plaintext.len())
        .any(|window| window == new_plaintext));
    let received = account
        .receive_encrypted_message(wire)
        .expect("receive newly encrypted message");
    assert_eq!(received, new_plaintext);

    let membership = &expected_snapshot.memberships[0];
    let authority_proof = account
        .exercise_membership_authority(&membership.membership_id, &random_bytes(39))
        .expect("exercise restored membership authority");
    assert_ne!(authority_proof, [0u8; 32]);
    let restored_devices_before = expected_snapshot.device_recovery.devices.len();
    account
        .authorize_recovery_device(
            &generated.recovery_secret,
            DeviceRecord {
                device_id: token("recovered-device"),
                public_key: random_bytes(32),
                authorized: true,
            },
        )
        .expect("complete recovery/device authority journey");
    assert_eq!(
        account.snapshot().device_recovery.devices.len(),
        restored_devices_before + 1
    );

    let evidence = AcceptanceEvidence {
        mode: AcceptanceMode::PackagedRoundTrip,
        clean_packaged_install: true,
        original_local_state_available: false,
        source_dependencies: Vec::new(),
        disclosure: disclosure.clone(),
        payment_records_present: false,
        authenticated_account_id: Some(expected_snapshot.identity_and_keys.account_id.clone()),
        restored_snapshot: Some(expected_snapshot.clone()),
        opened_messages,
        opened_attachments,
        new_encrypted_message_sent: true,
        new_encrypted_message_received: true,
        membership_authority_exercised: true,
        recovery_device_authority_completed: true,
    };
    verify_packaged_round_trip(&oracle, &evidence)
        .expect("6573 accepts only live post-import packaged behavior");

    println!(
        "TASK6572_ORACLE sha256={} identity={} friends={} conversations={} messages={} message_bytes={} attachments={} attachment_bytes={} settings={} memberships={} devices={} foreign_canaries={}",
        oracle_sha256,
        oracle.class_counts.identity_and_keys,
        oracle.class_counts.friends,
        oracle.class_counts.conversations,
        oracle.class_counts.messages,
        generated.message_bytes,
        oracle.class_counts.attachments,
        generated.attachment_bytes,
        oracle.class_counts.settings,
        oracle.class_counts.memberships,
        oracle.class_counts.devices,
        oracle.foreign_owner_canary_ids.len(),
    );
    println!(
        "TASK6572_ROUND_TRIP archive_bytes={} imported_account={} opened_messages={} opened_attachments={} encrypted_send=1 encrypted_receive=1 membership_authority=1 recovery_device_authority=1 payment_records=0 original_state_available=0 result=PASS",
        export.archive_bytes,
        import.account_id,
        oracle.message_bodies.len(),
        oracle.attachment_bytes.len(),
    );

    drop(account);
    drop(fresh_install);
    drop(evidence);
    drop(expected_snapshot);
    drop(generated);
    let fresh_path = fresh_root.path().to_path_buf();
    let transfer_path = transfer_dir.path().to_path_buf();
    let oracle_root = oracle_dir.path().to_path_buf();
    fresh_root
        .close()
        .expect("discard fresh install/account/key state");
    transfer_dir.close().expect("discard exported archive");
    oracle_dir.close().expect("discard fixed oracle");
    assert!(!fresh_path.exists() && !transfer_path.exists() && !oracle_root.exists());
    println!("TASK6572_DISCARD installs=2 archives=1 keys=1 accounts=1 all_absent=1");
}
