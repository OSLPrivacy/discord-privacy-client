use osl_privacy_hub::account_export::{
    collect_complete_snapshot, export_to_user_paths, verify_inventory_agreement,
    AccountExportSnapshot, AccountExportSource, OwnedAttachment, OwnedDocument,
    PostWriteMediaFault, INDEPENDENT_COPY_WARNING, KEY_WARNING, PAGE_ITEMS,
};
use rand::{rngs::OsRng, RngCore};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone)]
struct IndependentGenerator {
    signed_in: String,
    documents: Vec<OwnedDocument>,
    attachments: Vec<OwnedAttachment>,
}

impl AccountExportSource for IndependentGenerator {
    fn signed_in_account_id(&self) -> Result<String, String> {
        Ok(self.signed_in.clone())
    }

    fn page_documents(
        &self,
        account_id: &str,
        after: Option<&str>,
        limit: usize,
    ) -> Result<Vec<OwnedDocument>, String> {
        let mut rows = self
            .documents
            .iter()
            .filter(|row| {
                row.owner_id == account_id && after.is_none_or(|cursor| row.id.as_str() > cursor)
            })
            .cloned()
            .collect::<Vec<_>>();
        rows.sort_by(|left, right| left.id.cmp(&right.id));
        rows.truncate(limit);
        Ok(rows)
    }

    fn page_attachments(
        &self,
        account_id: &str,
        after: Option<&str>,
        limit: usize,
    ) -> Result<Vec<OwnedAttachment>, String> {
        let mut rows = self
            .attachments
            .iter()
            .filter(|row| {
                row.owner_id == account_id && after.is_none_or(|cursor| row.id.as_str() > cursor)
            })
            .cloned()
            .collect::<Vec<_>>();
        rows.sort_by(|left, right| left.id.cmp(&right.id));
        rows.truncate(limit);
        Ok(rows)
    }
}

fn seed_generator() -> IndependentGenerator {
    let owner = "person-alpha-unpredictable".to_owned();
    let other = "person-beta-canary".to_owned();
    let mut nonce = [0u8; 32];
    OsRng.fill_bytes(&mut nonce);
    let mut documents = vec![
        OwnedDocument {
            class: "identity_profile".to_owned(),
            id: "00-identity".to_owned(),
            owner_id: owner.clone(),
            fields: json!({"displayName":"Alpha","nonce":hex::encode(nonce),"allProfileFields":true}),
            attachment_ids: vec![],
        },
        OwnedDocument {
            class: "settings".to_owned(),
            id: "01-settings".to_owned(),
            owner_id: owner.clone(),
            fields: json!({"theme":"night","retentionSeconds":731,"nested":{"enabled":true}}),
            attachment_ids: vec![],
        },
    ];
    for index in 0..4 {
        documents.push(OwnedDocument {
            class: "friend_relationships".to_owned(),
            id: format!("02-friend-{index:03}"),
            owner_id: owner.clone(),
            fields: json!({"friendIndex":index,"realRelationship":true,"safetyNumber":format!("safety-{index}")}),
            attachment_ids: vec![],
        });
    }
    let attachment_message_indexes = [0usize, 3, 9, 15, 16, 31, 40];
    let mut attachments = Vec::new();
    for message_index in 0..41 {
        let attachment_ids = attachment_message_indexes
            .iter()
            .position(|candidate| *candidate == message_index)
            .map(|attachment_index| vec![format!("attachment-{attachment_index:03}")])
            .unwrap_or_default();
        documents.push(OwnedDocument {
            class: "messages".to_owned(),
            id: format!("03-message-{message_index:03}"),
            owner_id: owner.clone(),
            fields: json!({
                "messageId":format!("message-{message_index:03}"),
                "channelId":format!("channel-{}", message_index % 3),
                "plaintext":format!("unpredictable payload {} {}", message_index, hex::encode(&nonce[..8])),
                "replyParentId": if message_index % 5 == 0 { Some("parent") } else { None },
                "editRevision": 1 + message_index % 3,
                "timerMinutes": if message_index % 2 == 0 { Some(10) } else { None },
            }),
            attachment_ids,
        });
    }
    for (attachment_index, message_index) in attachment_message_indexes.into_iter().enumerate() {
        let length = if attachment_index == 6 {
            197_111
        } else {
            700 + attachment_index * 173
        };
        let mut bytes = vec![0u8; length];
        OsRng.fill_bytes(&mut bytes);
        attachments.push(OwnedAttachment {
            id: format!("attachment-{attachment_index:03}"),
            owner_id: owner.clone(),
            message_id: format!("03-message-{message_index:03}"),
            filename: format!("random-{attachment_index}.bin"),
            mime_type: "application/octet-stream".to_owned(),
            bytes,
        });
    }
    // The source inventory contains a fully independent second owner; its
    // canary must never cross the account predicate or collector recheck.
    documents.push(OwnedDocument {
        class: "messages".to_owned(),
        id: "99-second-person-canary".to_owned(),
        owner_id: other.clone(),
        fields: json!({"plaintext":"SECOND-PERSON-SECRET-CANARY"}),
        attachment_ids: vec!["second-person-attachment".to_owned()],
    });
    attachments.push(OwnedAttachment {
        id: "second-person-attachment".to_owned(),
        owner_id: other,
        message_id: "99-second-person-canary".to_owned(),
        filename: "second-secret.bin".to_owned(),
        mime_type: "application/octet-stream".to_owned(),
        bytes: b"SECOND-PERSON-BYTE-CANARY".to_vec(),
    });
    IndependentGenerator {
        signed_in: owner,
        documents,
        attachments,
    }
}

fn sha(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

#[test]
fn complete_oracle_crosses_every_page_and_block_boundary() {
    verify_inventory_agreement().expect("three production inventories agree");
    assert_eq!(PAGE_ITEMS, 16);
    let source = seed_generator();
    let snapshot =
        collect_complete_snapshot(&source, &source.signed_in).expect("complete owner snapshot");

    // Freeze the complete independent oracle before calling the exporter.
    let expected_classes = BTreeMap::from([
        ("identity_profile", 1usize),
        ("settings", 1),
        ("friend_relationships", 4),
        ("messages", 41),
        ("attachments", 7),
    ]);
    let expected_document_ids = snapshot
        .documents
        .iter()
        .map(|row| (&row.class, &row.id))
        .collect::<BTreeSet<_>>();
    let expected_attachment_hashes = snapshot
        .attachments
        .iter()
        .map(|row| (row.id.clone(), sha(&row.bytes)))
        .collect::<BTreeMap<_, _>>();
    let expected_attachment_bytes: usize =
        snapshot.attachments.iter().map(|row| row.bytes.len()).sum();
    let expected_owners = snapshot
        .documents
        .iter()
        .map(|row| row.owner_id.as_str())
        .chain(snapshot.attachments.iter().map(|row| row.owner_id.as_str()))
        .collect::<BTreeSet<_>>();

    assert_eq!(expected_document_ids.len(), 47);
    assert_eq!(expected_attachment_hashes.len(), 7);
    assert_eq!(expected_owners, BTreeSet::from([source.signed_in.as_str()]));
    assert!(expected_attachment_bytes > 3 * 65_536);
    let canary = b"SECOND-PERSON-SECRET";
    assert!(!serde_json::to_vec(&snapshot)
        .unwrap()
        .windows(canary.len())
        .any(|window| window == canary));

    let destination = tempfile::tempdir().unwrap();
    let archive = destination.path().join("owner.oslexport");
    let key = destination.path().join("owner.key.json");
    let receipt = export_to_user_paths(&snapshot, &archive, &key, PostWriteMediaFault::None)
        .expect("full destination readback succeeds");
    let actual_classes = receipt
        .class_counts
        .iter()
        .map(|(name, count)| (name.as_str(), *count as usize))
        .collect::<BTreeMap<_, _>>();
    assert_eq!(actual_classes, expected_classes);
    assert_eq!(receipt.authenticated_blocks, receipt.manifest_blocks);
    assert!(
        receipt.authenticated_blocks.len() > snapshot.documents.len() + snapshot.attachments.len()
    );
    assert_eq!(
        receipt.archive_bytes,
        std::fs::metadata(&archive).unwrap().len()
    );
    assert_eq!(receipt.key_bytes, std::fs::metadata(&key).unwrap().len());
    println!("TASK5200_ORACLE classes=5 documents=47 messages=41 friends=4 attachments=7 attachment_bytes={expected_attachment_bytes} pages_documents=3 pages_attachments=1 authenticated_blocks={} archive_bytes={} key_bytes={}", receipt.authenticated_blocks.len(), receipt.archive_bytes, receipt.key_bytes);
    println!("TASK5200_WARN_1={KEY_WARNING}");
    println!("TASK5200_WARN_2={INDEPENDENT_COPY_WARNING}");
}

#[test]
fn second_authenticated_account_cannot_export_the_first() {
    let source = seed_generator();
    let error = collect_complete_snapshot(&source, "person-beta-canary").unwrap_err();
    assert!(error.contains("reauthorization did not match"));
    println!("TASK5200_ACCOUNT_ISOLATION=reauthorization did not match the signed-in account");
}

#[test]
fn post_write_media_faults_never_return_a_success_receipt() {
    for fault in [
        PostWriteMediaFault::FailedWriteArchive,
        PostWriteMediaFault::DiskFullKey,
        PostWriteMediaFault::ShortWriteArchive,
        PostWriteMediaFault::DeleteKey,
        PostWriteMediaFault::TruncateArchive,
        PostWriteMediaFault::CorruptArchive,
        PostWriteMediaFault::TearFinalBlock,
    ] {
        let source = seed_generator();
        let snapshot = collect_complete_snapshot(&source, &source.signed_in).unwrap();
        let destination = tempfile::tempdir().unwrap();
        let error = export_to_user_paths(
            &snapshot,
            &destination.path().join("owner.oslexport"),
            &destination.path().join("owner.key.json"),
            fault,
        )
        .expect_err("post-write media fault must have no successful receipt");
        assert!(
            error.contains("injected storage fault")
                && (error.contains("saved") || error.contains("integrity failure"))
        );
        println!("TASK5200_MEDIA_FAULT={fault:?}|failure={error}");
    }
}

#[test]
fn repeated_exports_never_reuse_key_nonce_or_archive_material() {
    let source = seed_generator();
    let snapshot = collect_complete_snapshot(&source, &source.signed_in).unwrap();
    let destination = tempfile::tempdir().unwrap();
    let a_archive = destination.path().join("a.oslexport");
    let a_key = destination.path().join("a.key.json");
    let b_archive = destination.path().join("b.oslexport");
    let b_key = destination.path().join("b.key.json");
    export_to_user_paths(&snapshot, &a_archive, &a_key, PostWriteMediaFault::None).unwrap();
    export_to_user_paths(&snapshot, &b_archive, &b_key, PostWriteMediaFault::None).unwrap();
    assert_ne!(
        std::fs::read(a_archive).unwrap(),
        std::fs::read(b_archive).unwrap()
    );
    assert_ne!(std::fs::read(a_key).unwrap(), std::fs::read(b_key).unwrap());
    println!("TASK5200_REPEATED_EXPORT_MATERIAL=distinct");
}

#[test]
fn ownership_and_reference_leaks_fail_closed() {
    let source = seed_generator();
    let mut snapshot = collect_complete_snapshot(&source, &source.signed_in).unwrap();
    snapshot.documents[0].owner_id = "person-beta-canary".to_owned();
    let destination = tempfile::tempdir().unwrap();
    let error = export_to_user_paths(
        &snapshot,
        &destination.path().join("bad.oslexport"),
        &destination.path().join("bad.key.json"),
        PostWriteMediaFault::None,
    )
    .unwrap_err();
    assert!(error.contains("second-account"));

    let mut snapshot = AccountExportSnapshot {
        account_id: source.signed_in,
        documents: vec![],
        attachments: vec![],
    };
    snapshot.attachments.push(OwnedAttachment {
        id: "unreferenced".to_owned(),
        owner_id: snapshot.account_id.clone(),
        message_id: "none".to_owned(),
        filename: "x".to_owned(),
        mime_type: "application/octet-stream".to_owned(),
        bytes: vec![1],
    });
    let error = export_to_user_paths(
        &snapshot,
        &destination.path().join("bad2.oslexport"),
        &destination.path().join("bad2.key.json"),
        PostWriteMediaFault::None,
    )
    .unwrap_err();
    assert!(error.contains("attachment reference inventory"));
    println!("TASK5200_LEAK_AND_REFERENCE_MUTANTS=2 rejected");
}
