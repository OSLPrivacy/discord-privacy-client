//! F3.6: paid-feature gate primitives.
//!
//! **Model (post-F3.6 pivot)**: free tier has unlimited TEXT
//! encryption forever (no timer, no ads). Paid tier unlocks
//! ENCRYPTED ATTACHMENT SENDING and beta channel access. The
//! prior 60-min launch-window + ad-unlock model from F3.1 has
//! been retired; this module is the surviving sliver.
//!
//! What carried over from F3.1 / F3.2:
//!
//! - [`is_paid_equivalent`] — the one-line truth that gates every
//!   paid feature. `Paid` and `PaidOfflineGrace` both qualify;
//!   PaidOfflineGrace is included because F2.4 already classifies
//!   it as "you paid; the keyserver just can't reach you," and
//!   gating it would regress on the 7-day offline-grace contract.
//! - [`TierGateError`] — the typed error surface. The enum's
//!   single variant is now `PaidFeatureRequired { feature, raw_license_state }`,
//!   reflecting the new "paid-feature gate" model. The
//!   `OSL-TIER-BLOCKED:{json}` wire-string contract (`commands.rs`)
//!   and the JS-side modal infrastructure (boot.js) both consume
//!   the new shape unchanged in structure — only the field set
//!   differs from the F3.1 `FreeWindowExpired`.
//!
//! What got deleted in the pivot (kept here as breadcrumbs):
//!
//! - `check_send_allowed` — the text-encrypt gate. Replaced by
//!   "text encryption is always allowed for everyone."
//! - `free_window_end`, `free_tier_window_active`,
//!   `record_ad_unlock`, `clear_ad_unlock`, `set_launch_time_once`
//!   — all the launch-window + ad-unlock primitives.
//! - `FREE_WINDOW_SECONDS` constant (was 60min).
//! - `AppState::launch_time` and `AppState::free_tier_unlocked_until`
//!   fields.
//! - The bootstrap-time `set_launch_time_once` stamp.
//! - The `cmd_osl_record_ad_unlock` IPC (and its 5 capability
//!   artifacts) — F3.4's planned keyserver redeem flow is cut.
//! - The "nag toast" / "ad surface" plumbing — F3.5 is cut.
//!
//! What replaces the cut text-encrypt gate:
//!
//! - [`check_attachment_allowed`] — F3.6's new gate. Called from
//!   `cmd_osl_seal_attachment_with_cover_v3` (the v3 attachment
//!   seal path that boot.js's step-2 upload pipeline invokes).
//!   Receive-side (open_attachment*) is intentionally NOT gated:
//!   free users can always decrypt and view attachments sent by
//!   paid users.

use crate::AppState;
use keystore::LicenseState;
use serde::Serialize;

/// Typed error surface for paid-feature gates. Carries the
/// context the JS-side modal renders (which feature, current
/// license state). `serde::Serialize` so the IPC layer can ship
/// it as JSON on the error tail
/// (`Err(format!("OSL-TIER-BLOCKED:{}", serde_json::to_string(&e)?))`).
/// Same shape contract as F3.1; only the variant set has changed.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TierGateError {
    /// The user is Free (or Unconfigured) and tried to invoke a
    /// paid-only feature. F3.6 surfaces the "Upgrade / Send
    /// without attachment / Cancel" modal off this variant.
    PaidFeatureRequired {
        /// Human-readable feature label, e.g. `"encrypted attachments"`.
        /// Boot.js renders this in the modal body so future paid
        /// features can reuse the same variant + modal with a
        /// different label.
        feature: String,
        /// Cached license-state raw_status for diagnostic context
        /// (e.g. "Free", "Unconfigured", "EXPIRED"). Surfaces in
        /// the JS DevTools console; not in the modal body.
        raw_license_state: String,
    },
}

impl std::fmt::Display for TierGateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PaidFeatureRequired {
                feature,
                raw_license_state,
            } => write!(
                f,
                "paid feature required: {feature} \
                 (raw_license_state={raw_license_state})"
            ),
        }
    }
}

impl std::error::Error for TierGateError {}

/// `true` iff the cached license state grants paid-equivalent
/// behaviour. `PaidOfflineGrace` is included because F2.4 already
/// classifies that as "you paid; keyserver just can't reach you" —
/// gating it would be a regression on the 7-day grace contract.
///
/// This is the load-bearing primitive: every paid-feature check
/// in the codebase ultimately ANDs against this. Sub-microsecond
/// (one mutex lock + enum match).
pub fn is_paid_equivalent(state: &AppState) -> bool {
    matches!(
        state
            .license_state
            .lock()
            .expect("license_state mutex poisoned")
            .state,
        LicenseState::Paid | LicenseState::PaidOfflineGrace,
    )
}

/// F3.6 attachment-send gate. Called at the entry of
/// `cmd_osl_seal_attachment_with_cover_v3` (and any future
/// attachment-seal entry point added to that pipeline). Paid /
/// PaidOfflineGrace users return `Ok(())`; Free / Unconfigured /
/// EXPIRED / REVOKED users return
/// `Err(TierGateError::PaidFeatureRequired { feature: "encrypted attachments", ... })`.
///
/// Receive-side paths (open_attachment, open_attachment_v2,
/// open_attachment_v3_split) are intentionally NOT gated. Free
/// users always decrypt and view attachments sent by paid users —
/// only SENDING is gated. Decryption is a privacy feature, not a
/// paid feature.
pub fn check_attachment_allowed(state: &AppState) -> Result<(), TierGateError> {
    if is_paid_equivalent(state) {
        return Ok(());
    }
    let raw_license_state = state
        .license_state
        .lock()
        .expect("license_state mutex poisoned")
        .raw_status
        .clone();
    Err(TierGateError::PaidFeatureRequired {
        feature: "encrypted attachments".to_string(),
        raw_license_state,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use keystore::{LicenseState, LicenseStateDto};

    fn paid_state() -> LicenseStateDto {
        LicenseStateDto {
            state: LicenseState::Paid,
            raw_status: "ACTIVE".to_string(),
            current_period_end: Some(9_999_999_999),
            last_validated_at: Some(0),
        }
    }

    fn grace_state() -> LicenseStateDto {
        LicenseStateDto {
            state: LicenseState::PaidOfflineGrace,
            raw_status: "ACTIVE".to_string(),
            current_period_end: Some(9_999_999_999),
            last_validated_at: Some(0),
        }
    }

    fn free_state() -> LicenseStateDto {
        LicenseStateDto {
            state: LicenseState::Free,
            raw_status: "Unconfigured".to_string(),
            current_period_end: None,
            last_validated_at: None,
        }
    }

    fn expired_state() -> LicenseStateDto {
        LicenseStateDto {
            state: LicenseState::Free,
            raw_status: "EXPIRED".to_string(),
            current_period_end: Some(0),
            last_validated_at: Some(0),
        }
    }

    fn install(state: &AppState, dto: LicenseStateDto) {
        *state
            .license_state
            .lock()
            .expect("license_state mutex poisoned") = dto;
    }

    // ---- is_paid_equivalent ----

    #[test]
    fn is_paid_equivalent_recognizes_paid() {
        let state = AppState::new();
        install(&state, paid_state());
        assert!(is_paid_equivalent(&state));
    }

    #[test]
    fn is_paid_equivalent_recognizes_grace() {
        let state = AppState::new();
        install(&state, grace_state());
        assert!(is_paid_equivalent(&state));
    }

    #[test]
    fn is_paid_equivalent_rejects_free() {
        let state = AppState::new();
        install(&state, free_state());
        assert!(!is_paid_equivalent(&state));
    }

    #[test]
    fn is_paid_equivalent_rejects_expired() {
        let state = AppState::new();
        install(&state, expired_state());
        assert!(!is_paid_equivalent(&state));
    }

    // ---- check_attachment_allowed ----

    #[test]
    fn attachment_allowed_for_paid() {
        let state = AppState::new();
        install(&state, paid_state());
        assert!(check_attachment_allowed(&state).is_ok());
    }

    #[test]
    fn attachment_allowed_for_grace() {
        let state = AppState::new();
        install(&state, grace_state());
        assert!(check_attachment_allowed(&state).is_ok());
    }

    #[test]
    fn attachment_blocked_for_free_with_typed_err() {
        let state = AppState::new();
        install(&state, free_state());
        let err = check_attachment_allowed(&state).expect_err("free user should be blocked");
        match err {
            TierGateError::PaidFeatureRequired {
                feature,
                raw_license_state,
            } => {
                assert_eq!(feature, "encrypted attachments");
                assert_eq!(raw_license_state, "Unconfigured");
            }
        }
    }

    #[test]
    fn attachment_blocked_for_expired() {
        let state = AppState::new();
        install(&state, expired_state());
        let err = check_attachment_allowed(&state).expect_err("expired user should be blocked");
        match err {
            TierGateError::PaidFeatureRequired {
                feature,
                raw_license_state,
            } => {
                assert_eq!(feature, "encrypted attachments");
                assert_eq!(raw_license_state, "EXPIRED");
            }
        }
    }

    // ---- TierGateError serde shape ----

    #[test]
    fn paid_feature_required_serializes_with_kind_discriminator() {
        let e = TierGateError::PaidFeatureRequired {
            feature: "encrypted attachments".to_string(),
            raw_license_state: "Unconfigured".to_string(),
        };
        let json = serde_json::to_string(&e).expect("serialize");
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("parse");
        assert_eq!(parsed["kind"], "paid_feature_required");
        assert_eq!(parsed["feature"], "encrypted attachments");
        assert_eq!(parsed["raw_license_state"], "Unconfigured");
    }
}
