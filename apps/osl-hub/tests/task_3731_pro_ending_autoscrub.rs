//! TASK 3731: an active AutoScrub run pauses when Pro ends.

use ipc::AppState;
use keystore::{LicenseState, LicenseStateDto};
use osl_privacy_hub::autoscrub_run::{
    fleet_status, run_reviewed_account_plan_until_stop, start_reviewed_run,
    AutoScrubReviewedRunRequest, AutoScrubRunConsent, AutoScrubRunPhase,
    AUTOSCRUB_PRO_ENDED_NOTICE, AUTOSCRUB_RESULT_NOT_STARTED, AUTOSCRUB_RESULT_PAUSED,
};
use osl_privacy_hub::models::ServiceKind;

fn paid_state() -> AppState {
    let state = AppState::new();
    *state.license_state.lock().expect("license state lock") = LicenseStateDto {
        state: LicenseState::Paid,
        raw_status: "ACTIVE".to_owned(),
        current_period_end: None,
        last_validated_at: None,
    };
    state
}

fn reviewed_request(
    service_id: ServiceKind,
    account_id: &str,
    reviewed_item_count: u32,
) -> AutoScrubReviewedRunRequest {
    AutoScrubReviewedRunRequest {
        service_id,
        account_id: account_id.to_owned(),
        review_token: format!("review-token-{account_id}"),
        plan_digest: "a".repeat(64),
        reviewed_item_count,
        consent: AutoScrubRunConsent::ReviewedBatchOnly,
    }
}

#[test]
fn pro_ending_pauses_the_active_run_keeps_progress_and_refuses_a_new_run() {
    let state = paid_state();
    let first = reviewed_request(ServiceKind::Discord, "acct-discord-3731", 3);
    let second = reviewed_request(ServiceKind::Telegram, "acct-telegram-3731", 3);

    let results =
        run_reviewed_account_plan_until_stop(&state, vec![first.clone(), second.clone()], |_| {
            *state.license_state.lock().expect("license state lock") = LicenseStateDto {
                state: LicenseState::Free,
                raw_status: "EXPIRED".to_owned(),
                current_period_end: None,
                last_validated_at: None,
            };
            false
        })
        .expect("the active account reports its paused outcome");

    assert_eq!(results.len(), 2);
    assert_eq!(results[0].account_id, first.account_id);
    assert_eq!(results[0].result, AUTOSCRUB_RESULT_PAUSED);
    assert_eq!(results[1].account_id, second.account_id);
    assert_eq!(results[1].result, AUTOSCRUB_RESULT_NOT_STARTED);

    let paused = fleet_status(&state).expect("the paused run remains visible with its notice");
    assert_eq!(paused.open_run_count, 1);
    assert_eq!(paused.runs.len(), 1);
    let run = &paused.runs[0];
    assert_eq!(run.service_id, ServiceKind::Discord);
    assert_eq!(run.phase, AutoScrubRunPhase::Paused);
    assert_eq!(run.reviewed_item_count, 3);
    assert_eq!(run.scrubbed_item_count, 1);
    assert_eq!(run.remaining_item_count, 2);
    assert!(run.stop_requested);
    assert!(!run.mutation_allowed);
    assert_eq!(run.pause_notice, Some(AUTOSCRUB_PRO_ENDED_NOTICE));
    assert!(paused
        .runs
        .iter()
        .all(|saved| saved.service_id != ServiceKind::Telegram));

    let new_run_refusal = start_reviewed_run(
        &state,
        reviewed_request(ServiceKind::Signal, "acct-signal-3731", 3),
    )
    .expect_err("Pro expiry refuses a new run");
    assert_eq!(new_run_refusal, AUTOSCRUB_PRO_ENDED_NOTICE);

    let after_refusal = fleet_status(&state).expect("the saved paused run remains intact");
    assert_eq!(after_refusal.runs.len(), 1);
    assert_eq!(after_refusal.runs[0].scrubbed_item_count, 1);
    assert_eq!(after_refusal.runs[0].remaining_item_count, 2);
    println!(
        "TASK3731 result={} saved_scrubbed={} saved_remaining={} approved_runs={} new_run_notice={}",
        results[0].result,
        after_refusal.runs[0].scrubbed_item_count,
        after_refusal.runs[0].remaining_item_count,
        after_refusal.runs.len(),
        new_run_refusal,
    );
}
