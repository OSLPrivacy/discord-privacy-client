//! T3-T19 / ABI §5.2: the visual fallback is display-only evidence.

use osl_privacy_hub::adapters::{is_send_evidence_admissible, A11yTree, BindingEvidence};

#[test]
fn t3_t19_pixel_evidence_can_never_authorize_a_send() {
    let cases = [
        (
            "uia accessibility",
            BindingEvidence::Accessibility {
                tree: A11yTree::Uia,
            },
            true,
        ),
        (
            "msaa accessibility",
            BindingEvidence::Accessibility {
                tree: A11yTree::Msaa,
            },
            true,
        ),
        (
            "combined accessibility",
            BindingEvidence::Accessibility {
                tree: A11yTree::Both,
            },
            true,
        ),
        ("win32 structural", BindingEvidence::Win32Structural, true),
        (
            "user-confirmed visual binding",
            BindingEvidence::UserConfirmedVisualBinding {
                ceremony_id: "ceremony-opaque-id".into(),
                confirmed_at_ms: 1,
            },
            true,
        ),
        ("pixel", BindingEvidence::Pixel, false),
    ];

    for (name, evidence, admissible) in cases {
        assert_eq!(
            is_send_evidence_admissible(&evidence),
            admissible,
            "{name} tier must retain its ABI §5.2 send admissibility"
        );
    }
}
