use ipc::service_reply_operations::{
    ReplyGatedOperationError, ReplyGatedServiceItems, ServiceOperationRequest,
};
use keystore::service_reply::{
    ServiceReply, TRUNCATED_REPLY, WRONG_CONTENT_TYPE_REPLY, WRONG_SHAPED_REPLY, WRONG_VALUE_REPLY,
};
use sha2::{Digest, Sha256};

const CONTROL_ID: &str = "task3579-control-file";
const CONTROL_REQUEST_ID: &str = "task3579-control-request";
const CONTROL_BYTES: &[u8] = b"TASK3579-CONTROL-DOWNLOAD-BYTES";
const CONTROL_SHA256: &str = "450defe54a4135463325be40e5560f2f814fd2a2fe78d0ea36d3f34457a723c9";
const BAD_REPLY_KINDS: [&str; 4] = [
    WRONG_SHAPED_REPLY,
    TRUNCATED_REPLY,
    WRONG_CONTENT_TYPE_REPLY,
    WRONG_VALUE_REPLY,
];

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

fn request<'a>(request_id: &'a str, item_id: &'a str) -> ServiceOperationRequest<'a> {
    ServiceOperationRequest {
        request_id,
        item_id,
    }
}

fn valid_body(request_id: &str, item_id: &str) -> Vec<u8> {
    format!(
        r#"{{"operation":"download","request_id":"{request_id}","item_id":"{item_id}","accepted":true}}"#
    )
    .into_bytes()
}

fn valid_reply(request_id: &str, item_id: &str) -> ReplyFixture {
    let body = valid_body(request_id, item_id);
    ReplyFixture {
        kind: "valid",
        content_type: "application/json; charset=utf-8",
        declared_length: body.len(),
        body,
    }
}

fn bad_reply(kind: &'static str, request_id: &str, item_id: &str) -> ReplyFixture {
    let valid = valid_body(request_id, item_id);
    match kind {
        WRONG_SHAPED_REPLY => {
            let body = format!(
                r#"{{"operation":"download","request_id":"{request_id}","item_id":"{item_id}"}}"#
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
            content_type: "application/octet-stream",
            declared_length: valid.len(),
            body: valid,
        },
        WRONG_VALUE_REPLY => {
            let body = valid_body("task3579-wrong-request", item_id);
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

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn refusal_name(error: &ReplyGatedOperationError) -> &str {
    match error {
        ReplyGatedOperationError::ReplyRefused(refusal) => refusal.name(),
        other => panic!("expected a named bad-reply refusal, received {other}"),
    }
}

#[test]
fn task_3579_bad_download_replies_keep_exact_control_file() {
    let mut completed = ReplyGatedServiceItems::default();
    let count_before = completed.completed_file_count();
    assert_eq!(count_before, 0);

    let valid = valid_reply(CONTROL_REQUEST_ID, CONTROL_ID);
    let receipt = completed
        .offer_completed_marked_download(
            request(CONTROL_REQUEST_ID, CONTROL_ID),
            valid.as_reply(),
            CONTROL_BYTES,
        )
        .expect("the exact valid reply completes the marked control download");
    let count_after_valid = completed.completed_file_count();
    assert_eq!(valid.kind, "valid");
    assert_eq!(receipt.operation, "download");
    assert_eq!(receipt.request_id, CONTROL_REQUEST_ID);
    assert_eq!(receipt.item_id, CONTROL_ID);
    assert_eq!((count_before, count_after_valid), (0, 1));
    assert_eq!(completed.completed_file_is_marked(CONTROL_ID), Some(true));
    assert_eq!(
        completed.completed_file_bytes(CONTROL_ID),
        Some(CONTROL_BYTES)
    );
    assert_eq!(sha256_hex(CONTROL_BYTES), CONTROL_SHA256);
    assert_eq!(
        completed.completed_file_bytes(CONTROL_ID).map(sha256_hex),
        Some(CONTROL_SHA256.to_owned())
    );
    println!(
        "TASK3579 reply=valid marked=true completed_file_count_before={count_before} completed_file_count_after={count_after_valid} control_bytes=\"{}\" control_byte_length={} control_sha256={CONTROL_SHA256}",
        String::from_utf8_lossy(CONTROL_BYTES),
        CONTROL_BYTES.len()
    );

    for (index, kind) in BAD_REPLY_KINDS.into_iter().enumerate() {
        let fresh_number = index + 1;
        let item_id = format!("task3579-fresh-file-{fresh_number}");
        let request_id = format!("task3579-fresh-request-{fresh_number}");
        let fresh_bytes = format!("TASK3579-FRESH-DOWNLOAD-{fresh_number}").into_bytes();
        let fixture = bad_reply(kind, &request_id, &item_id);
        let before = completed.completed_file_count();
        let error = completed
            .offer_completed_marked_download(
                request(&request_id, &item_id),
                fixture.as_reply(),
                &fresh_bytes,
            )
            .expect_err("a bad reply must not complete a fresh download");
        let after = completed.completed_file_count();
        let named_refusal = refusal_name(&error);
        let control_after = completed
            .completed_file_bytes(CONTROL_ID)
            .expect("the control file must remain completed");
        let fingerprint_after = sha256_hex(control_after);

        assert_eq!(fixture.kind, kind);
        assert_eq!(named_refusal, kind);
        assert!(error.to_string().starts_with(kind));
        assert_eq!((before, after), (1, 1));
        assert!(!completed.offers_completed_file(&item_id));
        assert_eq!(completed.completed_file_is_marked(CONTROL_ID), Some(true));
        assert_eq!(control_after, CONTROL_BYTES);
        assert_eq!(fingerprint_after, CONTROL_SHA256);
        println!(
            "TASK3579 reply={kind} refusal_name={named_refusal} fresh_download={fresh_number} completed_file_count_before={before} completed_file_count_after={after} control_bytes=\"{}\" control_sha256={fingerprint_after}",
            String::from_utf8_lossy(control_after)
        );
    }

    assert_eq!(completed.completed_file_count(), 1);
    assert_eq!(
        completed.completed_file_bytes(CONTROL_ID),
        Some(CONTROL_BYTES)
    );
    println!(
        "TASK3579 finish completed_file_counts=[0,1,1,1,1,1] control_sha256={CONTROL_SHA256} refusal_names=[{},{},{},{}]",
        WRONG_SHAPED_REPLY, TRUNCATED_REPLY, WRONG_CONTENT_TYPE_REPLY, WRONG_VALUE_REPLY
    );
}
