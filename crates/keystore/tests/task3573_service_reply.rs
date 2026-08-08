use keystore::service_reply::{
    validate_service_reply, ServiceReply, ServiceReplyExpectation, ServiceReplyRefusal,
    TRUNCATED_REPLY, WRONG_CONTENT_TYPE_REPLY, WRONG_SHAPED_REPLY, WRONG_VALUE_REPLY,
};
use serde_json::json;

const VALID_BODY: &[u8] =
    br#"{"operation":"read","request_id":"request-3573","item_id":"item-3573","accepted":true}"#;

fn expectation() -> ServiceReplyExpectation {
    ServiceReplyExpectation::json(
        "application/json",
        [
            ("accepted", json!(true)),
            ("item_id", json!("item-3573")),
            ("operation", json!("read")),
            ("request_id", json!("request-3573")),
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

fn require_refusal(
    bad_reply_name: &str,
    reply: ServiceReply<'_>,
    expected: &ServiceReplyExpectation,
) -> ServiceReplyRefusal {
    match validate_service_reply(reply, expected) {
        Err(refusal) => refusal,
        Ok(_) => panic!("TASK3573 bad_reply={bad_reply_name} was accepted"),
    }
}

#[test]
fn task_3573_valid_reply_passes_and_each_bad_reply_is_refused_by_name() {
    let expected = expectation();
    let valid = validate_service_reply(
        reply(
            "application/json; charset=utf-8",
            VALID_BODY.len(),
            VALID_BODY,
        ),
        &expected,
    )
    .expect("the complete, correctly typed, matching reply must pass");
    assert_eq!(valid.byte_length(), VALID_BODY.len());
    assert_eq!(valid.field("operation"), Some(&json!("read")));
    assert_eq!(valid.field("request_id"), Some(&json!("request-3573")));
    println!(
        "TASK3573 valid_reply=passed byte_length={} field_count={} operation={} request_id={}",
        valid.byte_length(),
        valid.fields().len(),
        valid.field("operation").unwrap(),
        valid.field("request_id").unwrap(),
    );

    let wrong_shaped_body = br#"{"operation":"read","request_id":"request-3573","accepted":true}"#;
    let wrong_shaped = require_refusal(
        WRONG_SHAPED_REPLY,
        reply(
            "application/json",
            wrong_shaped_body.len(),
            wrong_shaped_body,
        ),
        &expected,
    );
    assert_eq!(wrong_shaped.name(), WRONG_SHAPED_REPLY);
    println!(
        "TASK3573 bad_reply=wrong-shaped refusal_name={} detail={}",
        wrong_shaped.name(),
        wrong_shaped.detail()
    );

    let truncated_body = &VALID_BODY[..VALID_BODY.len() - 8];
    let truncated = require_refusal(
        TRUNCATED_REPLY,
        reply("application/json", VALID_BODY.len(), truncated_body),
        &expected,
    );
    assert_eq!(truncated.name(), TRUNCATED_REPLY);
    println!(
        "TASK3573 bad_reply=truncated refusal_name={} declared_bytes={} received_bytes={} detail={}",
        truncated.name(),
        VALID_BODY.len(),
        truncated_body.len(),
        truncated.detail()
    );

    let wrong_content_type = require_refusal(
        WRONG_CONTENT_TYPE_REPLY,
        reply("text/html", VALID_BODY.len(), VALID_BODY),
        &expected,
    );
    assert_eq!(wrong_content_type.name(), WRONG_CONTENT_TYPE_REPLY);
    println!(
        "TASK3573 bad_reply=wrong-content-type refusal_name={} detail={}",
        wrong_content_type.name(),
        wrong_content_type.detail()
    );

    let wrong_value_body = br#"{"operation":"read","request_id":"request-OTHER","item_id":"item-3573","accepted":true}"#;
    let wrong_value = require_refusal(
        WRONG_VALUE_REPLY,
        reply("application/json", wrong_value_body.len(), wrong_value_body),
        &expected,
    );
    assert_eq!(wrong_value.name(), WRONG_VALUE_REPLY);
    println!(
        "TASK3573 bad_reply=wrong-value refusal_name={} detail={}",
        wrong_value.name(),
        wrong_value.detail()
    );
}
