use crypto::{ml_kem_768, x25519};
use ipc::ordinary_sync::{
    encrypt_sync_payload_v3, server_visible_v3_header, DeleteMarker, FieldValue, FieldWiseState,
    ListChange, ListRow, OrdinaryListState, OrdinarySyncPayload, VersionStamp,
    ADDED_SERVER_VISIBLE_MESSAGE_KINDS,
};
use ipc::wire_v2::{decrypt_v3, encrypt_v3, RecipientV3, MSG_TYPE_CONTENT, WIRE_VERSION_V3};
use serde_json::{json, Value};
use std::collections::BTreeMap;

fn stamp(counter: u64, device: &str) -> VersionStamp {
    VersionStamp::new(counter, device)
}

fn row(id: &str, counter: u64, device: &str) -> ListRow {
    ListRow::new(
        id,
        json!({ "row": id, "from": device }),
        stamp(counter, device),
    )
}

fn marker(id: &str, counter: u64, device: &str) -> DeleteMarker {
    DeleteMarker::new(id, stamp(counter, device))
}

fn row_ids(
    prefix: &str,
    range: std::ops::Range<u8>,
    counter_base: u64,
    device: &str,
) -> Vec<ListRow> {
    range
        .map(|ix| {
            row(
                &format!("{prefix}-{ix:02}"),
                counter_base + u64::from(ix),
                device,
            )
        })
        .collect()
}

fn apply_order(changes: &[ListChange], order: &[usize]) -> OrdinaryListState {
    let mut state = OrdinaryListState::default();
    for ix in order {
        state.apply(changes[*ix].clone());
    }
    state
}

fn state_from_field_values(values: &[(&str, &str)]) -> FieldWiseState {
    FieldWiseState::from_fields(values.iter().map(|(name, value)| {
        (
            (*name).to_string(),
            FieldValue::new(json!(value), stamp(1, "base")),
        )
    }))
}

fn expect_value(value: impl Into<Value>) -> Value {
    value.into()
}

#[test]
fn task4807_ordinary_merge_rules_finish_line() {
    let shared = row_ids("shared", 0..4, 10, "base");
    let mut laptop_rows = shared.clone();
    laptop_rows.extend(row_ids("laptop", 0..8, 100, "laptop"));
    let mut desktop_rows = shared;
    desktop_rows.extend(row_ids("desktop", 0..5, 200, "desktop"));

    let laptop_list = OrdinaryListState::from_rows(laptop_rows);
    let desktop_list = OrdinaryListState::from_rows(desktop_rows);
    assert_eq!(laptop_list.live_row_count(), 12);
    assert_eq!(desktop_list.live_row_count(), 9);
    let merged_list = laptop_list.merge(&desktop_list);
    let merged_reverse = desktop_list.merge(&laptop_list);
    assert_eq!(merged_list.live_row_count(), 17);
    assert_eq!(merged_reverse.live_row_count(), 17);
    println!("TASK4807 list_left_rows={}", laptop_list.live_row_count());
    println!("TASK4807 list_right_rows={}", desktop_list.live_row_count());
    println!("TASK4807 list_shared_rows=4");
    println!("TASK4807 list_merged_rows={}", merged_list.live_row_count());

    let edit_then_delete = vec![
        ListChange::Upsert(row("thread-42", 90, "laptop")),
        ListChange::Delete(marker("thread-42", 10, "desktop")),
    ];
    let delete_then_edit = vec![
        ListChange::Delete(marker("thread-43", 10, "desktop")),
        ListChange::Upsert(row("thread-43", 90, "laptop")),
    ];
    let shuffle_orders: [&[usize]; 20] = [
        &[0, 1],
        &[1, 0],
        &[0, 1, 0],
        &[1, 0, 1],
        &[0, 0, 1],
        &[1, 1, 0],
        &[0, 1, 1],
        &[1, 0, 0],
        &[0, 1, 0, 1],
        &[1, 0, 1, 0],
        &[0, 0, 1, 1],
        &[1, 1, 0, 0],
        &[0, 1, 1, 0],
        &[1, 0, 0, 1],
        &[0, 0, 0, 1],
        &[1, 1, 1, 0],
        &[0, 1, 0, 0],
        &[1, 0, 1, 1],
        &[0, 1, 1, 1],
        &[1, 0, 0, 0],
    ];
    let mut delete_exceptions = 0usize;
    for order in shuffle_orders {
        let edit_delete_state = apply_order(&edit_then_delete, order);
        let delete_edit_state = apply_order(&delete_then_edit, order);
        if edit_delete_state.contains_row("thread-42")
            || edit_delete_state.delete_marker_count() != 1
            || delete_edit_state.contains_row("thread-43")
            || delete_edit_state.delete_marker_count() != 1
        {
            delete_exceptions += 1;
        }
    }
    assert_eq!(delete_exceptions, 0);
    let final_delete_then_edit = apply_order(&delete_then_edit, &[0, 1]);
    let final_edit_then_delete = apply_order(&edit_then_delete, &[0, 1]);
    println!(
        "TASK4807 delete_then_edit_row_gone={}",
        !final_delete_then_edit.contains_row("thread-43")
    );
    println!(
        "TASK4807 delete_then_edit_markers={}",
        final_delete_then_edit.delete_marker_count()
    );
    println!(
        "TASK4807 edit_then_delete_row_gone={}",
        !final_edit_then_delete.contains_row("thread-42")
    );
    println!(
        "TASK4807 edit_then_delete_markers={}",
        final_edit_then_delete.delete_marker_count()
    );
    println!(
        "TASK4807 shuffled_orderings_checked={}",
        shuffle_orders.len()
    );
    println!("TASK4807 shuffled_ordering_exceptions={delete_exceptions}");

    let base_fields = [
        ("theme", "light"),
        ("notifications", "quiet"),
        ("font_size", "medium"),
        ("language", "en"),
        ("status", "available"),
        ("avatar", "old"),
        ("timezone", "UTC"),
    ];
    let mut laptop_settings = state_from_field_values(&base_fields);
    let mut desktop_settings = state_from_field_values(&base_fields);
    let laptop_changes = [
        ("theme", json!("dark")),
        ("font_size", json!("large")),
        ("language", json!("fr")),
    ];
    let desktop_changes = [
        ("notifications", json!("loud")),
        ("status", json!("busy")),
        ("avatar", json!("new")),
        ("timezone", json!("America/Los_Angeles")),
    ];
    for (ix, (name, value)) in laptop_changes.iter().enumerate() {
        laptop_settings.set_field(*name, value.clone(), stamp(10 + ix as u64, "laptop"));
    }
    for (ix, (name, value)) in desktop_changes.iter().enumerate() {
        desktop_settings.set_field(*name, value.clone(), stamp(20 + ix as u64, "desktop"));
    }
    let merged_settings = laptop_settings.merge(&desktop_settings);
    let merged_settings_reverse = desktop_settings.merge(&laptop_settings);
    assert_eq!(merged_settings, merged_settings_reverse);
    let mut expected = BTreeMap::new();
    for (name, value) in laptop_changes.iter().chain(desktop_changes.iter()) {
        expected.insert((*name).to_string(), value.clone());
    }
    let reverted = merged_settings.reverted_field_count(&expected);
    assert_eq!(merged_settings.field_count(), 7);
    assert_eq!(reverted, 0);
    println!(
        "TASK4807 settings_left_changed_fields={}",
        laptop_changes.len()
    );
    println!(
        "TASK4807 settings_right_changed_fields={}",
        desktop_changes.len()
    );
    println!("TASK4807 settings_merged_changed_fields={}", expected.len());
    println!("TASK4807 settings_reverted_fields={reverted}");

    let mut earlier = state_from_field_values(&[("theme", "light")]);
    earlier.set_field("theme", json!("blue"), stamp(30, "laptop"));
    let mut later = state_from_field_values(&[("theme", "light")]);
    later.set_field("theme", json!("green"), stamp(31, "desktop"));
    let same_field_lr = earlier.merge(&later);
    let same_field_rl = later.merge(&earlier);
    assert_eq!(same_field_lr.get("theme"), Some(&expect_value("green")));
    assert_eq!(same_field_rl.get("theme"), Some(&expect_value("green")));
    println!(
        "TASK4807 same_field_left_then_right={}",
        same_field_lr.get("theme").unwrap()
    );
    println!(
        "TASK4807 same_field_right_then_left={}",
        same_field_rl.get("theme").unwrap()
    );

    let (sender_sk, sender_pk) = x25519::generate_keypair();
    let (recipient_sk, recipient_pk) = x25519::generate_keypair();
    let (recipient_mlkem_sk, recipient_mlkem_pk) = ml_kem_768::generate_keypair();
    let recipients = [RecipientV3 {
        x25519_pub: recipient_pk,
        mlkem_pub: recipient_mlkem_pk,
    }];

    let ordinary_wire = encrypt_v3(
        &sender_sk,
        &sender_pk,
        &recipients,
        MSG_TYPE_CONTENT,
        b"ordinary user-visible message",
    )
    .expect("ordinary v3 content wire");
    let sync_payload = OrdinarySyncPayload::new(
        "laptop-device",
        "desktop-device",
        json!({ "op": "ordinary-sync", "rows": 17 }),
    );
    let sync_wire =
        encrypt_sync_payload_v3(&sync_payload, &sender_sk, &sender_pk, &recipients).unwrap();
    let ordinary_header = server_visible_v3_header(&ordinary_wire).unwrap();
    let sync_header = server_visible_v3_header(&sync_wire).unwrap();
    assert_eq!(ordinary_header, sync_header);
    assert_eq!(sync_header.wire_version, WIRE_VERSION_V3);
    assert_eq!(sync_header.message_kind, MSG_TYPE_CONTENT);
    assert_eq!(ADDED_SERVER_VISIBLE_MESSAGE_KINDS.len(), 0);
    let opened_sync = decrypt_v3(&sync_wire, &recipient_sk, &recipient_mlkem_sk).unwrap();
    assert_eq!(opened_sync.msg_type, MSG_TYPE_CONTENT);
    let decoded_sync: OrdinarySyncPayload = serde_json::from_slice(&opened_sync.plaintext).unwrap();
    assert_eq!(decoded_sync, sync_payload);
    println!(
        "TASK4807 captured_sync_wire_version={}",
        sync_header.wire_version
    );
    println!(
        "TASK4807 captured_sync_message_kind={}",
        sync_header.message_kind
    );
    println!(
        "TASK4807 ordinary_message_kind={}",
        ordinary_header.message_kind
    );
    println!(
        "TASK4807 captured_sync_header_bytes={}",
        sync_header.header_bytes
    );
    println!(
        "TASK4807 ordinary_header_bytes={}",
        ordinary_header.header_bytes
    );
    println!(
        "TASK4807 added_server_visible_message_kinds={}",
        ADDED_SERVER_VISIBLE_MESSAGE_KINDS.len()
    );
}
