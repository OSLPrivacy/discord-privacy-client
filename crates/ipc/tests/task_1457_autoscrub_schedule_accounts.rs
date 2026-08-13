//! Task 1457 check: the AutoScrub schedule cannot smuggle in a new account.
//!
//! Task 1455 put the approval check in front of the account *switch*. A schedule
//! row names an account too, and a saved schedule can be edited afterwards —
//! that edit is the bypass this check closes.
//!
//! Finish line, item by item:
//!
//! 1. the schedule-row count is 0 before and 1 after `maple-daily` names the
//!    approved account `discord-maple`;
//! 2. `discord-pine` is refused, by name, as `account_not_approved_for_autoscrub`
//!    when it is the only thing changed on that row;
//! 3. `maple-daily` still names only `discord-maple` and the count stays 1.

use ipc::autoscrub_account_switches::{
    fixture_autoscrub_accounts, fixture_available_account_ids, AutoScrubAccount,
    AutoScrubAccountSwitchSurface, ScrubAccountPermissionInput, ACCOUNT_NOT_APPROVED_FOR_AUTOSCRUB,
    UNKNOWN_ACCOUNT,
};
use ipc::autoscrub_pro_gate::{
    fixture_pro_code_directory, AutoScrubProSurface, AUTOSCRUB_SCHEDULE_COMMAND, PRO_CODE_REQUIRED,
};
use ipc::autoscrub_schedule_accounts::{
    fixture_schedule_surface, is_schedule_name, run_autoscrub_schedule_account_command,
    AutoScrubScheduleSurface, AUTOSCRUB_SCHEDULE_ACCOUNT_COMMAND, AUTOSCRUB_SCHEDULE_ROWS_COMMAND,
    FIXTURE_APPROVED_ACCOUNT, FIXTURE_CADENCE, FIXTURE_SCHEDULE_NAME, FIXTURE_UNAPPROVED_ACCOUNT,
    INVALID_SCHEDULE_NAME, UNKNOWN_COMMAND, UNKNOWN_SCHEDULE,
};

/// The three names the finish line is written in.
const SCHEDULE: &str = "maple-daily";
const APPROVED: &str = "discord-maple";
const UNAPPROVED: &str = "discord-pine";

/// The fixture constants and the finish line have to be the same strings, or
/// this file would be checking a different scenario than the one asked for.
#[test]
fn the_fixture_names_are_the_finish_line_names() {
    assert_eq!(FIXTURE_SCHEDULE_NAME, SCHEDULE);
    assert_eq!(FIXTURE_APPROVED_ACCOUNT, APPROVED);
    assert_eq!(FIXTURE_UNAPPROVED_ACCOUNT, UNAPPROVED);
    assert!(is_schedule_name(SCHEDULE), "{SCHEDULE} must be nameable");
    // Both are real signed-in accounts, so what separates them is the approval
    // and nothing else.
    let known: Vec<String> = fixture_available_account_ids();
    assert!(known.contains(&APPROVED.to_string()));
    assert!(known.contains(&UNAPPROVED.to_string()));
}

/// A surface with the Pro gate open and the normal Scrub consent approving
/// `discord-maple` and nothing else.
fn surface() -> AutoScrubScheduleSurface {
    let surface = fixture_schedule_surface();
    assert!(
        surface.switches().pro().pro_unlocked(),
        "the Task 1454 Pro gate must be open so this check measures the approval, not Pro"
    );
    let approved = surface.switches().scrub_consent().approved_account_ids();
    assert_eq!(
        approved,
        vec![APPROVED.to_string()],
        "the fixture must approve exactly one account"
    );
    surface
}

/// THE FINISH LINE, in one run, in the order it is written.
#[test]
fn the_schedule_cannot_be_edited_into_naming_an_unapproved_account() {
    let mut surface = surface();

    // 1a. count is 0 before.
    assert_eq!(surface.schedule_row_count(), 0, "count before the save");
    assert!(surface.schedule_row(SCHEDULE).is_none());
    assert_eq!(surface.accounts_named_by(SCHEDULE), Vec::<String>::new());

    // 1b. count is 1 after maple-daily names discord-maple.
    let saved = surface
        .save_schedule(SCHEDULE, APPROVED, FIXTURE_CADENCE)
        .expect("an approved account may be scheduled");
    assert_eq!(saved.row_count, 1, "count after the save");
    assert_eq!(surface.schedule_row_count(), 1);
    assert_eq!(saved.saved.schedule_name, SCHEDULE);
    assert_eq!(saved.saved.account_id, APPROVED);
    assert_eq!(
        surface.accounts_named_by(SCHEDULE),
        vec![APPROVED.to_string()]
    );

    // The row exactly as it stands, so step 3 can prove nothing moved.
    let row_before = surface.schedule_row(SCHEDULE).expect("saved").clone();

    // 2. change ONLY the account name to discord-pine — refused, by name.
    let refused = surface
        .set_schedule_account(SCHEDULE, UNAPPROVED)
        .expect_err("an unapproved account must be refused");
    assert_eq!(refused.reason, ACCOUNT_NOT_APPROVED_FOR_AUTOSCRUB);
    assert_eq!(refused.account_id, UNAPPROVED);
    assert_eq!(refused.command, AUTOSCRUB_SCHEDULE_ACCOUNT_COMMAND);
    assert!(
        refused.message.contains(UNAPPROVED),
        "the refusal names the account it refused: {}",
        refused.message
    );

    // 3. maple-daily still names ONLY discord-maple and the count is still 1.
    assert_eq!(
        surface.accounts_named_by(SCHEDULE),
        vec![APPROVED.to_string()],
        "the schedule must still name only the approved account"
    );
    assert_eq!(surface.schedule_row_count(), 1, "count after the refusal");
    assert_eq!(
        surface.schedule_row(SCHEDULE).expect("still saved"),
        &row_before,
        "a refused edit leaves the row byte-for-byte as it was"
    );
    assert!(
        !surface
            .scheduled_account_ids()
            .contains(&UNAPPROVED.to_string()),
        "no schedule anywhere may name the unapproved account"
    );
}

/// The same three steps through the direct invoke, which skips every screen.
/// If the check lived in the listing rather than the command, this is where it
/// would show.
#[test]
fn the_direct_invoke_is_no_way_around_the_approval() {
    let mut surface = surface();

    let before =
        run_autoscrub_schedule_account_command(&mut surface, AUTOSCRUB_SCHEDULE_ROWS_COMMAND, "{}");
    assert!(before.contains("\"rowCount\":0"), "{before}");

    let saved = run_autoscrub_schedule_account_command(
        &mut surface,
        AUTOSCRUB_SCHEDULE_COMMAND,
        &serde_json::json!({
            "scheduleName": SCHEDULE,
            "accountId": APPROVED,
            "cadence": FIXTURE_CADENCE,
        })
        .to_string(),
    );
    assert!(saved.contains("\"ok\":true"), "{saved}");
    assert!(saved.contains("\"rowCount\":1"), "{saved}");

    let refused = run_autoscrub_schedule_account_command(
        &mut surface,
        AUTOSCRUB_SCHEDULE_ACCOUNT_COMMAND,
        &serde_json::json!({ "scheduleName": SCHEDULE, "accountId": UNAPPROVED }).to_string(),
    );
    let parsed: serde_json::Value = serde_json::from_str(&refused).expect("a JSON reply");
    assert_eq!(parsed["ok"], serde_json::json!(false));
    assert_eq!(
        parsed["errorCode"],
        serde_json::json!(ACCOUNT_NOT_APPROVED_FOR_AUTOSCRUB)
    );
    assert_eq!(parsed["accountId"], serde_json::json!(UNAPPROVED));

    let after =
        run_autoscrub_schedule_account_command(&mut surface, AUTOSCRUB_SCHEDULE_ROWS_COMMAND, "{}");
    assert!(after.contains("\"rowCount\":1"), "{after}");
    assert!(after.contains(APPROVED), "{after}");
    assert!(
        !after.contains(UNAPPROVED),
        "the listing must not carry the unapproved account: {after}"
    );
    assert_eq!(
        surface.accounts_named_by(SCHEDULE),
        vec![APPROVED.to_string()]
    );
}

/// Saving a *new* schedule for the unapproved account is the other half of the
/// same bypass: refusing only the edit would leave the front door open.
#[test]
fn a_fresh_save_for_an_unapproved_account_is_refused_too() {
    let mut surface = surface();
    let refused = surface
        .save_schedule("pine-daily", UNAPPROVED, FIXTURE_CADENCE)
        .expect_err("an unapproved account may not be scheduled at all");
    assert_eq!(refused.reason, ACCOUNT_NOT_APPROVED_FOR_AUTOSCRUB);
    assert_eq!(refused.command, AUTOSCRUB_SCHEDULE_COMMAND);
    assert_eq!(
        surface.schedule_row_count(),
        0,
        "a refused save writes no row"
    );
    assert!(surface.schedule_row("pine-daily").is_none());
}

/// Matching is byte-exact: a one-character-off id, a case change and a stray
/// space are all "not approved", and none of them moves the row.
#[test]
fn near_miss_account_ids_are_refused_with_the_approval_reason() {
    let mut surface = surface();
    surface
        .save_schedule(SCHEDULE, APPROVED, FIXTURE_CADENCE)
        .expect("approved account schedules");
    let row_before = surface.schedule_row(SCHEDULE).expect("saved").clone();

    for near_miss in [
        "discord-pine",
        "discord-mapl",
        "discord-maple-2",
        "telegram-pine",
        "DISCORD-MAPLE",
        "discord maple",
        "discord-maple ",
        " discord-maple",
        "",
    ] {
        let refused = surface
            .set_schedule_account(SCHEDULE, near_miss)
            .expect_err("a near-miss id is not the approval");
        assert_eq!(
            refused.reason, ACCOUNT_NOT_APPROVED_FOR_AUTOSCRUB,
            "{near_miss:?} must be refused as not approved"
        );
        assert_eq!(refused.account_id, near_miss);
        assert_eq!(
            surface.schedule_row(SCHEDULE).expect("still saved"),
            &row_before,
            "{near_miss:?} must not move the row"
        );
    }
    assert_eq!(surface.schedule_row_count(), 1);
    assert_eq!(
        surface.accounts_named_by(SCHEDULE),
        vec![APPROVED.to_string()]
    );
}

/// One check, not a second one: what the switch refuses the schedule refuses,
/// account for account, so the two cannot drift apart.
#[test]
fn the_schedule_and_the_switch_answer_the_same_for_every_account() {
    let surface = surface();
    let mut switches = AutoScrubAccountSwitchSurface::new(
        {
            let mut pro = AutoScrubProSurface::new(fixture_pro_code_directory());
            pro.present_pro_code(ipc::autoscrub_pro_gate::FIXTURE_ACTIVE_PRO_CODE);
            pro
        },
        fixture_autoscrub_accounts(),
    );
    let available = fixture_available_account_ids();
    let available_refs: Vec<&str> = available.iter().map(String::as_str).collect();
    switches
        .save_scrub_account_permissions(ScrubAccountPermissionInput::new(
            &available_refs,
            &[APPROVED],
        ))
        .expect("consent saves");

    for account_id in ["discord-maple", "discord-pine", "telegram-pine", "nope", ""] {
        assert_eq!(
            surface.account_schedulable(account_id),
            switches.switch_available(account_id),
            "{account_id} must get the same answer from the schedule and the switch"
        );
    }
}

/// Withdrawing the approval takes the ability away again — approval is a live
/// condition, not a one-time stamp. The already-saved row is what it is; what
/// the withdrawal governs is whether it may be *named* again.
#[test]
fn withdrawing_the_approval_closes_the_schedule_again() {
    let mut surface = surface();
    surface
        .save_schedule(SCHEDULE, APPROVED, FIXTURE_CADENCE)
        .expect("approved account schedules");
    assert!(surface.account_schedulable(APPROVED));

    let available = fixture_available_account_ids();
    let available_refs: Vec<&str> = available.iter().map(String::as_str).collect();
    surface
        .save_scrub_account_permissions(ScrubAccountPermissionInput::new(&available_refs, &[]))
        .expect("consent saves");

    assert!(!surface.account_schedulable(APPROVED));
    let refused = surface
        .set_schedule_account(SCHEDULE, APPROVED)
        .expect_err("a withdrawn approval refuses");
    assert_eq!(refused.reason, ACCOUNT_NOT_APPROVED_FOR_AUTOSCRUB);
}

/// Approving `discord-pine` through the normal Scrub consent is what it takes —
/// the refusal is the missing approval, not a hard-coded blocklist. Without this
/// the whole check would pass with the account name simply ignored.
#[test]
fn approving_the_sibling_is_what_makes_the_edit_go_through() {
    let mut surface = surface();
    surface
        .save_schedule(SCHEDULE, APPROVED, FIXTURE_CADENCE)
        .expect("approved account schedules");
    assert!(surface.set_schedule_account(SCHEDULE, UNAPPROVED).is_err());

    let available = fixture_available_account_ids();
    let available_refs: Vec<&str> = available.iter().map(String::as_str).collect();
    surface
        .save_scrub_account_permissions(ScrubAccountPermissionInput::new(
            &available_refs,
            &[APPROVED, UNAPPROVED],
        ))
        .expect("consent saves");

    let outcome = surface
        .set_schedule_account(SCHEDULE, UNAPPROVED)
        .expect("an approved sibling may be named");
    assert_eq!(outcome.saved.account_id, UNAPPROVED);
    assert_eq!(outcome.saved.schedule_name, SCHEDULE);
    assert_eq!(
        outcome.saved.cadence, FIXTURE_CADENCE,
        "only the account moved"
    );
    assert_eq!(outcome.row_count, 1, "an edit replaces, it does not add");
}

/// The Task 1454 Pro gate stays in front: a locked gate refuses before the
/// approval question is asked, and still writes nothing.
#[test]
fn a_locked_pro_gate_refuses_first() {
    let mut surface = AutoScrubScheduleSurface::new(
        AutoScrubProSurface::new(fixture_pro_code_directory()),
        fixture_autoscrub_accounts(),
    );
    let available = fixture_available_account_ids();
    let available_refs: Vec<&str> = available.iter().map(String::as_str).collect();
    surface
        .save_scrub_account_permissions(ScrubAccountPermissionInput::new(
            &available_refs,
            &[APPROVED],
        ))
        .expect("consent saves");

    let refused = surface
        .save_schedule(SCHEDULE, APPROVED, FIXTURE_CADENCE)
        .expect_err("a locked gate refuses");
    assert_eq!(refused.reason, PRO_CODE_REQUIRED);
    assert_eq!(surface.schedule_row_count(), 0);
}

/// An account nobody is signed in to is refused as unknown, and a malformed
/// schedule name never reaches the row vector.
#[test]
fn unknown_accounts_and_malformed_schedule_names_are_refused() {
    let mut surface = AutoScrubScheduleSurface::new(
        {
            let mut pro = AutoScrubProSurface::new(fixture_pro_code_directory());
            pro.present_pro_code(ipc::autoscrub_pro_gate::FIXTURE_ACTIVE_PRO_CODE);
            pro
        },
        vec![AutoScrubAccount::new(APPROVED, "Discord app")],
    );
    surface
        .save_scrub_account_permissions(ScrubAccountPermissionInput::new(
            &[APPROVED, "discord-ghost"],
            &[APPROVED, "discord-ghost"],
        ))
        .expect("consent saves");

    // Approved by consent, but not a signed-in account AutoScrub can cover.
    let refused = surface
        .save_schedule("ghost-daily", "discord-ghost", FIXTURE_CADENCE)
        .expect_err("an account that is not signed in is refused");
    assert_eq!(refused.reason, UNKNOWN_ACCOUNT);
    assert_eq!(surface.schedule_row_count(), 0);

    let refused = surface
        .save_schedule("-bad-", APPROVED, FIXTURE_CADENCE)
        .expect_err("a malformed schedule name is refused");
    assert_eq!(refused.reason, INVALID_SCHEDULE_NAME);
    assert_eq!(surface.schedule_row_count(), 0);
}

/// Editing a schedule that was never saved is refused and writes nothing —
/// the edit path cannot be used to create a row behind the save path's back.
#[test]
fn editing_an_unsaved_schedule_creates_nothing() {
    let mut surface = surface();
    let refused = surface
        .set_schedule_account("pine-daily", APPROVED)
        .expect_err("an unsaved schedule cannot be edited");
    assert_eq!(refused.reason, UNKNOWN_SCHEDULE);
    assert_eq!(surface.schedule_row_count(), 0);
    assert!(surface.schedule_row("pine-daily").is_none());
}

/// A command this module does not own is named back rather than silently
/// treated as one of its own.
#[test]
fn an_unowned_command_is_refused_by_name() {
    let mut surface = surface();
    let reply = run_autoscrub_schedule_account_command(&mut surface, "autoscrub_anything", "{}");
    assert!(reply.contains(UNKNOWN_COMMAND), "{reply}");
    assert!(reply.contains("\"ok\":false"), "{reply}");
    assert_eq!(surface.schedule_row_count(), 0);
}
