//! F3.6: attachment-send tier-gate enforcement.
//!
//! Exercises the `enforce_attachment_tier_gate` call planted at
//! the top of [`cmd_osl_seal_attachment_with_cover_v3`]:
//!
//! - Paid / PaidOfflineGrace users: gate doesn't fire. The seal
//!   itself fails downstream with "identity not loaded" because
//!   the test doesn't seed identity/peer_map — we don't care,
//!   only that the failure is NOT the tier-gate prefix.
//! - Free / Unconfigured / EXPIRED users: gate fires with the
//!   `OSL-TIER-BLOCKED:{json}` wire string. The JSON tail parses
//!   to `TierGateError::PaidFeatureRequired` with
//!   `feature = "encrypted attachments"` and the cache's raw
//!   license status surfaced for diagnostics.
//!
//! Receive-side is intentionally NOT tested here for gating: per
//! the F3.6 spec, free users always decrypt and view attachments
//! sent by paid users. Receive-side decrypt tests live in their
//! existing files (phase8_attachment_integration.rs etc.) and
//! exercise the open path without any tier seed; those tests
//! continue to pass without modification — confirmation of the
//! "receive is tier-free" property.

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use ipc::commands::{
    cmd_osl_seal_attachment_with_cover_v2, cmd_osl_seal_attachment_with_cover_v3,
    OSL_TIER_BLOCKED_PREFIX,
};
use ipc::scope::{ScopeInput, ScopeKind};
use ipc::AppState;
use keystore::{LicenseState, LicenseStateDto};

const TIER_BLOCKED_PREFIX: &str = "OSL-TIER-BLOCKED:";

/// A valid ScopeInput so the function's scope-parse step doesn't
/// short-circuit before the gate fires. (The gate runs FIRST —
/// but a sane scope ensures any non-tier failure path is still
/// distinguishable.)
fn dm_scope() -> ScopeInput {
    ScopeInput {
        kind: ScopeKind::Dm,
        id: "1234567890123456".to_string(),
        server_id: None,
        channel_id: Some("1234567890123456".to_string()),
    }
}

fn install_license(state: &AppState, kind: LicenseState, raw_status: &str) {
    *state
        .license_state
        .lock()
        .expect("license_state mutex poisoned") = LicenseStateDto {
        state: kind,
        raw_status: raw_status.to_string(),
        current_period_end: None,
        last_validated_at: None,
    };
}

/// Call the v3 seal with minimal valid inputs. The test seeds
/// only license_state; identity / peer_map / channel_members are
/// all empty / None. Downstream-of-gate failures (identity not
/// loaded, missing peer pubkeys) produce non-tier errors that the
/// helpers below assert against.
fn call_seal_v3(state: &AppState) -> Result<ipc::commands::SealedAttachmentV2, String> {
    // 1×1 PNG (8-byte signature is enough for mime_for_filename;
    // the actual cipher work happens later, after the gate).
    let png_bytes = vec![0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];
    let original_b64 = STANDARD.encode(&png_bytes);
    cmd_osl_seal_attachment_with_cover_v3(
        state,
        dm_scope(),
        vec!["1234567890123456".to_string()],
        "1234567890123456".to_string(),
        original_b64,
        "photo.png".to_string(),
        "random.mp4".to_string(),
    )
}

/// F3.6-DEFENSE: same as `call_seal_v3` but hits the legacy v2
/// entry. The v2 gate was added in F3.6-DEFENSE as a
/// defense-in-depth measure (v2 is reachable via documented
/// boot.js fallbacks).
fn call_seal_v2(state: &AppState) -> Result<ipc::commands::SealedAttachmentV2, String> {
    let png_bytes = vec![0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];
    let original_b64 = STANDARD.encode(&png_bytes);
    cmd_osl_seal_attachment_with_cover_v2(
        state,
        dm_scope(),
        vec!["1234567890123456".to_string()],
        "1234567890123456".to_string(),
        original_b64,
        "photo.png".to_string(),
        "random.png".to_string(),
    )
}

fn assert_gate_did_not_block<T: std::fmt::Debug>(r: &Result<T, String>) {
    match r {
        Ok(_) => {} // would only happen with full identity seeded
        Err(e) => assert!(
            !e.starts_with(TIER_BLOCKED_PREFIX),
            "gate should not have blocked but got: {e}"
        ),
    }
}

fn assert_gate_blocked_and_parse<T: std::fmt::Debug>(r: Result<T, String>) -> serde_json::Value {
    let err = match r {
        Err(e) => e,
        Ok(v) => panic!("expected tier-gate block, got Ok({v:?})"),
    };
    assert!(
        err.starts_with(TIER_BLOCKED_PREFIX),
        "expected `{TIER_BLOCKED_PREFIX}` prefix, got: {err}"
    );
    let tail = &err[TIER_BLOCKED_PREFIX.len()..];
    serde_json::from_str(tail).unwrap_or_else(|e| panic!("JSON tail did not parse: {e}; raw={err}"))
}

// ---- Sanity ----

#[test]
fn prefix_constant_matches_wire_string() {
    assert_eq!(OSL_TIER_BLOCKED_PREFIX, TIER_BLOCKED_PREFIX);
}

// ---- Paid + Grace pass ----

#[test]
fn paid_user_attachment_send_not_blocked() {
    let state = AppState::new();
    install_license(&state, LicenseState::Paid, "ACTIVE");
    let r = call_seal_v3(&state);
    assert_gate_did_not_block(&r);
}

#[test]
fn paid_offline_grace_attachment_send_not_blocked() {
    let state = AppState::new();
    install_license(&state, LicenseState::PaidOfflineGrace, "ACTIVE");
    let r = call_seal_v3(&state);
    assert_gate_did_not_block(&r);
}

// ---- Free / Unconfigured / EXPIRED block ----

#[test]
fn free_user_attachment_send_blocked_with_typed_err() {
    let state = AppState::new();
    install_license(&state, LicenseState::Free, "Unconfigured");
    let r = call_seal_v3(&state);
    let parsed = assert_gate_blocked_and_parse(r);
    assert_eq!(
        parsed.get("kind").and_then(|v| v.as_str()),
        Some("paid_feature_required")
    );
    assert_eq!(
        parsed.get("feature").and_then(|v| v.as_str()),
        Some("encrypted attachments")
    );
    assert_eq!(
        parsed.get("raw_license_state").and_then(|v| v.as_str()),
        Some("Unconfigured")
    );
}

#[test]
fn unconfigured_user_default_appstate_attachment_send_blocked() {
    // Default AppState has LicenseStateDto::unconfigured() — Free
    // with raw_status "Unconfigured". A fresh install with no
    // license has the gate fire on the very first attachment send.
    let state = AppState::new();
    let r = call_seal_v3(&state);
    let parsed = assert_gate_blocked_and_parse(r);
    assert_eq!(
        parsed.get("kind").and_then(|v| v.as_str()),
        Some("paid_feature_required")
    );
}

#[test]
fn expired_license_attachment_send_blocked() {
    let state = AppState::new();
    install_license(&state, LicenseState::Free, "EXPIRED");
    let r = call_seal_v3(&state);
    let parsed = assert_gate_blocked_and_parse(r);
    assert_eq!(
        parsed.get("kind").and_then(|v| v.as_str()),
        Some("paid_feature_required")
    );
    assert_eq!(
        parsed.get("raw_license_state").and_then(|v| v.as_str()),
        Some("EXPIRED"),
        "raw_license_state surfaces the cache status for diagnostics: {parsed:?}"
    );
}

// ============================================================
// F3.6-DEFENSE: the legacy v2 seal path is gated identically.
// Mirrors the v3 matrix. If any of these regress, a free user
// could bypass the attachment paywall via the legacy command.
// ============================================================

#[test]
fn v2_paid_user_attachment_send_not_blocked() {
    let state = AppState::new();
    install_license(&state, LicenseState::Paid, "ACTIVE");
    let r = call_seal_v2(&state);
    assert_gate_did_not_block(&r);
}

#[test]
fn v2_paid_offline_grace_attachment_send_not_blocked() {
    let state = AppState::new();
    install_license(&state, LicenseState::PaidOfflineGrace, "ACTIVE");
    let r = call_seal_v2(&state);
    assert_gate_did_not_block(&r);
}

#[test]
fn v2_free_user_attachment_send_blocked_with_typed_err() {
    let state = AppState::new();
    install_license(&state, LicenseState::Free, "Unconfigured");
    let r = call_seal_v2(&state);
    let parsed = assert_gate_blocked_and_parse(r);
    assert_eq!(
        parsed.get("kind").and_then(|v| v.as_str()),
        Some("paid_feature_required")
    );
    assert_eq!(
        parsed.get("feature").and_then(|v| v.as_str()),
        Some("encrypted attachments")
    );
    assert_eq!(
        parsed.get("raw_license_state").and_then(|v| v.as_str()),
        Some("Unconfigured")
    );
}

#[test]
fn v2_unconfigured_user_default_appstate_attachment_send_blocked() {
    // Default AppState (no license) — the legacy v2 path must
    // block just like v3.
    let state = AppState::new();
    let r = call_seal_v2(&state);
    let parsed = assert_gate_blocked_and_parse(r);
    assert_eq!(
        parsed.get("kind").and_then(|v| v.as_str()),
        Some("paid_feature_required")
    );
}
