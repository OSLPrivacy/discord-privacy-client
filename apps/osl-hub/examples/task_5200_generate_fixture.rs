use osl_privacy_hub::account_export::{
    export_to_user_paths, AccountExportSnapshot, OwnedAttachment, OwnedDocument,
    PostWriteMediaFault,
};
use rand::{rngs::OsRng, RngCore};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, path::PathBuf};

fn main() {
    let args = std::env::args_os()
        .skip(1)
        .map(PathBuf::from)
        .collect::<Vec<_>>();
    if args.len() != 3 {
        eprintln!("usage: task_5200_generate_fixture ARCHIVE KEY ORACLE");
        std::process::exit(2);
    }
    let owner = "clean-profile-owner".to_owned();
    let attachment_indexes = [0usize, 3, 9, 15, 16, 31, 40];
    let mut documents = vec![
        OwnedDocument {
            class: "identity_profile".into(),
            id: "identity".into(),
            owner_id: owner.clone(),
            fields: json!({"name":"Clean profile owner"}),
            attachment_ids: vec![],
        },
        OwnedDocument {
            class: "settings".into(),
            id: "settings".into(),
            owner_id: owner.clone(),
            fields: json!({"setting":true}),
            attachment_ids: vec![],
        },
    ];
    for index in 0..4 {
        documents.push(OwnedDocument {
            class: "friend_relationships".into(),
            id: format!("friend-{index:02}"),
            owner_id: owner.clone(),
            fields: json!({"friend":index}),
            attachment_ids: vec![],
        });
    }
    let mut attachments = Vec::new();
    for index in 0..41 {
        let ids = attachment_indexes
            .iter()
            .position(|value| *value == index)
            .map(|a| vec![format!("attachment-{a:02}")])
            .unwrap_or_default();
        documents.push(OwnedDocument {
            class: "messages".into(),
            id: format!("message-{index:03}"),
            owner_id: owner.clone(),
            fields: json!({"body":format!("message body {index}"),"allFields":true}),
            attachment_ids: ids,
        });
    }
    for (index, message) in attachment_indexes.into_iter().enumerate() {
        let mut bytes = vec![
            0;
            if index == 6 {
                197_003
            } else {
                991 + index * 137
            }
        ];
        OsRng.fill_bytes(&mut bytes);
        attachments.push(OwnedAttachment {
            id: format!("attachment-{index:02}"),
            owner_id: owner.clone(),
            message_id: format!("message-{message:03}"),
            filename: format!("file-{index}.bin"),
            mime_type: "application/octet-stream".into(),
            bytes,
        });
    }
    let snapshot = AccountExportSnapshot {
        account_id: owner,
        documents,
        attachments,
    };
    // Freeze the oracle from source objects before the exporter sees them.  It
    // intentionally uses the published object bytes, including random media.
    let mut objects = BTreeMap::new();
    for document in &snapshot.documents {
        let bytes = serde_json::to_vec(document).expect("source document JSON");
        objects.insert(
            format!("{}:{}", document.class, document.id),
            json!({"byteCount":bytes.len(),"sha256":hex_sha256(&bytes)}),
        );
    }
    for attachment in &snapshot.attachments {
        objects.insert(
            format!("attachments:{}", attachment.id),
            json!({"byteCount":attachment.bytes.len(),"sha256":hex_sha256(&attachment.bytes)}),
        );
    }
    if objects.len() != 54 {
        panic!("fixture oracle inventory");
    }
    let oracle = json!({"accountId":snapshot.account_id,"objects":objects});
    std::fs::write(
        &args[2],
        serde_json::to_vec_pretty(&oracle).expect("oracle JSON"),
    )
    .unwrap_or_else(|_| {
        eprintln!("fixture oracle write failed");
        std::process::exit(1)
    });
    match export_to_user_paths(
        &snapshot,
        &args[0],
        &args[1],
        PostWriteMediaFault::None,
    ) {
        Ok(receipt) => println!(
            "TASK5200_FIXTURE archive_bytes={} key_bytes={} authenticated_blocks={} objects=54 messages=41 attachments=7 document_pages=3 final_message=message-040 final_attachment=attachment-06",
            receipt.archive_bytes,
            receipt.key_bytes,
            receipt.authenticated_blocks.len()
        ),
        Err(error) => {
            eprintln!("fixture generation failed: {error}");
            std::process::exit(1);
        }
    }
}

fn hex_sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
