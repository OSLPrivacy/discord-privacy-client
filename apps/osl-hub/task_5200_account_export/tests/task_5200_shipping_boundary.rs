#[path = "../../src/account_export.rs"]
mod shipping_account_export;

use shipping_account_export::{
    write_and_verify, AccountExportNativeState, DestinationKind, FORMAT_BLOCK_BYTES,
};
use std::fs;
use task_5200_clean_reader::read_complete;
use tempfile::TempDir;

#[test]
fn packaged_host_uses_opaque_native_grants_and_reopened_independent_readback() {
    let profile = TempDir::new().unwrap();
    let destination = TempDir::new().unwrap();
    fs::write(
        profile.path().join("identity.json"),
        br#"{"owner":"shipping-owner"}"#,
    )
    .unwrap();
    fs::write(
        profile.path().join("hub-profile.json"),
        br#"{"name":"Owner"}"#,
    )
    .unwrap();
    fs::write(
        profile.path().join("preferences.json"),
        br#"{"theme":"dark"}"#,
    )
    .unwrap();
    fs::write(profile.path().join("hub_people.json"), br#"{"friends":4}"#).unwrap();
    fs::write(
        profile.path().join("service-registry.json"),
        br#"{"accounts":1}"#,
    )
    .unwrap();
    fs::write(profile.path().join("whitelist.json"), br#"{"rules":1}"#).unwrap();
    fs::write(
        profile.path().join("activity-journal.json"),
        br#"{"events":1}"#,
    )
    .unwrap();
    for index in 0..41 {
        fs::write(
            profile.path().join(format!("message-{index:03}.json")),
            format!("shipping-owner-message-{index}"),
        )
        .unwrap();
    }
    for index in 0..7 {
        fs::write(
            profile.path().join(format!("attachment-{index:03}.blob")),
            vec![index as u8; FORMAT_BLOCK_BYTES * 2 + index + 1],
        )
        .unwrap();
    }

    let archive_path = destination.path().join("person.oslax");
    let key_path = destination.path().join("person.oslkey");
    let state = AccountExportNativeState::default();
    state.record_reauthorization("shipping-owner").unwrap();
    let archive = state
        .issue_save_grant(
            "shipping-owner",
            DestinationKind::Archive,
            archive_path.clone(),
        )
        .unwrap();
    let key = state
        .issue_save_grant("shipping-owner", DestinationKind::Key, key_path.clone())
        .unwrap();
    assert!(!archive
        .destination_token
        .contains(destination.path().to_str().unwrap()));
    assert!(!key
        .destination_token
        .contains(destination.path().to_str().unwrap()));

    let response = write_and_verify(
        &state,
        "shipping-owner",
        profile.path(),
        &archive.destination_token,
        &key.destination_token,
    )
    .unwrap();
    let reopened_archive = fs::read(&archive_path).unwrap();
    let reopened_key = fs::read(&key_path).unwrap();
    let independent = read_complete(&reopened_archive, &reopened_key).unwrap();

    assert_eq!(response.manifest.archive_byte_count, reopened_archive.len());
    assert_eq!(response.manifest.key_byte_count, reopened_key.len());
    assert_eq!(response.readback.archive_bytes_read, reopened_archive.len());
    assert_eq!(response.readback.key_bytes_read, reopened_key.len());
    assert_eq!(independent.entries.len(), 55);
    assert_eq!(independent.manifest.inventory_item_counts["messages"], 41);
    assert_eq!(independent.manifest.inventory_item_counts["attachments"], 7);
    assert!(response.readback.archive_closed_and_reopened);
    assert!(response.readback.key_closed_and_reopened);
    assert!(response.readback.header_authenticated);
    assert!(response.readback.manifest_authenticated);
    assert!(response.readback.final_block_authenticated);
    assert_eq!(
        response.readback.authenticated_block_ids,
        response.manifest.authenticated_block_ids
    );
    assert!(state
        .issue_save_grant(
            "shipping-owner",
            DestinationKind::Archive,
            destination.path().join("second.oslax"),
        )
        .unwrap_err()
        .contains("Reauthorize"));

    println!("TASK5200_SHIPPING_PROFILE_FILES=55");
    println!("TASK5200_SHIPPING_MESSAGES=41");
    println!("TASK5200_SHIPPING_ATTACHMENTS=7");
    println!("TASK5200_SHIPPING_ARCHIVE_BYTES={}", reopened_archive.len());
    println!("TASK5200_SHIPPING_KEY_BYTES={}", reopened_key.len());
    println!(
        "TASK5200_SHIPPING_AUTHENTICATED_BLOCKS={}",
        response.readback.authenticated_block_ids.len()
    );
    println!("TASK5200_SHIPPING_OPAQUE_GRANTS=2");
    println!("TASK5200_SHIPPING_SINGLE_USE_REAUTH=true");
}
