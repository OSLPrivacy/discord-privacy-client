//! Compatibility surface for cloud AutoScrub run inputs.
//!
//! The implementation is the native `autoscrub_run` ledger. This module keeps
//! the cloud-named run input contract testable without adding a second ledger.

use serde::Deserialize;

pub use crate::autoscrub_run::{ReviewedBatchInput, RunConsentInput, RunManifestInput};

#[derive(Clone, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RunConsentBatchInput {
    pub plan_fingerprint: String,
    pub findings_fingerprint: String,
    pub item_keys: Vec<String>,
}

impl RunConsentBatchInput {
    pub fn matches_reviewed_batch(&self, reviewed: &ReviewedBatchInput) -> bool {
        self.plan_fingerprint == reviewed.plan_fingerprint
            && self.findings_fingerprint == reviewed.findings_fingerprint
            && self.item_keys == reviewed.item_keys
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::autoscrub_run::{
        AutoScrubRunError, AutoScrubRunState, DeleteCapabilityInput, NativeEntitlement,
        OwnSessionProofInput, OwnedAccountRegistry,
    };
    use crate::models::ServiceKind;
    use serde_json::json;
    use std::time::{SystemTime, UNIX_EPOCH};

    const OWNER: &str = "owner-local";
    const ACCOUNT: &str = "acct-primary";
    const PROVIDER: &str = "imap";

    struct Registry;

    impl OwnedAccountRegistry for Registry {
        fn confirms_owned(&self, owner: &str, service: ServiceKind, account_id: &str) -> bool {
            owner == OWNER && service == ServiceKind::Email && account_id == ACCOUNT
        }
    }

    fn now_ms() -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock after unix epoch")
            .as_millis() as i64
    }

    fn manifest_json() -> serde_json::Value {
        let now = now_ms();
        json!({
            "runId": "run-strict-1",
            "providerId": PROVIDER,
            "accountId": ACCOUNT,
            "reviewedAt": now - 1000,
            "expiresAt": now + 120000,
            "tosCaveatAcknowledged": true,
            "unattended": true,
            "maxItems": 2,
            "batches": [{
                "planFingerprint": "plan-a",
                "findingsFingerprint": "findings-a",
                "itemKeys": ["5:INBOX5:<a@x>"]
            }]
        })
    }

    fn consent_json() -> serde_json::Value {
        json!({
            "runId": "run-strict-1",
            "providerId": PROVIDER,
            "accountId": ACCOUNT,
            "acknowledgedAt": now_ms() - 1000,
            "tosCaveatAcknowledged": true,
            "unattendedAcknowledged": true,
            "estimatedItemCount": 1,
            "batches": [{
                "planFingerprint": "plan-a",
                "findingsFingerprint": "findings-a",
                "itemCount": 1
            }]
        })
    }

    fn session() -> OwnSessionProofInput {
        OwnSessionProofInput {
            provider_id: PROVIDER.to_owned(),
            account_id: ACCOUNT.to_owned(),
            session_kind: "owner_provisioned_credential".to_owned(),
            session_epoch: "epoch-1".to_owned(),
            discovered_by_owner_detection: true,
            credential_bound_to_osl_identity: Some(true),
            reused_osl_owned_profile: None,
        }
    }

    fn capability() -> DeleteCapabilityInput {
        DeleteCapabilityInput {
            mechanism: "documented_provider_delete_api".to_owned(),
            stop_on: vec![
                "owner_stop".to_owned(),
                "manifest_drift".to_owned(),
                "deadline".to_owned(),
                "global_stop".to_owned(),
            ],
        }
    }

    fn open_with(
        manifest: &RunManifestInput,
        consent: &RunConsentInput,
    ) -> Result<(), AutoScrubRunError> {
        AutoScrubRunState::default()
            .open(
                OWNER,
                NativeEntitlement::Confirmed,
                manifest,
                &session(),
                consent,
                &capability(),
                &Registry,
            )
            .map(|_| ())
    }

    #[test]
    fn cloud_autoscrub_run_inputs_and_manifest_validate_strictly() {
        let manifest: RunManifestInput =
            serde_json::from_value(manifest_json()).expect("valid manifest input");
        let consent: RunConsentInput =
            serde_json::from_value(consent_json()).expect("valid consent input");
        let consent_batch: RunConsentBatchInput =
            serde_json::from_value(manifest_json()["batches"][0].clone())
                .expect("valid consent batch input");

        assert_eq!(open_with(&manifest, &consent), Ok(()));
        assert!(consent_batch.matches_reviewed_batch(&manifest.batches[0]));

        let mut manifest_with_extra = manifest_json();
        manifest_with_extra["unexpected"] = json!(true);
        assert!(serde_json::from_value::<RunManifestInput>(manifest_with_extra).is_err());

        let mut batch_with_extra = manifest_json();
        batch_with_extra["batches"][0]["unexpected"] = json!("ignored-if-permissive");
        assert!(serde_json::from_value::<RunManifestInput>(batch_with_extra).is_err());

        let mut consent_batch_with_extra = manifest_json()["batches"][0].clone();
        consent_batch_with_extra["unexpected"] = json!(true);
        assert!(serde_json::from_value::<RunConsentBatchInput>(consent_batch_with_extra).is_err());

        let mut drifted_consent_batch = consent_batch.clone();
        drifted_consent_batch.findings_fingerprint = "findings-other".to_owned();
        assert!(!drifted_consent_batch.matches_reviewed_batch(&manifest.batches[0]));

        let mut consent_with_extra = consent_json();
        consent_with_extra["unexpected"] = json!(true);
        assert!(serde_json::from_value::<RunConsentInput>(consent_with_extra).is_err());

        let mut missing_required = manifest_json();
        missing_required
            .as_object_mut()
            .expect("manifest object")
            .remove("tosCaveatAcknowledged");
        assert!(serde_json::from_value::<RunManifestInput>(missing_required).is_err());

        let mut mismatched_consent = consent.clone();
        mismatched_consent.estimated_item_count = 2;
        assert_eq!(
            open_with(&manifest, &mismatched_consent),
            Err(AutoScrubRunError::ConsentRequired)
        );

        let mut overbroad_manifest = manifest.clone();
        overbroad_manifest.max_items = 0;
        assert_eq!(
            open_with(&overbroad_manifest, &consent),
            Err(AutoScrubRunError::BoundsExceeded)
        );
    }
}
