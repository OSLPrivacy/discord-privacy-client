//! TASK 1457 check: an approved AutoScrub schedule cannot be repointed to a
//! newly named, unapproved account.

use ipc::autoscrub_account_switches::{
    fixture_autoscrub_accounts, AutoScrubAccountSwitchSurface, ScrubAccountPermissionInput,
    ACCOUNT_NOT_APPROVED_FOR_AUTOSCRUB,
};
use ipc::autoscrub_pro_gate::{
    fixture_pro_code_directory, AutoScrubProSurface, AutoScrubScheduleRequest,
    AUTOSCRUB_SCHEDULE_COMMAND, FIXTURE_ACTIVE_PRO_CODE,
};

#[test]
fn task_1457_new_account_cannot_bypass_scrub_approval_when_scheduling() {
    let mut pro = AutoScrubProSurface::new(fixture_pro_code_directory());
    assert!(pro.present_pro_code(FIXTURE_ACTIVE_PRO_CODE).unlocks());
    let mut surface = AutoScrubAccountSwitchSurface::new(pro, fixture_autoscrub_accounts());
    surface
        .save_scrub_account_permissions(ScrubAccountPermissionInput::new(
            &["discord-maple", "discord-pine", "telegram-pine"],
            &["discord-maple"],
        ))
        .expect("approve only discord-maple");

    println!(
        "TASK1457_SCHEDULE_COUNT_BEFORE={}",
        surface.schedules().len()
    );
    assert_eq!(surface.schedules().len(), 0);

    let mut request = AutoScrubScheduleRequest {
        schedule_name: "maple-daily".to_string(),
        account: "discord-maple".to_string(),
        cadence: "daily".to_string(),
    };
    let accepted = surface
        .schedule(request.clone())
        .expect("maple schedule accepted");
    assert_eq!(accepted.schedule_count, 1);
    assert_eq!(accepted.saved.schedule_name, "maple-daily");
    assert_eq!(accepted.saved.account, "discord-maple");
    println!(
        "TASK1457_APPROVED schedule_name={} account={} count={}",
        accepted.saved.schedule_name, accepted.saved.account, accepted.schedule_count
    );
    assert_eq!(surface.schedules().len(), 1);

    request.account = "discord-pine".to_string();
    let refusal = surface
        .schedule(request)
        .expect_err("unapproved pine schedule must be refused");
    assert_eq!(refusal.command, AUTOSCRUB_SCHEDULE_COMMAND);
    assert_eq!(refusal.reason, ACCOUNT_NOT_APPROVED_FOR_AUTOSCRUB);
    assert!(refusal
        .message
        .contains("discord-pine is not approved for AutoScrub"));
    println!(
        "TASK1457_REFUSED account={} reason={} message={}",
        refusal.account_id, refusal.reason, refusal.message
    );

    let schedules = surface.schedules();
    println!(
        "TASK1457_PERSISTED schedule_name={} account={} count={}",
        schedules[0].schedule_name,
        schedules[0].account,
        schedules.len()
    );
    assert_eq!(schedules.len(), 1);
    assert_eq!(schedules[0].schedule_name, "maple-daily");
    assert_eq!(schedules[0].account, "discord-maple");
}
