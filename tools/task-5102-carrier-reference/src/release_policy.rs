//! Immutable release policy for the Task 5103 provisional fidelity envelope.
//!
//! Automated runs may enforce this envelope but may not calibrate it. A later
//! blinded human A/B study is outside the all-agent task set and is accepted
//! only when separately authorized, signed by its human observer, and strictly
//! tightens at least one limit without loosening any other limit.

use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::Deserialize;
use std::fs;
use std::path::Path;

pub const POLICY_SCHEMA: &str = "osl-carrier-release-policy-v1";
pub const CALIBRATION_SCHEMA: &str = "osl-carrier-human-ab-calibration-v1";
pub const EXPECTED_POLICY_ID: &str = "task-5103-provisional-envelope";

// Release roots are public verification keys. Private signing material is not
// present in the runtime or repository.
const AUTHORIZER_PUBLIC_KEY_HEX: &str =
    "ea4a6c63e29c520abef5507b132ec5f9954776aebebe7b92421eea691446d22c";
const OBSERVER_PUBLIC_KEY_HEX: &str =
    "fd1724385aa0c75b64fb78cd602fa1d991fdebf76b13c58ed702eac835e9f618";

#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Limits {
    pub boundary_displacement_px_max: f64,
    pub baseline_displacement_px_max: f64,
    pub flat_fill_delta_e00_median_lt: f64,
    pub flat_fill_delta_e00_p99_lt: f64,
    pub structure_mismatch_pct_max: f64,
    pub tile_size_physical_px: f64,
    pub tile_16x16_mismatch_pct_max: f64,
    pub exact_probe_raw_mismatch_pct_max: f64,
}

pub const TASK_5103_LIMITS: Limits = Limits {
    boundary_displacement_px_max: 0.0,
    baseline_displacement_px_max: 0.0,
    flat_fill_delta_e00_median_lt: 1.0,
    flat_fill_delta_e00_p99_lt: 2.3,
    structure_mismatch_pct_max: 0.5,
    tile_size_physical_px: 16.0,
    tile_16x16_mismatch_pct_max: 5.0,
    exact_probe_raw_mismatch_pct_max: 0.5,
};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReleasePolicy {
    pub schema: String,
    pub policy_id: String,
    pub immutable_for_release: bool,
    pub calibration_authority: String,
    pub enforcement_scope: String,
    pub limits: Limits,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CalibrationRecord {
    pub schema: String,
    pub authorization_id: String,
    pub authorization_scope: String,
    pub authorization_signature_hex: String,
    pub observer_kind: String,
    pub observer_name: String,
    pub observer_record_id: String,
    pub blinded_same_monitor_ab: bool,
    pub signed_observer_record: bool,
    pub observer_signature_hex: String,
    pub tightened_limits: Limits,
}

pub fn check_files(policy_path: &Path, calibration_path: Option<&Path>) -> Result<String, String> {
    let policy_bytes = fs::read(policy_path).map_err(|error| {
        format!(
            "cannot read release policy {}: {error}",
            policy_path.display()
        )
    })?;
    let policy: ReleasePolicy = serde_json::from_slice(&policy_bytes)
        .map_err(|error| format!("release policy malformed: {error}"))?;
    check_policy(&policy)?;

    let mut report = format!(
        "RELEASE POLICY limits=8 boundary=0px baseline=0px flat_median<1.0 flat_p99<2.3 structure<=0.5% tile_size=16px tile16<=5.0% exact_raw<=0.5% scope=per-state-per-channel averaging=none immutable=true calibration=agent-forbidden\n"
    );
    if let Some(path) = calibration_path {
        let bytes = fs::read(path).map_err(|error| {
            format!("cannot read calibration record {}: {error}", path.display())
        })?;
        let record: CalibrationRecord = serde_json::from_slice(&bytes)
            .map_err(|error| format!("calibration record malformed: {error}"))?;
        let tightened = check_calibration(&record)?;
        report.push_str(&format!(
            "HUMAN CALIBRATION accepted authorization={} observer_record={} tightened={} loosened=0\n",
            record.authorization_id, record.observer_record_id, tightened
        ));
    } else {
        report.push_str("PASS policy=task-5103-provisional-envelope calibration=none\n");
    }
    Ok(report)
}

fn check_policy(policy: &ReleasePolicy) -> Result<(), String> {
    if policy.schema != POLICY_SCHEMA {
        return Err(format!("schema must be {POLICY_SCHEMA}"));
    }
    if policy.policy_id != EXPECTED_POLICY_ID {
        return Err(format!("policy_id must be {EXPECTED_POLICY_ID}"));
    }
    if !policy.immutable_for_release {
        return Err("immutable_for_release must be true".into());
    }
    if policy.calibration_authority != "separately_authorized_signed_human_only" {
        return Err("calibration_authority must be separately_authorized_signed_human_only".into());
    }
    if policy.enforcement_scope != "per_state_per_channel_no_averaging" {
        return Err("enforcement_scope must be per_state_per_channel_no_averaging".into());
    }
    for (name, actual, expected) in limit_values(policy.limits, TASK_5103_LIMITS) {
        if !actual.is_finite() || actual != expected {
            return Err(format!(
                "provisional limit {name} must remain exactly {expected}; got {actual}"
            ));
        }
    }
    Ok(())
}

fn check_calibration(record: &CalibrationRecord) -> Result<usize, String> {
    if record.schema != CALIBRATION_SCHEMA {
        return Err(format!("calibration schema must be {CALIBRATION_SCHEMA}"));
    }
    if record.observer_kind != "human" {
        return Err(format!(
            "automated/agent-generated noticeability claim rejected: observer_kind={}",
            record.observer_kind
        ));
    }
    if record.authorization_scope != "tighten_task_5103_after_blinded_human_ab"
        || record.authorization_id.trim().is_empty()
    {
        return Err("separate human A/B calibration authorization is required".into());
    }
    if !record.blinded_same_monitor_ab || !record.signed_observer_record {
        return Err("signed blinded same-monitor human-observer record is required".into());
    }
    if record.observer_name.trim().is_empty() || record.observer_record_id.trim().is_empty() {
        return Err("human observer name and record id are required".into());
    }

    let mut tightened = 0;
    for (name, proposed, provisional) in limit_values(record.tightened_limits, TASK_5103_LIMITS) {
        if !proposed.is_finite() || proposed < 0.0 {
            return Err(format!(
                "human calibration limit {name} is invalid: {proposed}"
            ));
        }
        if name == "tile_size_physical_px" && proposed != provisional {
            return Err(format!(
                "human calibration may not replace provisional localization {name}: {proposed} != {provisional}"
            ));
        }
        if proposed > provisional {
            return Err(format!(
                "human calibration may not loosen provisional limit {name}: {proposed} > {provisional}"
            ));
        }
        tightened += usize::from(proposed < provisional);
    }
    if tightened == 0 {
        return Err("human calibration must tighten at least one provisional limit".into());
    }

    verify_signature(
        AUTHORIZER_PUBLIC_KEY_HEX,
        &record.authorization_signature_hex,
        authorization_payload(record).as_bytes(),
        "separate authorization",
    )?;
    verify_signature(
        OBSERVER_PUBLIC_KEY_HEX,
        &record.observer_signature_hex,
        observer_payload(record).as_bytes(),
        "human observer",
    )?;
    Ok(tightened)
}

fn authorization_payload(record: &CalibrationRecord) -> String {
    format!(
        "{CALIBRATION_SCHEMA}\nauthorization_id={}\nscope={}\nobserver_record_id={}\n",
        record.authorization_id, record.authorization_scope, record.observer_record_id
    )
}

fn observer_payload(record: &CalibrationRecord) -> String {
    let l = record.tightened_limits;
    format!(
        "{CALIBRATION_SCHEMA}\nauthorization_id={}\nobserver_kind={}\nobserver_name={}\nobserver_record_id={}\nblinded_same_monitor_ab=true\nboundary={}\nbaseline={}\nflat_median={}\nflat_p99={}\nstructure={}\ntile_size={}\ntile16={}\nexact_raw={}\n",
        record.authorization_id,
        record.observer_kind,
        record.observer_name,
        record.observer_record_id,
        l.boundary_displacement_px_max,
        l.baseline_displacement_px_max,
        l.flat_fill_delta_e00_median_lt,
        l.flat_fill_delta_e00_p99_lt,
        l.structure_mismatch_pct_max,
        l.tile_size_physical_px,
        l.tile_16x16_mismatch_pct_max,
        l.exact_probe_raw_mismatch_pct_max,
    )
}

fn limit_values(actual: Limits, expected: Limits) -> [(&'static str, f64, f64); 8] {
    [
        (
            "boundary_displacement_px_max",
            actual.boundary_displacement_px_max,
            expected.boundary_displacement_px_max,
        ),
        (
            "baseline_displacement_px_max",
            actual.baseline_displacement_px_max,
            expected.baseline_displacement_px_max,
        ),
        (
            "flat_fill_delta_e00_median_lt",
            actual.flat_fill_delta_e00_median_lt,
            expected.flat_fill_delta_e00_median_lt,
        ),
        (
            "flat_fill_delta_e00_p99_lt",
            actual.flat_fill_delta_e00_p99_lt,
            expected.flat_fill_delta_e00_p99_lt,
        ),
        (
            "structure_mismatch_pct_max",
            actual.structure_mismatch_pct_max,
            expected.structure_mismatch_pct_max,
        ),
        (
            "tile_size_physical_px",
            actual.tile_size_physical_px,
            expected.tile_size_physical_px,
        ),
        (
            "tile_16x16_mismatch_pct_max",
            actual.tile_16x16_mismatch_pct_max,
            expected.tile_16x16_mismatch_pct_max,
        ),
        (
            "exact_probe_raw_mismatch_pct_max",
            actual.exact_probe_raw_mismatch_pct_max,
            expected.exact_probe_raw_mismatch_pct_max,
        ),
    ]
}

fn verify_signature(
    key_hex: &str,
    signature_hex: &str,
    payload: &[u8],
    role: &str,
) -> Result<(), String> {
    let key_bytes = decode_hex::<32>(key_hex).map_err(|error| format!("{role} key: {error}"))?;
    let signature_bytes =
        decode_hex::<64>(signature_hex).map_err(|error| format!("{role} signature: {error}"))?;
    let key = VerifyingKey::from_bytes(&key_bytes)
        .map_err(|error| format!("{role} key invalid: {error}"))?;
    key.verify(payload, &Signature::from_bytes(&signature_bytes))
        .map_err(|_| format!("{role} signature verification failed"))
}

fn decode_hex<const N: usize>(value: &str) -> Result<[u8; N], String> {
    if value.len() != N * 2 {
        return Err(format!("expected {} hex characters", N * 2));
    }
    let mut out = [0; N];
    for (index, slot) in out.iter_mut().enumerate() {
        *slot = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16)
            .map_err(|_| "contains non-hex characters".to_string())?;
    }
    Ok(out)
}
