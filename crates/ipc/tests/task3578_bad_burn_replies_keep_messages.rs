use ipc::service_reply_operations::{
    ReplyGatedOperationError, ReplyGatedServiceItems, ServiceOperationRequest,
};
use keystore::service_reply::{
    ServiceReply, TRUNCATED_REPLY, WRONG_CONTENT_TYPE_REPLY, WRONG_SHAPED_REPLY, WRONG_VALUE_REPLY,
};

const COPY_A_NAME: &str = "OSL Copy A";
const COPY_B_NAME: &str = "OSL Copy B";
const COPY_A_MESSAGE_ID: &str = "task3578-copy-a-message";
const COPY_B_MESSAGE_ID: &str = "task3578-copy-b-message";
const COPY_A_REQUEST_ID: &str = "task3578-copy-a-burn-request";
const COPY_B_REQUEST_ID: &str = "task3578-copy-b-burn-request";
const EXACT_TEXT: &str = "TASK3578 exact text survives every refused burn reply";

#[derive(Clone, Copy)]
struct CopyIdentity {
    name: &'static str,
    message_id: &'static str,
    request_id: &'static str,
}

const COPY_IDENTITIES: [CopyIdentity; 2] = [
    CopyIdentity {
        name: COPY_A_NAME,
        message_id: COPY_A_MESSAGE_ID,
        request_id: COPY_A_REQUEST_ID,
    },
    CopyIdentity {
        name: COPY_B_NAME,
        message_id: COPY_B_MESSAGE_ID,
        request_id: COPY_B_REQUEST_ID,
    },
];

struct LocalCopy {
    identity: CopyIdentity,
    items: ReplyGatedServiceItems,
}

struct ReplyFixture {
    kind: &'static str,
    content_type: &'static str,
    declared_length: usize,
    body: Vec<u8>,
}

impl ReplyFixture {
    fn as_reply(&self) -> ServiceReply<'_> {
        ServiceReply {
            content_type: Some(self.content_type),
            declared_byte_length: Some(self.declared_length),
            body: &self.body,
        }
    }
}

fn fresh_pair() -> [LocalCopy; 2] {
    COPY_IDENTITIES.map(|identity| LocalCopy {
        identity,
        items: ReplyGatedServiceItems::with_message_records([(identity.message_id, EXACT_TEXT)]),
    })
}

fn exact_valid_body(identity: CopyIdentity) -> Vec<u8> {
    format!(
        r#"{{"operation":"burn","request_id":"{}","item_id":"{}","accepted":true}}"#,
        identity.request_id, identity.message_id
    )
    .into_bytes()
}

fn valid_reply(identity: CopyIdentity) -> ReplyFixture {
    let body = exact_valid_body(identity);
    ReplyFixture {
        kind: "valid",
        content_type: "application/json; charset=utf-8",
        declared_length: body.len(),
        body,
    }
}

fn bad_reply(identity: CopyIdentity, kind: &'static str) -> ReplyFixture {
    let valid = exact_valid_body(identity);
    match kind {
        WRONG_SHAPED_REPLY => {
            let body = format!(
                r#"{{"operation":"burn","request_id":"{}","item_id":"{}"}}"#,
                identity.request_id, identity.message_id
            )
            .into_bytes();
            ReplyFixture {
                kind,
                content_type: "application/json",
                declared_length: body.len(),
                body,
            }
        }
        TRUNCATED_REPLY => ReplyFixture {
            kind,
            content_type: "application/json",
            declared_length: valid.len(),
            body: valid[..valid.len() - 1].to_vec(),
        },
        WRONG_CONTENT_TYPE_REPLY => ReplyFixture {
            kind,
            content_type: "text/html",
            declared_length: valid.len(),
            body: valid,
        },
        WRONG_VALUE_REPLY => {
            let body = format!(
                r#"{{"operation":"burn","request_id":"{}","item_id":"task3578-wrong-message","accepted":true}}"#,
                identity.request_id
            )
            .into_bytes();
            ReplyFixture {
                kind,
                content_type: "application/json",
                declared_length: body.len(),
                body,
            }
        }
        other => panic!("unknown bad reply kind {other}"),
    }
}

fn request(identity: CopyIdentity) -> ServiceOperationRequest<'static> {
    ServiceOperationRequest {
        request_id: identity.request_id,
        item_id: identity.message_id,
    }
}

fn refusal_name(error: &ReplyGatedOperationError) -> &str {
    match error {
        ReplyGatedOperationError::ReplyRefused(refusal) => refusal.name(),
        other => panic!("expected reply refusal, received {other}"),
    }
}

fn pair_counts(copies: &[LocalCopy; 2]) -> (usize, usize) {
    (
        copies[0].items.message_count(),
        copies[1].items.message_count(),
    )
}

fn pair_exact_texts(copies: &[LocalCopy; 2]) -> (Option<&str>, Option<&str>) {
    (
        copies[0].items.message_text(copies[0].identity.message_id),
        copies[1].items.message_text(copies[1].identity.message_id),
    )
}

#[test]
fn task_3578_valid_burn_removes_both_copies_and_bad_replies_keep_exact_text() {
    let mut valid_pair = fresh_pair();
    assert_eq!(pair_counts(&valid_pair), (1, 1));
    assert_eq!(
        pair_exact_texts(&valid_pair),
        (Some(EXACT_TEXT), Some(EXACT_TEXT))
    );
    let valid_before = pair_counts(&valid_pair);
    for copy in &mut valid_pair {
        let fixture = valid_reply(copy.identity);
        let receipt = copy
            .items
            .burn(request(copy.identity), fixture.as_reply())
            .expect("each copy accepts its exact valid burn reply");
        assert_eq!(fixture.kind, "valid");
        assert_eq!(receipt.operation, "burn");
        assert_eq!(receipt.request_id, copy.identity.request_id);
        assert_eq!(receipt.item_id, copy.identity.message_id);
    }
    let valid_after = pair_counts(&valid_pair);
    assert_eq!(valid_before, (1, 1));
    assert_eq!(valid_after, (0, 0));
    assert_eq!(pair_exact_texts(&valid_pair), (None, None));
    println!(
        "TASK3578 reply=valid copies=\"{}\",\"{}\" pair_before={},{} pair_after={},{} exact_text_after=absent,absent",
        valid_pair[0].identity.name,
        valid_pair[1].identity.name,
        valid_before.0,
        valid_before.1,
        valid_after.0,
        valid_after.1
    );

    for kind in [
        WRONG_SHAPED_REPLY,
        TRUNCATED_REPLY,
        WRONG_CONTENT_TYPE_REPLY,
        WRONG_VALUE_REPLY,
    ] {
        let mut pair = fresh_pair();
        let before = pair_counts(&pair);
        assert_eq!(before, (1, 1), "{kind} must start with two fresh copies");
        assert_eq!(
            pair_exact_texts(&pair),
            (Some(EXACT_TEXT), Some(EXACT_TEXT)),
            "{kind} fresh copies must contain the exact text"
        );

        let mut refusals = Vec::new();
        for copy in &mut pair {
            let fixture = bad_reply(copy.identity, kind);
            let error = copy
                .items
                .burn(request(copy.identity), fixture.as_reply())
                .expect_err("a bad reply must not authorize either copy's burn");
            assert_eq!(fixture.kind, kind);
            assert_eq!(refusal_name(&error), kind);
            assert!(
                error.to_string().starts_with(kind),
                "refusal text must name {kind}: {error}"
            );
            refusals.push(refusal_name(&error).to_owned());
        }

        let after = pair_counts(&pair);
        let exact_texts = pair_exact_texts(&pair);
        assert_eq!(after, (1, 1), "{kind} changed a message count");
        assert_eq!(
            exact_texts,
            (Some(EXACT_TEXT), Some(EXACT_TEXT)),
            "{kind} changed exact message text"
        );
        println!(
            "TASK3578 reply={} copies=\"{}\",\"{}\" refusals={},{} pair_before={},{} pair_after={},{} copy_a=\"{}\" copy_b=\"{}\"",
            kind,
            pair[0].identity.name,
            pair[1].identity.name,
            refusals[0],
            refusals[1],
            before.0,
            before.1,
            after.0,
            after.1,
            exact_texts.0.expect("copy A exact text"),
            exact_texts.1.expect("copy B exact text")
        );
    }
}
