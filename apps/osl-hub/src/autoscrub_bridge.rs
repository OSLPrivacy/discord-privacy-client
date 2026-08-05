//! The only path from an AutoScrub reviewed step to native IMAP mutation.
//!
//! A ledger step is not itself permission to delete. This bridge consumes its
//! short-lived pending authorization immediately before the native adapter is
//! invoked, and keeps a single-flight lease until read-back verification ends.

use crate::autoscrub_run::{AutoScrubRunError, AutoScrubRunState, NativeEntitlement, RunStepInput};
use crate::scrub_imap::{self, DeleteVerification, NativeImapAdapter, ScrubImapError};
use std::fmt;
use std::sync::Mutex;

/// Process-local exclusion for destructive AutoScrub operations.
#[derive(Default)]
pub struct AutoScrubBridge {
    in_flight: Mutex<()>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum AutoScrubBridgeError {
    Authorization(AutoScrubRunError),
    Native(ScrubImapError),
    StepInFlight,
    StateUnavailable,
}

impl fmt::Display for AutoScrubBridgeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Authorization(error) => write!(f, "AutoScrub authorization refused: {error}"),
            Self::Native(error) => write!(f, "native IMAP execution failed: {error}"),
            Self::StepInFlight => f.write_str("an AutoScrub deletion is already in flight"),
            Self::StateUnavailable => f.write_str("AutoScrub bridge state is unavailable"),
        }
    }
}

impl std::error::Error for AutoScrubBridgeError {}

/// Execute exactly one reviewed IMAP deletion.
///
/// The lease is intentionally acquired before asking the ledger for a step so
/// a rejected concurrent call cannot replace another call's pending authority.
pub fn execute_imap_step(
    bridge: &AutoScrubBridge,
    ledger: &AutoScrubRunState,
    owner: &str,
    entitlement: NativeEntitlement,
    account_id: &str,
    step: &RunStepInput,
    uid: u64,
    adapter: &mut dyn NativeImapAdapter,
) -> Result<DeleteVerification, AutoScrubBridgeError> {
    let _lease = match bridge.in_flight.try_lock() {
        Ok(lease) => lease,
        Err(std::sync::TryLockError::WouldBlock) => return Err(AutoScrubBridgeError::StepInFlight),
        Err(std::sync::TryLockError::Poisoned(_)) => {
            return Err(AutoScrubBridgeError::StateUnavailable)
        }
    };

    ledger
        .step(owner, entitlement, step)
        .map_err(AutoScrubBridgeError::Authorization)?;
    let authorization = ledger
        .authorize_imap_prepare(
            owner,
            entitlement,
            account_id,
            &step.channel_id,
            &step.item_id,
        )
        .map_err(AutoScrubBridgeError::Authorization)?;
    if authorization.mailbox != step.channel_id || authorization.message_id != step.item_id {
        return Err(AutoScrubBridgeError::Authorization(
            AutoScrubRunError::AuthorityRevoked,
        ));
    }

    scrub_imap::delete_and_verify(adapter, uid).map_err(AutoScrubBridgeError::Native)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::autoscrub_run::{
        DeleteCapabilityInput, OwnSessionProofInput, OwnedAccountRegistry, ReviewedBatchInput,
        RunConsentBatchInput, RunConsentInput, RunManifestInput,
    };
    use crate::models::ServiceKind;
    use crate::scrub_imap::QueryAfterDelete;
    use std::sync::{Arc, Barrier};
    use std::thread;
    use std::time::{SystemTime, UNIX_EPOCH};

    const OWNER: &str = "owner-local";
    const ACCOUNT: &str = "acct-primary";

    struct Owned;

    impl OwnedAccountRegistry for Owned {
        fn confirms_owned(&self, _owner: &str, _service: ServiceKind, _account_id: &str) -> bool {
            true
        }
    }

    fn now_ms() -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64
    }

    fn open(ledger: &AutoScrubRunState) -> crate::autoscrub_run::RunOpened {
        let now = now_ms();
        let item_key = "5:INBOX5:<a@x>".to_owned();
        let manifest = RunManifestInput {
            run_id: "run-a".to_owned(),
            provider_id: "imap".to_owned(),
            account_id: ACCOUNT.to_owned(),
            reviewed_at: now - 1_000,
            expires_at: now + 120_000,
            tos_caveat_acknowledged: true,
            unattended: true,
            max_items: 1,
            batches: vec![ReviewedBatchInput {
                plan_fingerprint: "plan-a".to_owned(),
                findings_fingerprint: "findings-a".to_owned(),
                item_keys: vec![item_key],
            }],
        };
        let session = OwnSessionProofInput {
            provider_id: "imap".to_owned(),
            account_id: ACCOUNT.to_owned(),
            session_kind: "owner_provisioned_credential".to_owned(),
            session_epoch: "epoch-1".to_owned(),
            discovered_by_owner_detection: true,
            credential_bound_to_osl_identity: Some(true),
            reused_osl_owned_profile: None,
        };
        let consent = RunConsentInput {
            run_id: "run-a".to_owned(),
            provider_id: "imap".to_owned(),
            account_id: ACCOUNT.to_owned(),
            acknowledged_at: now - 1_000,
            tos_caveat_acknowledged: true,
            unattended_acknowledged: true,
            estimated_item_count: 1,
            batches: vec![RunConsentBatchInput {
                plan_fingerprint: "plan-a".to_owned(),
                findings_fingerprint: "findings-a".to_owned(),
                item_count: 1,
            }],
        };
        let capability = DeleteCapabilityInput {
            mechanism: "documented_provider_delete_api".to_owned(),
            stop_on: vec![
                "owner_stop".to_owned(),
                "manifest_drift".to_owned(),
                "deadline".to_owned(),
                "global_stop".to_owned(),
            ],
        };
        ledger
            .open(
                OWNER,
                NativeEntitlement::Confirmed,
                &manifest,
                &session,
                &consent,
                &capability,
                &Owned,
            )
            .unwrap()
    }

    fn step(opened: &crate::autoscrub_run::RunOpened) -> RunStepInput {
        RunStepInput {
            run_id: opened.run_id.clone(),
            manifest_digest: opened.manifest_digest.clone(),
            consent_digest: opened.consent_digest.clone(),
            plan_fingerprint: "plan-a".to_owned(),
            findings_fingerprint: "findings-a".to_owned(),
            channel_id: "INBOX".to_owned(),
            item_id: "<a@x>".to_owned(),
        }
    }

    struct Adapter {
        entered: Option<Arc<Barrier>>,
        release: Option<Arc<Barrier>>,
    }

    impl NativeImapAdapter for Adapter {
        fn delete_message(&mut self, _uid: u64) -> Result<(), ScrubImapError> {
            if let Some(entered) = &self.entered {
                entered.wait();
                self.release.as_ref().unwrap().wait();
            }
            Ok(())
        }

        fn query_message(&mut self, _uid: u64) -> Result<QueryAfterDelete, ScrubImapError> {
            Ok(QueryAfterDelete::Gone)
        }
    }

    #[test]
    fn scr_a1_step_requires_live_authorization_and_refuses_second_in_flight_step() {
        let bridge = Arc::new(AutoScrubBridge::default());
        let ledger = Arc::new(AutoScrubRunState::default());
        let mut adapter = Adapter {
            entered: None,
            release: None,
        };
        let unopened = RunStepInput {
            run_id: "no-run".to_owned(),
            manifest_digest: "manifest".to_owned(),
            consent_digest: "consent".to_owned(),
            plan_fingerprint: "plan-a".to_owned(),
            findings_fingerprint: "findings-a".to_owned(),
            channel_id: "INBOX".to_owned(),
            item_id: "<a@x>".to_owned(),
        };
        assert_eq!(
            execute_imap_step(
                &bridge,
                &ledger,
                OWNER,
                NativeEntitlement::Confirmed,
                ACCOUNT,
                &unopened,
                1,
                &mut adapter,
            ),
            Err(AutoScrubBridgeError::Authorization(
                AutoScrubRunError::NoRun
            ))
        );

        let opened = open(&ledger);
        let first_step = step(&opened);
        let entered = Arc::new(Barrier::new(2));
        let release = Arc::new(Barrier::new(2));
        let first_bridge = Arc::clone(&bridge);
        let first_ledger = Arc::clone(&ledger);
        let first_entered = Arc::clone(&entered);
        let first_release = Arc::clone(&release);
        let worker = thread::spawn(move || {
            let mut blocking_adapter = Adapter {
                entered: Some(first_entered),
                release: Some(first_release),
            };
            execute_imap_step(
                &first_bridge,
                &first_ledger,
                OWNER,
                NativeEntitlement::Confirmed,
                ACCOUNT,
                &first_step,
                1,
                &mut blocking_adapter,
            )
        });
        entered.wait();

        assert_eq!(
            execute_imap_step(
                &bridge,
                &ledger,
                OWNER,
                NativeEntitlement::Confirmed,
                ACCOUNT,
                &step(&opened),
                2,
                &mut adapter,
            ),
            Err(AutoScrubBridgeError::StepInFlight),
            "SCR-A1: a second concurrent step must never reach native execution"
        );
        release.wait();
        assert_eq!(worker.join().unwrap(), Ok(DeleteVerification::VerifiedGone));
    }
}
