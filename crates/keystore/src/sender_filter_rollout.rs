//! Validation for the independently administered sender-filter capability
//! floor returned by the production keyserver.
//!
//! The floor is not stored in a caller-selected or locally deletable file.
//! Migration 0032 installs an append-only D1 record, and the Worker derives
//! capability version 1 from its own live migration-0031 schema check before it
//! may create that record. The signed request carries a fresh random request
//! id; the exact production origin echoes it in the response. Deleting local
//! application data or restarting the process therefore cannot recreate a
//! `NeverObserved` state.

use crate::identity::Identity;
use crate::{Error, Result};
use serde::Deserialize;
use sha2::{Digest, Sha256};

const FLOOR_FORMAT: &str = "osl.keyserver.sender-filter-capability-floor.v3";
const IDENTITY_ANCHOR_DOMAIN: &[u8] = b"OSL-SENDER-FILTER-FLOOR-IDENTITY-v1\0";
pub(crate) const SENDER_FILTER_CAPABILITY_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(crate) enum SenderFilterCapabilityFloor {
    Version1,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SenderFilterCapabilityFloorObservation {
    format: String,
    recipient_user_id: String,
    identity_anchor_sha256: String,
    capability_version: u32,
    monotonic_version: u64,
    first_observed_at_ms: i64,
    request_timestamp_ms: i64,
    request_id: String,
}

fn write_lp(output: &mut Vec<u8>, value: &[u8]) {
    output.extend_from_slice(&(value.len() as u32).to_be_bytes());
    output.extend_from_slice(value);
}

pub(crate) fn sender_filter_floor_identity_anchor_sha256(identity: &Identity) -> String {
    let mut canonical =
        Vec::with_capacity(IDENTITY_ANCHOR_DOMAIN.len() + identity.user_id.len() + 4 + 32);
    write_lp(&mut canonical, IDENTITY_ANCHOR_DOMAIN);
    write_lp(&mut canonical, identity.user_id.as_bytes());
    write_lp(&mut canonical, identity.ed25519_public.as_bytes());
    let digest = Sha256::digest(canonical);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub(crate) fn validate_sender_filter_capability_floor_observation(
    identity: &Identity,
    request_timestamp_ms: i64,
    request_id: &str,
    observation: SenderFilterCapabilityFloorObservation,
) -> Result<SenderFilterCapabilityFloor> {
    if observation.format != FLOOR_FORMAT
        || observation.recipient_user_id != identity.user_id
        || observation.identity_anchor_sha256
            != sender_filter_floor_identity_anchor_sha256(identity)
        || observation.capability_version != SENDER_FILTER_CAPABILITY_VERSION
        || observation.monotonic_version != 1
        || observation.first_observed_at_ms <= 0
        || observation.request_timestamp_ms != request_timestamp_ms
        || observation.request_id != request_id
        || request_timestamp_ms <= 0
        || request_id.len() != 43
        || !request_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    {
        return Err(Error::Transport(
            "sender-filter capability floor authority response mismatch".into(),
        ));
    }
    Ok(SenderFilterCapabilityFloor::Version1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn observation(
        identity: &Identity,
        request_timestamp_ms: i64,
        request_id: &str,
    ) -> SenderFilterCapabilityFloorObservation {
        SenderFilterCapabilityFloorObservation {
            format: FLOOR_FORMAT.to_owned(),
            recipient_user_id: identity.user_id.clone(),
            identity_anchor_sha256: sender_filter_floor_identity_anchor_sha256(identity),
            capability_version: SENDER_FILTER_CAPABILITY_VERSION,
            monotonic_version: 1,
            first_observed_at_ms: request_timestamp_ms,
            request_timestamp_ms,
            request_id: request_id.to_owned(),
        }
    }

    #[test]
    fn exact_nonempty_authority_observation_is_accepted() {
        let identity = crate::generate_identity("recipient-positive".into());
        let request_id = "A".repeat(43);
        assert_eq!(
            validate_sender_filter_capability_floor_observation(
                &identity,
                1_700_000_000_000,
                &request_id,
                observation(&identity, 1_700_000_000_000, &request_id),
            )
            .unwrap(),
            SenderFilterCapabilityFloor::Version1,
        );
    }

    #[test]
    fn stale_empty_mismatched_or_replayed_observations_are_refused() {
        let identity = crate::generate_identity("recipient-positive".into());
        let other = crate::generate_identity("recipient-other".into());
        let request_id = "A".repeat(43);
        let mut zero = observation(&identity, 1_700_000_000_000, &request_id);
        zero.monotonic_version = 0;
        assert!(validate_sender_filter_capability_floor_observation(
            &identity,
            1_700_000_000_000,
            &request_id,
            zero,
        )
        .is_err());
        let mut future = observation(&identity, 1_700_000_000_000, &request_id);
        future.monotonic_version = 2;
        assert!(validate_sender_filter_capability_floor_observation(
            &identity,
            1_700_000_000_000,
            &request_id,
            future,
        )
        .is_err());
        assert!(validate_sender_filter_capability_floor_observation(
            &other,
            1_700_000_000_000,
            &request_id,
            observation(&identity, 1_700_000_000_000, &request_id),
        )
        .is_err());
        assert!(validate_sender_filter_capability_floor_observation(
            &identity,
            1_700_000_000_001,
            &request_id,
            observation(&identity, 1_700_000_000_000, &request_id),
        )
        .is_err());
        assert!(validate_sender_filter_capability_floor_observation(
            &identity,
            1_700_000_000_000,
            &"B".repeat(43),
            observation(&identity, 1_700_000_000_000, &request_id),
        )
        .is_err());
    }
}
