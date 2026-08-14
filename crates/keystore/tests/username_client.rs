use keystore::client::{is_normalized_username, username_claim_msg};

#[test]
fn username_validation_never_silently_normalizes() {
    for valid in ["a", "Alice", "alice_01", "_alice", "alice_"] {
        assert!(is_normalized_username(valid), "rejected {valid:?}");
    }
    for invalid in ["alice-name", "alice.name", "alice name"] {
        assert!(!is_normalized_username(invalid), "accepted {invalid:?}");
    }
    assert!(!is_normalized_username(&"a".repeat(17)));
}
#[test]
fn username_claim_vector_matches_worker() {
    assert_eq!(
        username_claim_msg(
            "alice_01",
            "user-7",
            "OSLFR1.invite",
            &"A".repeat(43),
            1_700_000_000_123,
        ),
        b"OSL-USERNAME-CLAIM-v1\nalice_01\nuser-7\nOSLFR1.invite\nAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA\n1700000000123"
    );
}
