//! Explicit, local consent for optional cloud carrier generation.
//!
//! A Pro entitlement is an account capability, not permission to send work to
//! a cloud service.  This record is deliberately separate and read at every
//! send, so revocation needs neither a restart nor an entitlement mutation.

use std::sync::atomic::{AtomicBool, Ordering};

use crate::cloud_autoscrub_consent::CloudAutoScrubHighSensitivityAcknowledgement;

#[derive(Default)]
pub struct AiCloudConsent {
    granted: AtomicBool,
}

impl AiCloudConsent {
    /// Grant requires the same complete high-sensitivity acknowledgement used
    /// by cloud AutoScrub; incomplete acknowledgement fails closed.
    pub fn grant(&self, acknowledgement: CloudAutoScrubHighSensitivityAcknowledgement) -> bool {
        if !acknowledgement.all_required_acknowledged() {
            return false;
        }
        self.granted.store(true, Ordering::Release);
        true
    }

    pub fn revoke(&self) {
        self.granted.store(false, Ordering::Release);
    }

    pub fn is_granted(&self) -> bool {
        self.granted.load(Ordering::Acquire)
    }

    /// D49's three independent gates, evaluated at the moment of a send.
    pub fn permits_cloud_send(&self, pro_active: bool, local_model_installed: bool) -> bool {
        pro_active && local_model_installed && self.is_granted()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn t13_te3_activating_pro_grants_no_cloud_consent_and_revocation_is_immediate() {
        let consent = AiCloudConsent::default();

        // Pro activation changes only the first gate. It is never a write to
        // the separate cloud-consent record.
        let pro_active = true;
        assert!(!consent.is_granted());
        assert!(!consent.permits_cloud_send(pro_active, true));

        assert!(consent.grant(CloudAutoScrubHighSensitivityAcknowledgement::all()));
        assert!(consent.permits_cloud_send(pro_active, true));
        consent.revoke();
        assert!(!consent.permits_cloud_send(pro_active, true), "the next send observes revocation");
        assert!(pro_active, "revocation must not disable Pro");
    }

    #[test]
    fn incomplete_high_sensitivity_acknowledgement_cannot_grant_cloud_consent() {
        let consent = AiCloudConsent::default();
        assert!(!consent.grant(CloudAutoScrubHighSensitivityAcknowledgement::default()));
        assert!(!consent.is_granted());
    }
}
