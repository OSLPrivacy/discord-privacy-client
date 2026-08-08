//! TASK 1471 regression: changing only a Find-and-delete submission's
//! agreement from yes to no must refuse the submission before schedule state
//! changes.

use std::path::PathBuf;

use ipc::autoscrub_deletion_agreement::{
    AutoScrubDeletionAgreementRequest, DELETION_AGREEMENT_REQUIRED,
};
use ipc::autoscrub_pro_gate::{
    fixture_pro_code_directory, AutoScrubProSurface, AutoScrubScheduleRequest,
    FIXTURE_ACTIVE_PRO_CODE,
};
use ipc::autoscrub_schedule_mode::{
    AutoScrubScheduleMode, AutoScrubScheduleModeStore, FIND_AND_DELETE_LABEL,
};
use ipc::commands::{
    cmd_osl_authorize_autoscrub_find_and_delete, cmd_osl_save_autoscrub_bad_message_rule,
    cmd_osl_save_autoscrub_deletion_agreement,
};
use ipc::state::AppState;
use tempfile::tempdir;

struct FileKeyGuard;

impl FileKeyGuard {
    fn install() -> Self {
        ipc::main_password::set_file_storage_key(Some([0x71; 32]));
        Self
    }
}

impl Drop for FileKeyGuard {
    fn drop(&mut self) {
        ipc::main_password::set_file_storage_key(None);
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct DeleteScheduleSubmission {
    schedule_name: String,
    account_id: String,
    cadence: String,
    mode: AutoScrubScheduleMode,
    selected_rule_names: Vec<String>,
    agreement: bool,
}

#[derive(Debug, Eq, PartialEq)]
struct SavedDeleteSchedule {
    schedule_name: String,
    mode_label: &'static str,
}

struct DeleteScheduleCoordinator {
    pro: AutoScrubProSurface,
    modes: AutoScrubScheduleModeStore,
}

struct AgreementFixture {
    state: AppState,
    config_dir: PathBuf,
}

impl AgreementFixture {
    fn new(config_dir: PathBuf) -> Self {
        let state = AppState::new();
        cmd_osl_save_autoscrub_bad_message_rule(
            &state,
            "private words".to_owned(),
            "MAPLE-DELETE".to_owned(),
            Some(config_dir.clone()),
        )
        .expect("fixture rule saves");

        Self { state, config_dir }
    }
}

impl DeleteScheduleCoordinator {
    fn new() -> Self {
        let mut pro = AutoScrubProSurface::new(fixture_pro_code_directory());
        assert!(pro.present_pro_code(FIXTURE_ACTIVE_PRO_CODE).unlocks());
        Self {
            pro,
            modes: AutoScrubScheduleModeStore::new(),
        }
    }

    fn submit(
        &mut self,
        agreement_fixture: &AgreementFixture,
        request: &DeleteScheduleSubmission,
    ) -> Result<SavedDeleteSchedule, String> {
        if request.mode != AutoScrubScheduleMode::FindAndDelete {
            return Err("TASK1471 fixture only accepts Find and delete".to_owned());
        }

        // This is deliberately part of each schedule submission. The clean
        // no-agreement copy must reach this gate before shared schedule state.
        cmd_osl_save_autoscrub_deletion_agreement(
            &agreement_fixture.state,
            AutoScrubDeletionAgreementRequest {
                selected_account_ids: vec![request.account_id.clone()],
                selected_rule_names: request.selected_rule_names.clone(),
                deleted_messages_can_be_permanent: true,
                service_rules_may_forbid_automated_reading_or_deletion: true,
                suspension_or_ban_risk_is_real: true,
                allow_autoscrub_to_delete_matching_messages: request.agreement,
            },
            Some(agreement_fixture.config_dir.clone()),
        )?;
        cmd_osl_authorize_autoscrub_find_and_delete(
            &agreement_fixture.state,
            vec![request.account_id.clone()],
            request.selected_rule_names.clone(),
        )?;

        let saved_mode = self
            .modes
            .save_mode(&request.account_id, request.mode)
            .map_err(|error| error.to_string())?;
        let scheduled = self
            .pro
            .schedule(AutoScrubScheduleRequest {
                schedule_name: request.schedule_name.clone(),
                account: request.account_id.clone(),
                cadence: request.cadence.clone(),
            })
            .map_err(|refusal| refusal.message)?;

        Ok(SavedDeleteSchedule {
            schedule_name: scheduled.saved.schedule_name,
            mode_label: saved_mode.label,
        })
    }

    fn delete_schedules(&self) -> Vec<SavedDeleteSchedule> {
        self.pro
            .schedules()
            .iter()
            .filter_map(|schedule| {
                let mode = self.modes.mode_for(&schedule.account).ok()?;
                (mode.mode == AutoScrubScheduleMode::FindAndDelete).then(|| SavedDeleteSchedule {
                    schedule_name: schedule.schedule_name.clone(),
                    mode_label: mode.label,
                })
            })
            .collect()
    }
}

fn accepted_submission() -> DeleteScheduleSubmission {
    DeleteScheduleSubmission {
        schedule_name: "maple-delete".to_owned(),
        account_id: "discord-maple".to_owned(),
        cadence: "daily".to_owned(),
        mode: AutoScrubScheduleMode::FindAndDelete,
        selected_rule_names: vec!["private words".to_owned()],
        agreement: true,
    }
}

#[test]
fn task_1471_agreement_no_refuses_without_adding_or_replacing_delete_schedule() {
    let _file_key = FileKeyGuard::install();
    let agreed_dir = tempdir().unwrap();
    let refused_dir = tempdir().unwrap();
    let agreed_fixture = AgreementFixture::new(agreed_dir.path().to_path_buf());
    let refused_clean_fixture = AgreementFixture::new(refused_dir.path().to_path_buf());
    let mut coordinator = DeleteScheduleCoordinator::new();

    let before = coordinator.delete_schedules();
    println!("TASK1471_DELETE_SCHEDULE_COUNT_BEFORE={}", before.len());
    assert_eq!(before.len(), 0);

    let agreed = accepted_submission();
    let saved = coordinator
        .submit(&agreed_fixture, &agreed)
        .expect("agreement yes creates the delete schedule");
    assert_eq!(saved.schedule_name, "maple-delete");
    assert_eq!(saved.mode_label, FIND_AND_DELETE_LABEL);
    let after_yes = coordinator.delete_schedules();
    println!(
        "TASK1471_ACCEPTED schedule_name={} mode={} agreement=yes delete_schedule_count={}",
        saved.schedule_name,
        saved.mode_label,
        after_yes.len()
    );
    assert_eq!(after_yes, vec![saved]);
    assert_eq!(after_yes.len(), 1);

    // Equal copy: the sole changed field is the agreement answer.
    let mut no_agreement = agreed.clone();
    no_agreement.agreement = false;
    assert_eq!(no_agreement.schedule_name, agreed.schedule_name);
    assert_eq!(no_agreement.account_id, agreed.account_id);
    assert_eq!(no_agreement.cadence, agreed.cadence);
    assert_eq!(no_agreement.mode, agreed.mode);
    assert_eq!(no_agreement.selected_rule_names, agreed.selected_rule_names);
    assert_ne!(no_agreement.agreement, agreed.agreement);

    let refusal = coordinator
        .submit(&refused_clean_fixture, &no_agreement)
        .expect_err("agreement no must refuse Find and delete");
    assert_eq!(refusal, DELETION_AGREEMENT_REQUIRED);
    let clean_copy_authorization = cmd_osl_authorize_autoscrub_find_and_delete(
        &refused_clean_fixture.state,
        vec![no_agreement.account_id.clone()],
        no_agreement.selected_rule_names.clone(),
    )
    .expect_err("clean copy must hold no reusable deletion agreement");
    assert_eq!(clean_copy_authorization, DELETION_AGREEMENT_REQUIRED);
    let after_no = coordinator.delete_schedules();
    println!(
        "TASK1471_REFUSED schedule_name={} mode={} agreement=no reason={} only_schedule={} delete_schedule_count={}",
        no_agreement.schedule_name,
        no_agreement.mode.label(),
        refusal,
        after_no[0].schedule_name,
        after_no.len()
    );
    assert_eq!(after_no, after_yes);
    assert_eq!(after_no.len(), 1);
    assert_eq!(after_no[0].schedule_name, "maple-delete");
    assert_eq!(after_no[0].mode_label, FIND_AND_DELETE_LABEL);
}
