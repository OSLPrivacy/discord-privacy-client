use keystore::service_reply::{
    DirectServiceReplyCalls, ServiceReply, ServiceReplyExpectation, ServiceReplyRefusal,
    TRUNCATED_REPLY, WRONG_CONTENT_TYPE_REPLY, WRONG_SHAPED_REPLY, WRONG_VALUE_REPLY,
};
use serde_json::json;

const VALID_SEND_BODY: &[u8] =
    br#"{"operation":"send","request_id":"send-3574","item_id":"message-3574","accepted":true}"#;
const VALID_READ_BODY: &[u8] =
    br#"{"operation":"read","request_id":"read-3574","item_id":"message-3574","accepted":true}"#;
const PRIVATE_CONTENT: &str = "TASK3574_PRIVATE_CONTENT";

fn expectation(operation: &str, request_id: &str) -> ServiceReplyExpectation {
    ServiceReplyExpectation::json(
        "application/json",
        [
            ("accepted", json!(true)),
            ("item_id", json!("message-3574")),
            ("operation", json!(operation)),
            ("request_id", json!(request_id)),
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

#[derive(Clone, Copy)]
struct BadReply {
    name: &'static str,
    content_type: &'static str,
    declared: usize,
    body: &'static [u8],
}

fn bad_replies(valid_body: &'static [u8], wrong_value_body: &'static [u8]) -> [BadReply; 4] {
    let wrong_shaped_body = if valid_body == VALID_SEND_BODY {
        br#"{"operation":"send","request_id":"send-3574","accepted":true}"#
    } else {
        br#"{"operation":"read","request_id":"read-3574","accepted":true}"#
    };
    let truncated_body = if valid_body == VALID_SEND_BODY {
        br#"{"operation":"send","request_id":"send-3574","item_id":"message-3574","accepted"#
    } else {
        br#"{"operation":"read","request_id":"read-3574","item_id":"message-3574","accepted"#
    };

    [
        BadReply {
            name: WRONG_SHAPED_REPLY,
            content_type: "application/json",
            declared: wrong_shaped_body.len(),
            body: wrong_shaped_body,
        },
        BadReply {
            name: TRUNCATED_REPLY,
            content_type: "application/json",
            declared: valid_body.len(),
            body: truncated_body,
        },
        BadReply {
            name: WRONG_CONTENT_TYPE_REPLY,
            content_type: "text/html",
            declared: valid_body.len(),
            body: valid_body,
        },
        BadReply {
            name: WRONG_VALUE_REPLY,
            content_type: "application/json",
            declared: wrong_value_body.len(),
            body: wrong_value_body,
        },
    ]
}

fn require_refusal<T>(
    operation: &str,
    bad_reply: BadReply,
    result: Result<T, ServiceReplyRefusal>,
) -> ServiceReplyRefusal {
    match result {
        Err(refusal) => {
            assert_eq!(refusal.name(), bad_reply.name);
            refusal
        }
        Ok(_) => panic!(
            "TASK3574 operation={operation} bad_reply={} was accepted",
            bad_reply.name
        ),
    }
}

#[test]
fn task_3574_direct_send_and_read_gate_success_and_all_bad_replies() {
    let send_expected = expectation("send", "send-3574");
    let read_expected = expectation("read", "read-3574");
    let mut calls = DirectServiceReplyCalls::new();

    let valid_send = calls
        .direct_send(
            reply(
                "application/json; charset=utf-8",
                VALID_SEND_BODY.len(),
                VALID_SEND_BODY,
            ),
            &send_expected,
        )
        .expect("the valid direct-send reply must be accepted");
    assert_eq!(valid_send.byte_length(), VALID_SEND_BODY.len());
    assert_eq!(calls.successful_send_count(), 1);
    println!(
        "TASK3574 operation=send valid_reply=accepted success_count={} byte_length={}",
        calls.successful_send_count(),
        valid_send.byte_length()
    );

    let wrong_send_value = br#"{"operation":"send","request_id":"send-OTHER","item_id":"message-3574","accepted":true}"#;
    let mut send_refusal_count = 0;
    for bad_reply in bad_replies(VALID_SEND_BODY, wrong_send_value) {
        let count_before = calls.successful_send_count();
        let refusal = require_refusal(
            "send",
            bad_reply,
            calls.direct_send(
                reply(bad_reply.content_type, bad_reply.declared, bad_reply.body),
                &send_expected,
            ),
        );
        let count_after = calls.successful_send_count();
        assert_eq!(count_after, count_before);
        send_refusal_count += 1;
        println!(
            "TASK3574 operation=send bad_reply={} refusal_name={} success_count_before={} success_count_after={}",
            bad_reply.name,
            refusal.name(),
            count_before,
            count_after
        );
    }

    let private_content_release_count = std::cell::Cell::new(0usize);
    let valid_private_content = calls
        .direct_read(
            reply(
                "application/json; charset=utf-8",
                VALID_READ_BODY.len(),
                VALID_READ_BODY,
            ),
            &read_expected,
            PRIVATE_CONTENT,
        )
        .map(|content| {
            private_content_release_count.set(private_content_release_count.get() + 1);
            content
        })
        .expect("the valid direct-read reply must release private content");
    assert_eq!(valid_private_content, PRIVATE_CONTENT);
    assert_eq!(calls.successful_read_count(), 1);
    println!(
        "TASK3574 operation=read valid_reply=accepted success_count={} private_content_release_count={}",
        calls.successful_read_count(),
        private_content_release_count.get()
    );

    let wrong_read_value = br#"{"operation":"read","request_id":"read-OTHER","item_id":"message-3574","accepted":true}"#;
    let mut read_refusal_count = 0;
    for bad_reply in bad_replies(VALID_READ_BODY, wrong_read_value) {
        let count_before = calls.successful_read_count();
        let refusal = require_refusal(
            "read",
            bad_reply,
            calls
                .direct_read(
                    reply(bad_reply.content_type, bad_reply.declared, bad_reply.body),
                    &read_expected,
                    PRIVATE_CONTENT,
                )
                .map(|content| {
                    private_content_release_count.set(private_content_release_count.get() + 1);
                    content
                }),
        );
        let count_after = calls.successful_read_count();
        assert_eq!(count_after, count_before);
        assert_eq!(private_content_release_count.get(), 1);
        read_refusal_count += 1;
        println!(
            "TASK3574 operation=read bad_reply={} refusal_name={} success_count_before={} success_count_after={} private_content_release_count={}",
            bad_reply.name,
            refusal.name(),
            count_before,
            count_after,
            private_content_release_count.get()
        );
    }

    assert_eq!(send_refusal_count, 4);
    assert_eq!(read_refusal_count, 4);
    assert_eq!(calls.successful_send_count(), 1);
    assert_eq!(calls.successful_read_count(), 1);
    assert_eq!(private_content_release_count.get(), 1);
    println!(
        "TASK3574 finish send_valid=1 send_bad_refused={} send_success_count={} read_valid=1 read_bad_refused={} read_success_count={} private_content_release_count={}",
        send_refusal_count,
        calls.successful_send_count(),
        read_refusal_count,
        calls.successful_read_count(),
        private_content_release_count.get()
    );
}
