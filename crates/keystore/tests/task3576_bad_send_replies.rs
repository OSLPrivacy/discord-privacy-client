use keystore::service_reply::{
    DirectServiceReplyCalls, ServiceReply, ServiceReplyExpectation, ServiceReplyRefusal,
    TRUNCATED_REPLY, WRONG_CONTENT_TYPE_REPLY, WRONG_SHAPED_REPLY, WRONG_VALUE_REPLY,
};
use serde_json::json;

const CONTROL_MARK: &str = "TASK3576_MARK_CONTROL";

#[derive(Clone, Copy)]
struct BadMarkedSend {
    kind: &'static str,
    request_id: &'static str,
    item_id: &'static str,
    mark: &'static str,
    content_type: &'static str,
    declared_byte_length: usize,
    body: &'static [u8],
}

#[derive(Default)]
struct MarkedSendFixture {
    sender: DirectServiceReplyCalls,
    receiver_marks: Vec<&'static str>,
}

impl MarkedSendFixture {
    fn send_marked(
        &mut self,
        request_id: &str,
        item_id: &str,
        mark: &'static str,
        reply: ServiceReply<'_>,
    ) -> Result<(), ServiceReplyRefusal> {
        self.sender
            .direct_send(reply, &send_expectation(request_id, item_id))?;
        self.receiver_marks.push(mark);
        Ok(())
    }

    fn receiver_marked_count(&self) -> usize {
        self.receiver_marks.len()
    }
}

fn send_expectation(request_id: &str, item_id: &str) -> ServiceReplyExpectation {
    ServiceReplyExpectation::json(
        "application/json",
        [
            ("accepted", json!(true)),
            ("item_id", json!(item_id)),
            ("operation", json!("send")),
            ("request_id", json!(request_id)),
        ],
    )
}

fn reply<'a>(
    content_type: &'a str,
    declared_byte_length: usize,
    body: &'a [u8],
) -> ServiceReply<'a> {
    ServiceReply {
        content_type: Some(content_type),
        declared_byte_length: Some(declared_byte_length),
        body,
    }
}

fn bad_marked_sends() -> [BadMarkedSend; 4] {
    const WRONG_SHAPED_BODY: &[u8] =
        br#"{"operation":"send","request_id":"send-3576-shape","accepted":true}"#;
    const TRUNCATED_BODY: &[u8] = br#"{"operation":"send","request_id":"send-3576-truncated","item_id":"message-3576-truncated","accepted"#;
    const TRUNCATED_DECLARED_LENGTH: usize = br#"{"operation":"send","request_id":"send-3576-truncated","item_id":"message-3576-truncated","accepted":true}"#.len();
    const WRONG_CONTENT_TYPE_BODY: &[u8] = br#"{"operation":"send","request_id":"send-3576-content-type","item_id":"message-3576-content-type","accepted":true}"#;
    const WRONG_VALUE_BODY: &[u8] = br#"{"operation":"send","request_id":"send-3576-wrong-value","item_id":"message-3576-wrong-value","accepted":false}"#;

    [
        BadMarkedSend {
            kind: WRONG_SHAPED_REPLY,
            request_id: "send-3576-shape",
            item_id: "message-3576-shape",
            mark: "TASK3576_MARK_WRONG_SHAPED",
            content_type: "application/json",
            declared_byte_length: WRONG_SHAPED_BODY.len(),
            body: WRONG_SHAPED_BODY,
        },
        BadMarkedSend {
            kind: TRUNCATED_REPLY,
            request_id: "send-3576-truncated",
            item_id: "message-3576-truncated",
            mark: "TASK3576_MARK_TRUNCATED",
            content_type: "application/json",
            declared_byte_length: TRUNCATED_DECLARED_LENGTH,
            body: TRUNCATED_BODY,
        },
        BadMarkedSend {
            kind: WRONG_CONTENT_TYPE_REPLY,
            request_id: "send-3576-content-type",
            item_id: "message-3576-content-type",
            mark: "TASK3576_MARK_WRONG_CONTENT_TYPE",
            content_type: "text/html",
            declared_byte_length: WRONG_CONTENT_TYPE_BODY.len(),
            body: WRONG_CONTENT_TYPE_BODY,
        },
        BadMarkedSend {
            kind: WRONG_VALUE_REPLY,
            request_id: "send-3576-wrong-value",
            item_id: "message-3576-wrong-value",
            mark: "TASK3576_MARK_WRONG_VALUE",
            content_type: "application/json",
            declared_byte_length: WRONG_VALUE_BODY.len(),
            body: WRONG_VALUE_BODY,
        },
    ]
}

#[test]
fn task_3576_bad_replies_do_not_deliver_fresh_marked_sends() {
    const CONTROL_REQUEST_ID: &str = "send-3576-control";
    const CONTROL_ITEM_ID: &str = "message-3576-control";
    const CONTROL_BODY: &[u8] = br#"{"operation":"send","request_id":"send-3576-control","item_id":"message-3576-control","accepted":true}"#;

    let mut fixture = MarkedSendFixture::default();
    assert_eq!(fixture.receiver_marked_count(), 0);
    assert_eq!(fixture.sender.successful_send_count(), 0);
    println!(
        "TASK3576 initial receiver_marked_count={} sender_success_count={}",
        fixture.receiver_marked_count(),
        fixture.sender.successful_send_count()
    );

    fixture
        .send_marked(
            CONTROL_REQUEST_ID,
            CONTROL_ITEM_ID,
            CONTROL_MARK,
            reply(
                "application/json; charset=utf-8",
                CONTROL_BODY.len(),
                CONTROL_BODY,
            ),
        )
        .expect("the valid control reply must deliver exactly one marked send");
    assert_eq!(fixture.receiver_marks, [CONTROL_MARK]);
    assert_eq!(fixture.receiver_marked_count(), 1);
    assert_eq!(fixture.sender.successful_send_count(), 1);
    println!(
        "TASK3576 control mark={} receiver_marked_count={} sender_success_count={}",
        CONTROL_MARK,
        fixture.receiver_marked_count(),
        fixture.sender.successful_send_count()
    );

    let mut refusal_kinds = Vec::new();
    for bad_send in bad_marked_sends() {
        let refusal = match fixture.send_marked(
                bad_send.request_id,
                bad_send.item_id,
                bad_send.mark,
                reply(
                    bad_send.content_type,
                    bad_send.declared_byte_length,
                    bad_send.body,
                ),
            ) {
            Err(refusal) => refusal,
            Ok(()) => panic!(
                "TASK3576 bad reply kind={} delivered an extra marked send: receiver_marked_count={} sender_success_count={}",
                bad_send.kind,
                fixture.receiver_marked_count(),
                fixture.sender.successful_send_count()
            ),
        };
        let refusal_text = refusal.to_string();

        assert_eq!(refusal.name(), bad_send.kind);
        assert!(
            refusal_text.starts_with(&format!("{}:", bad_send.kind)),
            "refusal did not name bad reply kind {}: {refusal_text}",
            bad_send.kind
        );
        assert_eq!(fixture.receiver_marks, [CONTROL_MARK]);
        assert_eq!(fixture.receiver_marked_count(), 1);
        assert_eq!(fixture.sender.successful_send_count(), 1);
        refusal_kinds.push(bad_send.kind);

        println!(
            "TASK3576 refused kind={} request_id={} mark={} refusal=\"{}\" receiver_marked_count={} sender_success_count={}",
            bad_send.kind,
            bad_send.request_id,
            bad_send.mark,
            refusal_text,
            fixture.receiver_marked_count(),
            fixture.sender.successful_send_count()
        );
    }

    assert_eq!(
        refusal_kinds,
        [
            WRONG_SHAPED_REPLY,
            TRUNCATED_REPLY,
            WRONG_CONTENT_TYPE_REPLY,
            WRONG_VALUE_REPLY,
        ]
    );
    assert_eq!(fixture.receiver_marked_count(), 1);
    assert_eq!(fixture.sender.successful_send_count(), 1);
    println!(
        "TASK3576 finish control_valid=1 bad_replies_refused={} receiver_marked_count={} sender_success_count={} refusal_kinds={}",
        refusal_kinds.len(),
        fixture.receiver_marked_count(),
        fixture.sender.successful_send_count(),
        refusal_kinds.join(",")
    );
}
