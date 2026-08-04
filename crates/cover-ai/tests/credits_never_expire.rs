//! T13-F5 regression guard for the v1 credits decision.
//!
//! v1 has no cloud-generation offering and therefore no credits balance that
//! could silently acquire an expiry.  If a future offering is added, this test
//! must be replaced by a ledger-level test that advances time and preserves a
//! purchased balance; it must not be weakened into an expiry policy.

const ARCHITECTURE: &str = include_str!("../../../docs/design/ai-carrier-architecture.md");

#[test]
fn t13_tf5_v1_has_no_credit_balance_to_expire_or_renew() {
    assert!(
        ARCHITECTURE.contains("\"cloudGenerationOffered\": false"),
        "v1 must not create a cloud-generation balance before that offering exists"
    );
    assert!(
        ARCHITECTURE.contains("\"creditPurchaseOffered\": false"),
        "v1 must not sell a balance with an undisclosed expiry or renewal rule"
    );
    assert!(
        ARCHITECTURE
            .contains("Credits and Pro time are separate balances and must never be conflated."),
        "a future credit balance must never inherit Pro-time semantics"
    );
}
