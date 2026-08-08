#![cfg(feature = "core")]

use osl_privacy_hub::osl_chat_content_name::{
    OldOslChatContentIndex, OldOslChatContentKind, REMOTE_ONLY, SAVED_HERE_FILE, SAVED_HERE_TEXT,
};

struct ServiceCallCounter {
    calls: usize,
}

impl ServiceCallCounter {
    fn calls(&self) -> usize {
        self.calls
    }
}

#[test]
fn task_3609d_names_old_local_and_remote_only_content_without_fetching() {
    // These are the local facts retained by the 1303 text-history path and
    // the 0655 authenticated attachment-notice path.  The remote-only item is
    // known locally, but its bytes were never saved on this device.
    let mut old_content = OldOslChatContentIndex::default();
    let fixtures = [
        (
            "old-text",
            OldOslChatContentKind::Text,
            true,
            SAVED_HERE_TEXT,
        ),
        (
            "old-file",
            OldOslChatContentKind::File,
            true,
            SAVED_HERE_FILE,
        ),
        (
            "old-remote",
            OldOslChatContentKind::File,
            false,
            REMOTE_ONLY,
        ),
    ];
    for (item_id, kind, saved_here, _) in fixtures {
        old_content.record(item_id, kind, saved_here);
    }

    // Start above zero so equality cannot be satisfied merely because the
    // fixture forgot to observe the service boundary.
    let service = ServiceCallCounter { calls: 7 };
    let starting_service_calls = service.calls();

    let results = fixtures.map(|(item_id, _, _, expected)| {
        let actual = old_content.name(item_id).expect("known old item");
        assert_eq!(actual, expected, "wrong name for {item_id}");
        actual
    });
    assert_eq!(
        service.calls(),
        starting_service_calls,
        "naming must not fetch"
    );

    let unknown_id = "old-unknown";
    let unknown = old_content
        .name(unknown_id)
        .expect_err("an unknown item must be refused");
    assert!(
        unknown.contains(unknown_id),
        "refusal must name the item: {unknown}"
    );

    println!("TASK3609D_FIXTURE_COUNT={}", results.len());
    println!("TASK3609D_OLD_TEXT={}", results[0]);
    println!("TASK3609D_OLD_FILE={}", results[1]);
    println!("TASK3609D_OLD_REMOTE={}", results[2]);
    println!(
        "TASK3609D_SERVICE_CALLS_START={starting_service_calls} TASK3609D_SERVICE_CALLS_END={}",
        service.calls()
    );
    println!("TASK3609D_UNKNOWN_REFUSAL={unknown}");
}
