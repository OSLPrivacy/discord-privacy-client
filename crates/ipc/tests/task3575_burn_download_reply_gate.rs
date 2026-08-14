use ipc::service_reply_operations::{
    ReplyGatedOperationError, ReplyGatedServiceItems, ServiceOperationRequest,
};
use keystore::service_reply::{
    ServiceReply, TRUNCATED_REPLY, WRONG_CONTENT_TYPE_REPLY, WRONG_SHAPED_REPLY, WRONG_VALUE_REPLY,
};

const BURN_ITEM: &str = "message-3575";
const BURN_REQUEST: &str = "burn-request-3575";
const DOWNLOAD_ITEM: &str = "file-3575";
const DOWNLOAD_REQUEST: &str = "download-request-3575";

#[derive(Clone, Copy)]
struct ReplyFixture {
    name: &'static str,
    content_type: &'static str,
    declared_length: usize,
    body: &'static [u8],
}

fn valid_burn_reply() -> ReplyFixture {
    let body = br#"{"operation":"burn","request_id":"burn-request-3575","item_id":"message-3575","accepted":true}"#;
    ReplyFixture {
        name: "valid",
        content_type: "application/json; charset=utf-8",
        declared_length: body.len(),
        body,
    }
}

fn valid_download_reply() -> ReplyFixture {
    let body = br#"{"operation":"download","request_id":"download-request-3575","item_id":"file-3575","accepted":true}"#;
    ReplyFixture {
        name: "valid",
        content_type: "application/json",
        declared_length: body.len(),
        body,
    }
}

fn burn_bad_replies() -> [ReplyFixture; 4] {
    let valid = valid_burn_reply();
    let wrong_shaped =
        br#"{"operation":"burn","request_id":"burn-request-3575","item_id":"message-3575"}"#;
    let wrong_value = br#"{"operation":"burn","request_id":"burn-request-3575","item_id":"other-message","accepted":true}"#;
    [
        ReplyFixture {
            name: WRONG_SHAPED_REPLY,
            content_type: "application/json",
            declared_length: wrong_shaped.len(),
            body: wrong_shaped,
        },
        ReplyFixture {
            name: TRUNCATED_REPLY,
            content_type: "application/json",
            declared_length: valid.body.len(),
            body: &valid.body[..valid.body.len() - 1],
        },
        ReplyFixture {
            name: WRONG_CONTENT_TYPE_REPLY,
            content_type: "text/html",
            declared_length: valid.body.len(),
            body: valid.body,
        },
        ReplyFixture {
            name: WRONG_VALUE_REPLY,
            content_type: "application/json",
            declared_length: wrong_value.len(),
            body: wrong_value,
        },
    ]
}

fn download_bad_replies() -> [ReplyFixture; 4] {
    let valid = valid_download_reply();
    let wrong_shaped =
        br#"{"operation":"download","request_id":"download-request-3575","item_id":"file-3575"}"#;
    let wrong_value = br#"{"operation":"download","request_id":"other-request","item_id":"file-3575","accepted":true}"#;
    [
        ReplyFixture {
            name: WRONG_SHAPED_REPLY,
            content_type: "application/json",
            declared_length: wrong_shaped.len(),
            body: wrong_shaped,
        },
        ReplyFixture {
            name: TRUNCATED_REPLY,
            content_type: "application/json",
            declared_length: valid.body.len(),
            body: &valid.body[..valid.body.len() - 1],
        },
        ReplyFixture {
            name: WRONG_CONTENT_TYPE_REPLY,
            content_type: "application/octet-stream",
            declared_length: valid.body.len(),
            body: valid.body,
        },
        ReplyFixture {
            name: WRONG_VALUE_REPLY,
            content_type: "application/json",
            declared_length: wrong_value.len(),
            body: wrong_value,
        },
    ]
}

fn raw_reply(fixture: ReplyFixture) -> ServiceReply<'static> {
    ServiceReply {
        content_type: Some(fixture.content_type),
        declared_byte_length: Some(fixture.declared_length),
        body: fixture.body,
    }
}

fn refusal_name(error: ReplyGatedOperationError) -> String {
    match error {
        ReplyGatedOperationError::ReplyRefused(refusal) => refusal.name().to_owned(),
        other => panic!("expected reply refusal, received {other}"),
    }
}

#[test]
fn task_3575_direct_burn_and_download_gate_item_changes_on_valid_replies() {
    let burn_request = ServiceOperationRequest {
        request_id: BURN_REQUEST,
        item_id: BURN_ITEM,
    };
    let mut valid_burn_items = ReplyGatedServiceItems::with_messages([BURN_ITEM]);
    let burn_before = valid_burn_items.message_count();
    let valid_burn = valid_burn_items
        .burn(burn_request, raw_reply(valid_burn_reply()))
        .expect("the direct burn call accepts its exact valid reply");
    let burn_after = valid_burn_items.message_count();
    assert_eq!(valid_burn.operation, "burn");
    assert_eq!(valid_burn.request_id, BURN_REQUEST);
    assert_eq!(valid_burn.item_id, BURN_ITEM);
    assert_eq!((burn_before, burn_after), (1, 0));
    assert!(!valid_burn_items.has_message(BURN_ITEM));
    println!(
        "TASK3575 operation=burn reply=valid accepted=true item_count_before={burn_before} item_count_after={burn_after}"
    );

    for bad in burn_bad_replies() {
        let mut items = ReplyGatedServiceItems::with_messages([BURN_ITEM]);
        let before = items.message_count();
        let refused = refusal_name(
            items
                .burn(burn_request, raw_reply(bad))
                .expect_err("each bad burn reply must be refused"),
        );
        let after = items.message_count();
        assert_eq!(refused, bad.name);
        assert_eq!((before, after), (1, 1));
        assert!(items.has_message(BURN_ITEM));
        println!(
            "TASK3575 operation=burn reply={} refusal_name={} item_count_before={before} item_count_after={after}",
            bad.name, refused
        );
    }

    let download_request = ServiceOperationRequest {
        request_id: DOWNLOAD_REQUEST,
        item_id: DOWNLOAD_ITEM,
    };
    let mut valid_download_items = ReplyGatedServiceItems::default();
    let download_before = valid_download_items.completed_file_count();
    let valid_download = valid_download_items
        .offer_completed_download(download_request, raw_reply(valid_download_reply()))
        .expect("the direct download call accepts its exact valid reply");
    let download_after = valid_download_items.completed_file_count();
    assert_eq!(valid_download.operation, "download");
    assert_eq!(valid_download.request_id, DOWNLOAD_REQUEST);
    assert_eq!(valid_download.item_id, DOWNLOAD_ITEM);
    assert_eq!((download_before, download_after), (0, 1));
    assert!(valid_download_items.offers_completed_file(DOWNLOAD_ITEM));
    println!(
        "TASK3575 operation=download reply=valid accepted=true item_count_before={download_before} item_count_after={download_after}"
    );

    for bad in download_bad_replies() {
        let mut items = ReplyGatedServiceItems::default();
        let before = items.completed_file_count();
        let refused = refusal_name(
            items
                .offer_completed_download(download_request, raw_reply(bad))
                .expect_err("each bad download reply must be refused"),
        );
        let after = items.completed_file_count();
        assert_eq!(refused, bad.name);
        assert_eq!((before, after), (0, 0));
        assert!(!items.offers_completed_file(DOWNLOAD_ITEM));
        println!(
            "TASK3575 operation=download reply={} refusal_name={} item_count_before={before} item_count_after={after}",
            bad.name, refused
        );
    }
}
