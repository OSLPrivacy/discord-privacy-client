use std::collections::HashMap;

use keystore::service_reply::{
    DirectServiceReplyCalls, ServiceReply, ServiceReplyExpectation, ServiceReplyRefusal,
    TRUNCATED_REPLY, WRONG_CONTENT_TYPE_REPLY, WRONG_SHAPED_REPLY, WRONG_VALUE_REPLY,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

const VALID_READ_BODY: &[u8] =
    br#"{"operation":"read","request_id":"read-3577","item_id":"marked-3577","accepted":true}"#;
const WRONG_VALUE_BODY: &[u8] =
    br#"{"operation":"read","request_id":"read-OTHER","item_id":"marked-3577","accepted":true}"#;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct SavedItem {
    marked: bool,
    private_content: String,
}

#[derive(Clone, Copy)]
struct BadReply {
    kind: &'static str,
    content_type: &'static str,
    declared: usize,
    body: &'static [u8],
}

fn read_expectation() -> ServiceReplyExpectation {
    ServiceReplyExpectation::json(
        "application/json",
        [
            ("accepted", json!(true)),
            ("item_id", json!("marked-3577")),
            ("operation", json!("read")),
            ("request_id", json!("read-3577")),
        ],
    )
}

fn reply<'a>(content_type: &'a str, declared: usize, body: &'a [u8]) -> ServiceReply<'a> {
    ServiceReply {
        content_type: Some(content_type),
        declared_byte_length: Some(declared),
        body,
    }
}

fn bad_replies() -> [BadReply; 4] {
    let wrong_shaped_body = br#"{"operation":"read","request_id":"read-3577","accepted":true}"#;
    let truncated_body =
        br#"{"operation":"read","request_id":"read-3577","item_id":"marked-3577","accepted"#;

    [
        BadReply {
            kind: WRONG_SHAPED_REPLY,
            content_type: "application/json",
            declared: wrong_shaped_body.len(),
            body: wrong_shaped_body,
        },
        BadReply {
            kind: TRUNCATED_REPLY,
            content_type: "application/json",
            declared: VALID_READ_BODY.len(),
            body: truncated_body,
        },
        BadReply {
            kind: WRONG_CONTENT_TYPE_REPLY,
            content_type: "text/html",
            declared: VALID_READ_BODY.len(),
            body: VALID_READ_BODY,
        },
        BadReply {
            kind: WRONG_VALUE_REPLY,
            content_type: "application/json",
            declared: WRONG_VALUE_BODY.len(),
            body: WRONG_VALUE_BODY,
        },
    ]
}

fn one_item_store(private_content: String) -> HashMap<String, SavedItem> {
    HashMap::from([(
        "marked-3577".to_owned(),
        SavedItem {
            marked: true,
            private_content,
        },
    )])
}

fn require_named_refusal<T>(
    bad_reply: BadReply,
    result: Result<T, ServiceReplyRefusal>,
) -> ServiceReplyRefusal {
    match result {
        Err(refusal) => {
            assert_eq!(refusal.name(), bad_reply.kind);
            refusal
        }
        Ok(_) => panic!(
            "TASK3577 bad_reply={} released saved private content",
            bad_reply.kind
        ),
    }
}

#[test]
fn task_3577_bad_read_replies_hide_saved_content_and_preserve_items() {
    let stores_before_save = (0..5)
        .map(|index| one_item_store(format!("TASK3577_PRIVATE_CONTENT_{index}")))
        .collect::<Vec<_>>();
    let saved_file = tempfile::NamedTempFile::new().expect("create saved-item file");
    serde_json::to_writer_pretty(saved_file.as_file(), &stores_before_save)
        .expect("save the five one-item stores");
    let stores: Vec<HashMap<String, SavedItem>> =
        serde_json::from_reader(saved_file.reopen().expect("reopen saved-item file"))
            .expect("load the five one-item stores");

    assert_eq!(stores.len(), 5);
    assert!(stores[0]["marked-3577"].marked);

    let expected = read_expectation();
    let mut reads = DirectServiceReplyCalls::new();
    let mut successful_content_count = 0usize;
    let mut stored_item_counts = Vec::with_capacity(5);

    let valid_content = reads
        .direct_read(
            reply(
                "application/json; charset=utf-8",
                VALID_READ_BODY.len(),
                VALID_READ_BODY,
            ),
            &expected,
            stores[0]["marked-3577"].private_content.as_str(),
        )
        .map(|content| {
            successful_content_count += 1;
            content
        })
        .expect("valid reply must release the marked item's content");
    assert_eq!(valid_content, "TASK3577_PRIVATE_CONTENT_0");
    assert_eq!(successful_content_count, 1);
    assert_eq!(reads.successful_read_count(), 1);
    stored_item_counts.push(stores[0].len());
    assert_eq!(stored_item_counts[0], 1);
    println!(
        "TASK3577 read=valid marked=true successful_content_count={} stored_item_count={}",
        successful_content_count, stored_item_counts[0]
    );

    for (index, bad_reply) in bad_replies().into_iter().enumerate() {
        let store = &stores[index + 1];
        let refusal = require_named_refusal(
            bad_reply,
            reads
                .direct_read(
                    reply(bad_reply.content_type, bad_reply.declared, bad_reply.body),
                    &expected,
                    store["marked-3577"].private_content.as_str(),
                )
                .map(|content| {
                    successful_content_count += 1;
                    content
                }),
        );

        assert_eq!(successful_content_count, 1);
        assert_eq!(reads.successful_read_count(), 1);
        stored_item_counts.push(store.len());
        assert_eq!(stored_item_counts[index + 1], 1);
        println!(
            "TASK3577 read=bad bad_reply={} refusal_name={} successful_content_count={} stored_item_count={}",
            bad_reply.kind,
            refusal.name(),
            successful_content_count,
            stored_item_counts[index + 1]
        );
    }

    assert_eq!(successful_content_count, 1);
    assert_eq!(reads.successful_read_count(), 1);
    assert_eq!(stored_item_counts, vec![1, 1, 1, 1, 1]);
    println!(
        "TASK3577 finish valid_content_count=1 bad_reply_refusal_names=[{},{},{},{}] successful_content_count={} stored_item_counts={stored_item_counts:?}",
        WRONG_SHAPED_REPLY,
        TRUNCATED_REPLY,
        WRONG_CONTENT_TYPE_REPLY,
        WRONG_VALUE_REPLY,
        successful_content_count
    );
}
