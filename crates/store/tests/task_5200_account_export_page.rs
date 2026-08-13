use store::{MessageStore, StoredMessage};

#[test]
fn account_export_pages_all_41_messages_without_ui_channel_caps() {
    let directory = tempfile::tempdir().unwrap();
    let store = MessageStore::open(directory.path(), &[0x52; 32]).unwrap();
    for index in 0..41 {
        let message = StoredMessage {
            discord_message_id: format!("message-{index:03}"),
            channel_id: format!("channel-{}", index % 4),
            sender_discord_id: format!("sender-{}", index % 3),
            sender_osl_user_id: format!("osl-sender-{}", index % 3),
            plaintext: format!("complete field payload {index}"),
            decrypted_at: 1_800_000_000 + index,
            reply_parent_id: (index % 7 == 0).then(|| "parent-message".to_owned()),
            edit_revision: 1 + index % 3,
            burned: false,
        };
        // `put` derives its authenticated edit revision from the number of
        // committed versions, so seed the same independently expected value.
        for _ in 0..message.edit_revision {
            store.put(&message).unwrap();
        }
    }
    let mut cursor = None;
    let mut pages = Vec::new();
    let mut rows = Vec::new();
    loop {
        let page = store.account_export_page(cursor, 16).unwrap();
        if page.is_empty() {
            break;
        }
        pages.push(page.len());
        cursor = page.last().map(|row| row.cursor);
        rows.extend(page);
    }
    assert_eq!(pages, [16, 16, 9]);
    assert_eq!(rows.len(), 41);
    let mut observed_indexes = Vec::new();
    for row in &rows {
        let index = row.message.discord_message_id[8..]
            .parse::<i64>()
            .expect("seeded message index");
        observed_indexes.push(index);
        assert_eq!(
            row.message.plaintext,
            format!("complete field payload {index}")
        );
        assert_eq!(row.message.edit_revision, 1 + index % 3);
        assert!(row.attachments.is_empty());
    }
    observed_indexes.sort_unstable();
    assert_eq!(observed_indexes, (0..41).collect::<Vec<_>>());
    println!("TASK5200_STORE_PAGES=16,16,9|messages=41|fields=complete");
}
