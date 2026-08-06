#![cfg(feature = "core")]

use osl_privacy_hub::scrub_hosted::place_scope::{
    place_hosted_sent_item, prepare_hosted_place_draft, read_hosted_place, scrub_hosted_item,
    HostedPlaceActionError, HostedPlaceActionLog, HostedPlaceActionRequest, HostedPlaceList,
};

const ALLOWED_PLACE: &str = "LANTERN-0124";
const UNLISTED_PLACE: &str = "LANTERN-0124-UNLISTED";
const ITEM: &str = "fixture-visible-item-0124";

#[test]
fn task_0124_osl_is_silent_outside_the_allowed_place_list() {
    let places = HostedPlaceList::new([ALLOWED_PLACE.to_owned()]);
    let allowed_request = HostedPlaceActionRequest::new(ALLOWED_PLACE, ITEM);
    let unlisted_request = allowed_request.with_stable_place_id(UNLISTED_PLACE);
    let mut log = HostedPlaceActionLog::default();

    assert_eq!(allowed_request.item_id, unlisted_request.item_id);
    assert_ne!(
        allowed_request.stable_place_id,
        unlisted_request.stable_place_id
    );

    let before_count = log.action_count();
    let readable = read_hosted_place(&places, &mut log, &allowed_request).expect("read allowed");
    prepare_hosted_place_draft(&places, &mut log, &allowed_request).expect("draft allowed");
    place_hosted_sent_item(&places, &mut log, &allowed_request).expect("place allowed");
    scrub_hosted_item(&places, &mut log, &allowed_request).expect("Scrub allowed");

    let allowed_names = log.action_names().join(",");
    let allowed_count = log.action_count();
    let allowed_fingerprint = log.record_fingerprint();

    let refused = [
        (
            "read",
            read_hosted_place(&places, &mut log, &unlisted_request).map(|_| ()),
        ),
        (
            "draft",
            prepare_hosted_place_draft(&places, &mut log, &unlisted_request).map(|_| ()),
        ),
        (
            "place",
            place_hosted_sent_item(&places, &mut log, &unlisted_request).map(|_| ()),
        ),
        (
            "Scrub",
            scrub_hosted_item(&places, &mut log, &unlisted_request).map(|_| ()),
        ),
    ];

    for (call, result) in &refused {
        assert_eq!(*result, Err(HostedPlaceActionError::PlaceNotAllowed));
        println!(
            "TASK0124_UNLISTED_CALL call={call} refused={}",
            result.as_ref().unwrap_err()
        );
    }

    let after_unlisted_count = log.action_count();
    let after_unlisted_fingerprint = log.record_fingerprint();
    let allowed_records = log
        .records()
        .iter()
        .map(|record| {
            format!(
                "{}:{}:{}",
                record.stable_place_id,
                record.item_id,
                record.kind.as_str()
            )
        })
        .collect::<Vec<_>>()
        .join("|");

    println!("TASK0124_ALLOWED_PLACE={ALLOWED_PLACE}");
    println!("TASK0124_ALLOWED_PLACE_READABLE={}", readable.readable);
    println!("TASK0124_ACTION_COUNT_BEFORE={before_count}");
    println!("TASK0124_ALLOWED_ACTION_NAMES={allowed_names}");
    println!("TASK0124_ACTION_COUNT_AFTER_ALLOWED={allowed_count}");
    println!("TASK0124_UNLISTED_PLACE={UNLISTED_PLACE}");
    println!("TASK0124_STABLE_PLACE_ID_ONLY_FIELD_CHANGED=true");
    println!("TASK0124_ACTION_COUNT_AFTER_UNLISTED={after_unlisted_count}");
    println!("TASK0124_ALLOWED_RECORD_FINGERPRINT_BEFORE_UNLISTED={allowed_fingerprint}");
    println!("TASK0124_ALLOWED_RECORD_FINGERPRINT_AFTER_UNLISTED={after_unlisted_fingerprint}");
    println!("TASK0124_ALLOWED_RECORDS={allowed_records}");

    assert_eq!(readable.stable_place_id, ALLOWED_PLACE);
    assert!(readable.readable);
    assert_eq!(before_count, 0);
    assert_eq!(allowed_names, "read,draft,sent item,Scrub item");
    assert_eq!(allowed_count, 4);
    assert_eq!(after_unlisted_count, 4);
    assert_eq!(after_unlisted_fingerprint, allowed_fingerprint);
    assert!(log
        .records()
        .iter()
        .all(|record| record.stable_place_id == ALLOWED_PLACE));
}
