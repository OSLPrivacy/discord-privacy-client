use std::collections::HashSet;

use osl_privacy_hub::sync_policy::{
    allowed_sync_kinds, check_sync_payload_before_send, refused_sync_kinds, SyncPayloadCheckError,
};

#[test]
fn task_4811_sync_policy_lists_exact_refusals_and_refuses_before_send() {
    let refused = refused_sync_kinds();
    let allowed = allowed_sync_kinds();

    println!("TASK4811_REFUSED_COUNT={}", refused.len());
    println!("TASK4811_ALLOWED_COUNT={}", allowed.len());

    assert_eq!(
        refused.len(),
        6,
        "there must be exactly six refused sync kinds"
    );
    assert!(
        allowed.len() >= 8,
        "allowed side must name at least eight sync kinds"
    );

    let mut refused_seen = HashSet::new();
    for item in refused {
        println!("TASK4811_REFUSED kind={} reason={}", item.kind, item.reason);
        assert!(
            !item.kind.trim().is_empty(),
            "refused kind must not be blank"
        );
        assert!(
            !item.reason.trim().is_empty(),
            "refused reason must not be blank"
        );
        assert!(
            refused_seen.insert(item.kind),
            "refused kind must be unique: {}",
            item.kind
        );
        assert_eq!(
            item.reason.matches('.').count(),
            1,
            "each refused kind must have exactly one written reason: {}",
            item.kind
        );
    }

    let mut allowed_seen = HashSet::new();
    for kind in allowed {
        println!("TASK4811_ALLOWED kind={kind}");
        assert!(!kind.trim().is_empty(), "allowed kind must not be blank");
        assert!(
            allowed_seen.insert(*kind),
            "allowed kind must be unique: {kind}"
        );
        assert!(
            !refused_seen.contains(kind),
            "kind appears on both allowed and refused sides: {kind}"
        );
    }

    let mut refusal_attempts = 0;
    for item in refused {
        match check_sync_payload_before_send([item.kind]) {
            Err(SyncPayloadCheckError::Refused { kind, reason }) => {
                refusal_attempts += 1;
                println!("TASK4811_REFUSAL_ATTEMPT kind={kind} sent=0 reason={reason}");
                assert_eq!(kind, item.kind);
                assert_eq!(reason, item.reason);
            }
            other => panic!("refused kind did not stop before send: {other:?}"),
        }
    }
    println!("TASK4811_REFUSAL_ATTEMPTS={refusal_attempts}");
    assert_eq!(refusal_attempts, 6);

    check_sync_payload_before_send(allowed.iter().copied())
        .expect("known allowed sync kinds pass the pre-send check");
    println!(
        "TASK4811_ALLOWED_PAYLOAD sent=1 kinds={}",
        allowed.join("|")
    );

    match check_sync_payload_before_send(["future_unclassified_kind"]) {
        Err(SyncPayloadCheckError::UnknownKind { kind }) => {
            println!("TASK4811_UNKNOWN_KIND kind={kind}");
            assert_eq!(kind, "future_unclassified_kind");
        }
        other => panic!("unknown kind must be named as neither allowed nor refused: {other:?}"),
    }
}
