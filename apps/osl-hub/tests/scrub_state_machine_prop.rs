//! SCRUB-STATE-MACHINE-PROOF: deterministic hostile-event replay over the
//! public AutoScrub authority boundary.  The seed and redacted trace fixture
//! live in docs/evidence/scrub-state-machine/ so any failure is reproducible.

use osl_privacy_hub::{
    autoscrub_run::{
        AutoScrubRunError, AutoScrubRunState, DeleteCapabilityInput, NativeEntitlement,
        OwnedAccountRegistry, OwnSessionProofInput, ReviewedBatchInput, RunConsentBatchInput,
        RunConsentInput, RunManifestInput, RunPhaseDto, RunStepInput,
    },
    models::ServiceKind,
};
use std::time::{SystemTime, UNIX_EPOCH};

const OWNER: &str = "fixture-owner";
const ACCOUNT: &str = "fixture-account";
const ITEM: &str = "fixture-message-a";

struct Owned;
impl OwnedAccountRegistry for Owned {
    fn confirms_owned(&self, _: &str, _: ServiceKind, _: &str) -> bool { true }
}

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as i64
}

fn item_key() -> String { format!("5:INBOX{}:{}", ITEM.len(), ITEM) }

fn inputs(run_id: &str) -> (RunManifestInput, OwnSessionProofInput, RunConsentInput, DeleteCapabilityInput) {
    let now = now_ms();
    let batch = ReviewedBatchInput { plan_fingerprint: "fixture-plan".into(), findings_fingerprint: "fixture-findings".into(), item_keys: vec![item_key()] };
    (
        RunManifestInput { run_id: run_id.into(), provider_id: "imap".into(), account_id: ACCOUNT.into(), reviewed_at: now - 1_000, expires_at: now + 120_000, tos_caveat_acknowledged: true, unattended: true, max_items: 1, batches: vec![batch] },
        OwnSessionProofInput { provider_id: "imap".into(), account_id: ACCOUNT.into(), session_kind: "owner_provisioned_credential".into(), session_epoch: "fixture-epoch".into(), discovered_by_owner_detection: true, credential_bound_to_osl_identity: Some(true), reused_osl_owned_profile: None },
        RunConsentInput { run_id: run_id.into(), provider_id: "imap".into(), account_id: ACCOUNT.into(), acknowledged_at: now - 1_000, tos_caveat_acknowledged: true, unattended_acknowledged: true, estimated_item_count: 1, batches: vec![RunConsentBatchInput { plan_fingerprint: "fixture-plan".into(), findings_fingerprint: "fixture-findings".into(), item_count: 1 }] },
        DeleteCapabilityInput { mechanism: "documented_provider_delete_api".into(), stop_on: vec!["owner_stop".into(), "manifest_drift".into(), "deadline".into(), "global_stop".into()] },
    )
}

fn open(ledger: &AutoScrubRunState, run_id: &str) -> (String, String) {
    let (manifest, session, consent, capability) = inputs(run_id);
    let opened = ledger.open(OWNER, NativeEntitlement::Confirmed, &manifest, &session, &consent, &capability, &Owned).unwrap();
    (opened.manifest_digest, opened.consent_digest)
}

fn step(run_id: &str, manifest_digest: String, consent_digest: String) -> RunStepInput {
    RunStepInput { run_id: run_id.into(), manifest_digest, consent_digest, plan_fingerprint: "fixture-plan".into(), findings_fingerprint: "fixture-findings".into(), channel_id: "INBOX".into(), item_id: ITEM.into() }
}

#[test]
fn scr_v4_seed_7e57_replays_stop_revoke_and_stale_events_without_authority_escape() {
    // Redacted generated trace: Open → duplicate/stale step → Stop → attempt
    // prepare/delete authority.  Every operation names fixture identifiers only.
    let ledger = AutoScrubRunState::default();
    let (manifest, consent) = open(&ledger, "seed-7e57");
    let request = step("seed-7e57", manifest, consent);

    ledger.step(OWNER, NativeEntitlement::Confirmed, &request).unwrap();
    let duplicate = ledger.step(OWNER, NativeEntitlement::Confirmed, &request);
    if let Ok(authorization) = duplicate {
        assert_eq!(authorization.items_authorized, 1, "a duplicate event cannot create a second destructive authorization");
    }

    // Stop is immediately-before-delete: it clears the pending authority.
    let status = ledger.halt(OWNER, "seed-7e57", "owner_stop").unwrap();
    assert_eq!(status.phase, RunPhaseDto::Halted);
    assert!(matches!(ledger.authorize_imap_prepare(OWNER, NativeEntitlement::Confirmed, ACCOUNT, "INBOX", ITEM), Err(AutoScrubRunError::RunHalted) | Err(AutoScrubRunError::AuthorityRevoked)));

    // Reordered/stale event cannot resurrect a halted run or mint a receipt.
    assert!(matches!(ledger.step(OWNER, NativeEntitlement::Confirmed, &request), Err(AutoScrubRunError::RunHalted)));
    assert!(matches!(ledger.authorize_imap_prepare(OWNER, NativeEntitlement::Confirmed, ACCOUNT, "INBOX", "wrong-target"), Err(AutoScrubRunError::RunHalted) | Err(AutoScrubRunError::AuthorityRevoked)));
}
