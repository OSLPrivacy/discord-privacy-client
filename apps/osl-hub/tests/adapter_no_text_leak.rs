//! T3-T20 / ABI C10: adapter receipts are commitments, never provider text.

use osl_privacy_hub::adapters::{
    AdapterRefusal, Bounds, PaintConfidence, PaintTarget, PlacementReceipt, PlacementStatus,
    SendOutcome, SendReceipt,
};

fn assert_marker_absent(marker: &str, value: impl std::fmt::Debug) {
    assert!(
        !format!("{value:?}").contains(marker),
        "provider-supplied text escaped an adapter result"
    );
}

#[test]
fn t3_t20_provider_text_never_escapes_a_receipt_refusal_or_digest() {
    // These distinctive values model provider names, conversation titles,
    // participants and body text. The ABI lets adapter results contain only
    // enum states, fixed-width commitments, and geometry.
    for marker in [
        "provider-title::orion-7",
        "provider-recipient::solstice-9",
        "provider-body::cobalt-13",
    ] {
        for refusal in [
            AdapterRefusal::ProfileNotUsable,
            AdapterRefusal::ProfileExpired,
            AdapterRefusal::CanaryMismatch,
            AdapterRefusal::WindowGone,
            AdapterRefusal::GenerationStale,
            AdapterRefusal::NotFocused,
            AdapterRefusal::Occluded,
            AdapterRefusal::ComposerNotFound,
            AdapterRefusal::ComposerAmbiguous,
            AdapterRefusal::PasswordField,
            AdapterRefusal::TranscriptNotFound,
            AdapterRefusal::ReadIncomplete,
            AdapterRefusal::DestinationUnattested,
            AdapterRefusal::DestinationChanged,
            AdapterRefusal::AuthorizationRejected,
            AdapterRefusal::CapabilityNotGranted,
            AdapterRefusal::AccessibilityUnavailable,
            AdapterRefusal::PlatformUnsupported,
            AdapterRefusal::Timeout,
        ] {
            assert_marker_absent(marker, refusal);
        }

        for receipt in [
            PlacementReceipt {
                status: PlacementStatus::Placed,
                placed_sha256: Some("f".repeat(64)),
                elapsed_ms: 1,
            },
            PlacementReceipt {
                status: PlacementStatus::NotPlaced,
                placed_sha256: None,
                elapsed_ms: 0,
            },
        ] {
            assert_marker_absent(marker, receipt);
        }
        for receipt in [
            SendReceipt {
                outcome: SendOutcome::Sent,
                elapsed_ms: 1,
            },
            SendReceipt {
                outcome: SendOutcome::NotSent,
                elapsed_ms: 0,
            },
            SendReceipt {
                outcome: SendOutcome::Unknown,
                elapsed_ms: 1,
            },
        ] {
            assert_marker_absent(marker, receipt);
        }
        assert_marker_absent(
            marker,
            PaintTarget {
                carrier_sha256: "a".repeat(64),
                rect: Bounds {
                    x: 1,
                    y: 2,
                    width: 3,
                    height: 4,
                },
                clipped_by: None,
                confidence: PaintConfidence::Exact,
            },
        );
    }
}
