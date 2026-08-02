//! T12-V1: each independently ambiguous receipt must stay Unknown.

use osl_privacy_hub::scrub_imap::{AmbiguityCounts, DeleteVerification, VerificationAmbiguity};

#[test]
fn scr_v1_each_ambiguity_is_unknown_and_counted_separately() {
    let sources = [
        VerificationAmbiguity::DroppedConnection,
        VerificationAmbiguity::AuthEpochChanged,
        VerificationAmbiguity::SchemaDrift,
        VerificationAmbiguity::RateLimited,
        VerificationAmbiguity::AmbiguousReadback,
        VerificationAmbiguity::PriorUnknown,
    ];
    let mut counts = AmbiguityCounts::default();

    for source in sources {
        assert_eq!(
            counts.record(source),
            DeleteVerification::Unknown,
            "{source:?} must never be promoted to VerifiedGone"
        );
        assert_eq!(counts.count(source), 1, "{source:?} is separately accounted");
    }
    assert_eq!(counts.count(VerificationAmbiguity::DroppedConnection), 1);
    assert_eq!(counts.count(VerificationAmbiguity::PriorUnknown), 1);
}
