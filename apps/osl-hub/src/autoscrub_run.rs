//! Native AutoScrub run authority.
//!
//! This module is the production boundary for reviewed local cleanup runs. It
//! does not own service adapters and it does not perform deletion. It records
//! only bounded run state so the renderer can show the honest fleet status and
//! request a global stop without inventing send/delete authority.

use crate::models::ServiceKind;
use ipc::AppState;
use serde::{Deserialize, Serialize};
use std::sync::{Mutex, OnceLock};

const CONTRACT: &str = "autoscrubRunFleet.v1";
const PRO_REQUIRED: &str = "AutoScrub requires an active Pro license";
const MAX_OPEN_RUNS: usize = 2;
const MAX_ACCOUNT_ID_BYTES: usize = 64;
const MAX_REVIEW_TOKEN_BYTES: usize = 96;
const MAX_REVIEWED_ITEMS: u32 = 500;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AutoScrubRunPhase {
    ReviewRequired,
    Running,
    Stopping,
    Blocked,
    Complete,
    Failed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AutoScrubRunOutcome {
    None,
    Prepared,
    Confirmed,
    Held,
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AutoScrubQuitGuardState {
    NotRequested,
    Checking,
    Estimated,
    Stopped,
    Unknown,
    Refused,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoScrubRunSummary {
    pub run_id: String,
    pub service_id: ServiceKind,
    pub phase: AutoScrubRunPhase,
    pub reviewed_item_count: u32,
    pub remaining_item_count: u32,
    pub stop_requested: bool,
    pub mutation_allowed: bool,
    pub last_outcome: AutoScrubRunOutcome,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoScrubQuitGuardEstimate {
    pub state: AutoScrubQuitGuardState,
    pub honest_remaining_seconds_estimate: Option<u32>,
    pub reason: &'static str,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoScrubFleetStatus {
    pub contract: &'static str,
    pub open_run_count: usize,
    pub global_stop_requested: bool,
    pub unattended_execution_allowed: bool,
    pub quit_guard: AutoScrubQuitGuardEstimate,
    pub runs: Vec<AutoScrubRunSummary>,
}

#[derive(Clone, Copy, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum AutoScrubRunConsent {
    ReviewedBatchOnly,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutoScrubReviewedRunRequest {
    pub service_id: ServiceKind,
    pub account_id: String,
    pub review_token: String,
    pub plan_digest: String,
    pub reviewed_item_count: u32,
    pub consent: AutoScrubRunConsent,
}

#[derive(Default)]
struct AutoScrubRunStore {
    next_sequence: u64,
    runs: Vec<AutoScrubRunSummary>,
    global_stop_requested: bool,
}

static RUN_STORE: OnceLock<Mutex<AutoScrubRunStore>> = OnceLock::new();

fn run_store() -> &'static Mutex<AutoScrubRunStore> {
    RUN_STORE.get_or_init(|| Mutex::new(AutoScrubRunStore::default()))
}

#[cfg(test)]
fn reset_run_store_for_test() {
    *run_store().lock().expect("AutoScrub test run store lock") = AutoScrubRunStore::default();
}

pub fn fleet_status(state: &AppState) -> Result<AutoScrubFleetStatus, String> {
    require_pro(state)?;
    let store = run_store()
        .lock()
        .map_err(|_| "AutoScrub run store is unavailable".to_owned())?;
    Ok(store.fleet())
}

pub fn start_reviewed_run(
    state: &AppState,
    request: AutoScrubReviewedRunRequest,
) -> Result<AutoScrubFleetStatus, String> {
    require_pro(state)?;
    validate_reviewed_run_request(&request)?;
    let mut store = run_store()
        .lock()
        .map_err(|_| "AutoScrub run store is unavailable".to_owned())?;
    store.start_reviewed_run(request)
}

pub fn request_global_stop(state: &AppState) -> Result<AutoScrubFleetStatus, String> {
    require_pro(state)?;
    let mut store = run_store()
        .lock()
        .map_err(|_| "AutoScrub run store is unavailable".to_owned())?;
    Ok(store.request_global_stop())
}

fn require_pro(state: &AppState) -> Result<(), String> {
    if ipc::tier_gate::is_paid_equivalent(state) {
        Ok(())
    } else {
        Err(PRO_REQUIRED.to_owned())
    }
}

impl AutoScrubRunStore {
    fn start_reviewed_run(
        &mut self,
        request: AutoScrubReviewedRunRequest,
    ) -> Result<AutoScrubFleetStatus, String> {
        if self.open_runs() >= MAX_OPEN_RUNS {
            return Err("AutoScrub cannot open more than two reviewed runs".to_owned());
        }
        self.next_sequence = self.next_sequence.saturating_add(1);
        self.runs.push(AutoScrubRunSummary {
            run_id: format!("autoscrub-run-{:04}", self.next_sequence),
            service_id: request.service_id,
            phase: AutoScrubRunPhase::Running,
            reviewed_item_count: request.reviewed_item_count,
            remaining_item_count: request.reviewed_item_count,
            stop_requested: false,
            mutation_allowed: false,
            last_outcome: AutoScrubRunOutcome::Held,
        });
        Ok(self.fleet())
    }

    fn request_global_stop(&mut self) -> AutoScrubFleetStatus {
        self.global_stop_requested = true;
        for run in &mut self.runs {
            if matches!(
                run.phase,
                AutoScrubRunPhase::Running | AutoScrubRunPhase::ReviewRequired
            ) {
                run.phase = AutoScrubRunPhase::Stopping;
            }
            run.stop_requested = true;
        }
        self.fleet()
    }

    fn fleet(&self) -> AutoScrubFleetStatus {
        let open_run_count = self.open_runs();
        AutoScrubFleetStatus {
            contract: CONTRACT,
            open_run_count,
            global_stop_requested: self.global_stop_requested,
            unattended_execution_allowed: false,
            quit_guard: self.quit_guard(open_run_count),
            runs: self.runs.clone(),
        }
    }

    fn open_runs(&self) -> usize {
        self.runs
            .iter()
            .filter(|run| {
                matches!(
                    run.phase,
                    AutoScrubRunPhase::ReviewRequired
                        | AutoScrubRunPhase::Running
                        | AutoScrubRunPhase::Stopping
                        | AutoScrubRunPhase::Blocked
                )
            })
            .count()
    }

    fn quit_guard(&self, open_run_count: usize) -> AutoScrubQuitGuardEstimate {
        if !self.global_stop_requested {
            return AutoScrubQuitGuardEstimate {
                state: AutoScrubQuitGuardState::NotRequested,
                honest_remaining_seconds_estimate: None,
                reason: "No stop request is active.",
            };
        }
        if open_run_count == 0 {
            return AutoScrubQuitGuardEstimate {
                state: AutoScrubQuitGuardState::Stopped,
                honest_remaining_seconds_estimate: None,
                reason: "No open AutoScrub run remains.",
            };
        }
        AutoScrubQuitGuardEstimate {
            state: AutoScrubQuitGuardState::Estimated,
            honest_remaining_seconds_estimate: Some(honest_stop_estimate_seconds(&self.runs)),
            reason: "OSL is stopping after the checked local items already in review.",
        }
    }
}

fn honest_stop_estimate_seconds(runs: &[AutoScrubRunSummary]) -> u32 {
    runs.iter()
        .filter(|run| run.stop_requested)
        .map(|run| run.remaining_item_count.max(1).saturating_mul(30))
        .max()
        .unwrap_or(30)
}

fn validate_reviewed_run_request(request: &AutoScrubReviewedRunRequest) -> Result<(), String> {
    if !valid_opaque(&request.account_id, MAX_ACCOUNT_ID_BYTES)
        || !valid_opaque(&request.review_token, MAX_REVIEW_TOKEN_BYTES)
        || !valid_digest(&request.plan_digest)
        || request.reviewed_item_count == 0
        || request.reviewed_item_count > MAX_REVIEWED_ITEMS
        || request.consent != AutoScrubRunConsent::ReviewedBatchOnly
    {
        return Err("AutoScrub reviewed run request is invalid".to_owned());
    }
    Ok(())
}

fn valid_opaque(value: &str, max: usize) -> bool {
    !value.is_empty()
        && value.len() <= max
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(test)]
mod tests {
    use super::*;
    use keystore::{LicenseState, LicenseStateDto};

    fn state_with_license(state: LicenseState, raw_status: &str) -> AppState {
        let state_holder = AppState::new();
        *state_holder
            .license_state
            .lock()
            .expect("license state lock") = LicenseStateDto {
            state,
            raw_status: raw_status.to_owned(),
            current_period_end: None,
            last_validated_at: None,
        };
        state_holder
    }

    fn reviewed_request(
        service_id: ServiceKind,
        reviewed_item_count: u32,
    ) -> AutoScrubReviewedRunRequest {
        AutoScrubReviewedRunRequest {
            service_id,
            account_id: "acct-discord-1".to_owned(),
            review_token: format!("review-token-{reviewed_item_count}"),
            plan_digest: "a".repeat(64),
            reviewed_item_count,
            consent: AutoScrubRunConsent::ReviewedBatchOnly,
        }
    }

    #[test]
    fn full_autoscrub_native_authority_acceptance() {
        let _guard = crate::GLOBAL_KEYSTORE_TEST_LOCK
            .lock()
            .expect("global keystore test lock");
        reset_run_store_for_test();
        let state = state_with_license(LicenseState::Paid, "ACTIVE");

        let first = start_reviewed_run(&state, reviewed_request(ServiceKind::Discord, 3))
            .expect("first reviewed AutoScrub run");
        assert_eq!(first.open_run_count, 1);
        assert!(!first.global_stop_requested);
        assert!(!first.unattended_execution_allowed);
        assert_eq!(first.runs[0].phase, AutoScrubRunPhase::Running);
        assert!(!first.runs[0].mutation_allowed);

        let second = start_reviewed_run(&state, reviewed_request(ServiceKind::Telegram, 5))
            .expect("second reviewed AutoScrub run");
        assert_eq!(second.open_run_count, 2);
        assert_eq!(second.runs.len(), 2);
        assert!(start_reviewed_run(&state, reviewed_request(ServiceKind::Signal, 1)).is_err());

        let stopped = request_global_stop(&state).expect("global AutoScrub stop request");
        assert_eq!(stopped.contract, CONTRACT);
        assert_eq!(stopped.open_run_count, 2);
        assert!(stopped.global_stop_requested);
        assert_eq!(stopped.quit_guard.state, AutoScrubQuitGuardState::Estimated);
        assert_eq!(
            stopped.quit_guard.honest_remaining_seconds_estimate,
            Some(150)
        );
        assert!(stopped.runs.iter().all(|run| run.stop_requested
            && run.phase == AutoScrubRunPhase::Stopping
            && !run.mutation_allowed));

        assert_eq!(fleet_status(&state).unwrap().contract, CONTRACT);
        let free = state_with_license(LicenseState::Free, "Unconfigured");
        assert_eq!(fleet_status(&free).unwrap_err(), PRO_REQUIRED);

        assert!(serde_json::from_str::<AutoScrubReviewedRunRequest>(
            r#"{"serviceId":"discord","accountId":"acct-1","reviewToken":"review-1","planDigest":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","reviewedItemCount":1,"consent":"reviewedBatchOnly","unattended":true}"#,
        )
        .is_err());
        assert!(serde_json::from_str::<AutoScrubReviewedRunRequest>(
            r#"{"serviceId":"discord","accountId":"acct-1","reviewToken":"review-1","planDigest":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","reviewedItemCount":1}"#,
        )
        .is_err());
    }
}
