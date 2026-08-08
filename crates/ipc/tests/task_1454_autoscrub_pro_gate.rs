//! TASK 1454 check: AutoScrub setup, schedules, deletion, and activity history
//! require an active Pro code.
//!
//! Finish line, item by item:
//!
//! 1. Free AutoScrub setup, schedule, deletion, and activity commands are
//!    refused **by name** before an active fixture Pro code.
//! 2. A direct invoke (naming the command, skipping the control listing) is
//!    refused.
//! 3. The same controls become available after the **exact** active fixture
//!    code.
//! 4. **Only** that code changes them.

use ipc::autoscrub_pro_gate::{
    classify_pro_code, fixture_pro_code_directory, run_autoscrub_pro_command,
    AutoScrubDeletionRequest, AutoScrubProSurface, AutoScrubScheduleRequest, AutoScrubSetupRequest,
    ProCodeStatus, ProCodeVerdict, AUTOSCRUB_ACTIVITY_COMMAND, AUTOSCRUB_DELETION_COMMAND,
    AUTOSCRUB_PRO_COMMANDS, AUTOSCRUB_SCHEDULE_COMMAND, AUTOSCRUB_SETUP_COMMAND,
    FIXTURE_ACTIVE_PRO_CODE, FIXTURE_EXPIRED_PRO_CODE, FIXTURE_REVOKED_PRO_CODE, PRO_CODE_REQUIRED,
};

/// Every string that is not the exact active fixture code but is close enough
/// that a sloppy gate would let it through.
const NEAR_MISS_CODES: [(&str, ProCodeVerdict); 8] = [
    (FIXTURE_EXPIRED_PRO_CODE, ProCodeVerdict::Expired),
    (FIXTURE_REVOKED_PRO_CODE, ProCodeVerdict::Revoked),
    ("OSL-1454-AUTO-SCRB-PRO2", ProCodeVerdict::NotRecognized),
    ("OSL-1454-AUTO-SCRB-PR01", ProCodeVerdict::NotRecognized),
    ("osl-1454-auto-scrb-pro1", ProCodeVerdict::Malformed),
    ("OSL-1454-AUTO-SCRB-PRO1 ", ProCodeVerdict::Malformed),
    (" OSL-1454-AUTO-SCRB-PRO1", ProCodeVerdict::Malformed),
    ("", ProCodeVerdict::Missing),
];

fn locked_surface() -> AutoScrubProSurface {
    AutoScrubProSurface::new(fixture_pro_code_directory())
}

fn setup_request() -> AutoScrubSetupRequest {
    serde_json::from_str(r#"{"accounts":["discord-maple","telegram-pine"]}"#)
        .expect("setup fixture parses")
}

fn schedule_request() -> AutoScrubScheduleRequest {
    serde_json::from_str(
        r#"{"scheduleName":"maple-daily","account":"discord-maple","cadence":"daily"}"#,
    )
    .expect("schedule fixture parses")
}

fn deletion_request() -> AutoScrubDeletionRequest {
    serde_json::from_str(
        r#"{"account":"discord-maple","markedLocators":["marked-1","marked-2","marked-3"]}"#,
    )
    .expect("deletion fixture parses")
}

fn request_json_for(command: &str) -> &'static str {
    match command {
        AUTOSCRUB_SETUP_COMMAND => r#"{"accounts":["discord-maple","telegram-pine"]}"#,
        AUTOSCRUB_SCHEDULE_COMMAND => {
            r#"{"scheduleName":"maple-daily","account":"discord-maple","cadence":"daily"}"#
        }
        AUTOSCRUB_DELETION_COMMAND => {
            r#"{"account":"discord-maple","markedLocators":["marked-1","marked-2","marked-3"]}"#
        }
        _ => "{}",
    }
}

// ---- 1. Free is refused by name, on all four commands ----

#[test]
fn task_1454_free_refuses_all_four_autoscrub_commands_by_name() {
    let mut surface = locked_surface();

    assert!(!surface.pro_unlocked(), "a new surface holds no Pro code");
    assert_eq!(surface.available_commands(), Vec::<String>::new());
    assert_eq!(
        surface.refused_commands(),
        vec![
            AUTOSCRUB_SETUP_COMMAND.to_string(),
            AUTOSCRUB_SCHEDULE_COMMAND.to_string(),
            AUTOSCRUB_DELETION_COMMAND.to_string(),
            AUTOSCRUB_ACTIVITY_COMMAND.to_string(),
        ],
        "all four AutoScrub controls are locked for Free"
    );

    for control in surface.controls() {
        let refusal = control
            .refusal
            .as_ref()
            .expect("a locked control carries its refusal");
        assert!(!control.available);
        assert_eq!(
            refusal.command, control.command,
            "the refusal names the command it refused"
        );
        assert_eq!(refusal.reason, PRO_CODE_REQUIRED);
        println!(
            "TASK1454_FREE_REFUSED command={} label={} reason={} message={}",
            control.command, control.label, refusal.reason, refusal.message
        );
    }

    // Each typed command refuses itself, by its own name.
    let setup = surface.setup(setup_request()).expect_err("free setup");
    assert_eq!(setup.command, AUTOSCRUB_SETUP_COMMAND);
    assert_eq!(setup.reason, PRO_CODE_REQUIRED);

    let schedule = surface
        .schedule(schedule_request())
        .expect_err("free schedule");
    assert_eq!(schedule.command, AUTOSCRUB_SCHEDULE_COMMAND);
    assert_eq!(schedule.reason, PRO_CODE_REQUIRED);

    let deletion = surface
        .deletion(deletion_request())
        .expect_err("free deletion");
    assert_eq!(deletion.command, AUTOSCRUB_DELETION_COMMAND);
    assert_eq!(deletion.reason, PRO_CODE_REQUIRED);

    let activity = surface.activity().expect_err("free activity history");
    assert_eq!(activity.command, AUTOSCRUB_ACTIVITY_COMMAND);
    assert_eq!(activity.reason, PRO_CODE_REQUIRED);
}

#[test]
fn task_1454_a_refused_free_command_changes_nothing() {
    let mut surface = locked_surface();

    assert!(surface.setup(setup_request()).is_err());
    assert!(surface.schedule(schedule_request()).is_err());
    assert!(surface.deletion(deletion_request()).is_err());
    assert!(surface.activity().is_err());
    assert_eq!(surface.schedules().len(), 0);

    // Unlock and read the history: if any refused Free call had run, it would
    // be recorded here.
    surface.present_pro_code(FIXTURE_ACTIVE_PRO_CODE);
    let history = surface.activity().expect("pro reads activity history");
    assert_eq!(
        history.entry_count, 0,
        "a refused Free command left no activity"
    );
    assert_eq!(surface.schedules().len(), 0, "no schedule was saved");
    println!(
        "TASK1454_FREE_LEFT_NO_TRACE schedules={} activity_entries={}",
        surface.schedules().len(),
        history.entry_count
    );
}

// ---- 2. A direct invoke is refused ----

#[test]
fn task_1454_a_direct_invoke_is_refused_for_every_command() {
    let mut surface = locked_surface();

    for command in AUTOSCRUB_PRO_COMMANDS {
        let reply = run_autoscrub_pro_command(&mut surface, command, request_json_for(command));
        let parsed: serde_json::Value = serde_json::from_str(&reply).expect("json reply");
        assert_eq!(parsed["ok"], false, "{command} direct invoke must refuse");
        assert_eq!(parsed["command"], command, "the refusal names the command");
        assert_eq!(parsed["errorCode"], PRO_CODE_REQUIRED);
        assert!(
            parsed["result"].is_null(),
            "{command} refusal carries no result"
        );
        println!("TASK1454_DIRECT_INVOKE_REFUSED {command} reply={reply}");
    }

    // The direct invoke did not smuggle any state past the gate either.
    surface.present_pro_code(FIXTURE_ACTIVE_PRO_CODE);
    assert_eq!(surface.schedules().len(), 0);
    assert_eq!(
        surface.activity().expect("pro history").entry_count,
        0,
        "direct invokes wrote no activity"
    );
}

// ---- 3. The exact active fixture code makes the same controls available ----

#[test]
fn task_1454_the_exact_active_fixture_code_opens_the_same_four_controls() {
    let mut surface = locked_surface();
    assert_eq!(surface.available_commands().len(), 0);

    let verdict = surface.present_pro_code(FIXTURE_ACTIVE_PRO_CODE);
    assert_eq!(verdict, ProCodeVerdict::Active);
    assert!(surface.pro_unlocked());
    assert_eq!(surface.active_code(), Some(FIXTURE_ACTIVE_PRO_CODE));
    assert_eq!(
        surface.available_commands(),
        vec![
            AUTOSCRUB_SETUP_COMMAND.to_string(),
            AUTOSCRUB_SCHEDULE_COMMAND.to_string(),
            AUTOSCRUB_DELETION_COMMAND.to_string(),
            AUTOSCRUB_ACTIVITY_COMMAND.to_string(),
        ]
    );
    assert_eq!(surface.refused_commands().len(), 0);
    for control in surface.controls() {
        assert!(control.available);
        assert!(control.refusal.is_none());
    }
    println!(
        "TASK1454_UNLOCKED code={} available={:?}",
        FIXTURE_ACTIVE_PRO_CODE,
        surface.available_commands()
    );

    // The commands do not just report available; they run.
    let setup = surface.setup(setup_request()).expect("pro setup");
    assert_eq!(setup.account_count, 2);

    let schedule = surface.schedule(schedule_request()).expect("pro schedule");
    assert_eq!(schedule.schedule_count, 1);
    assert_eq!(schedule.saved.schedule_name, "maple-daily");
    assert_eq!(schedule.saved.account, "discord-maple");

    let deletion = surface.deletion(deletion_request()).expect("pro deletion");
    assert_eq!(deletion.deleted_count, 3);
    assert_eq!(deletion.account, "discord-maple");

    let history = surface.activity().expect("pro activity history");
    assert_eq!(history.entry_count, 3);
    assert_eq!(
        history
            .entries
            .iter()
            .map(|entry| entry.command.as_str())
            .collect::<Vec<_>>(),
        vec![
            AUTOSCRUB_SETUP_COMMAND,
            AUTOSCRUB_SCHEDULE_COMMAND,
            AUTOSCRUB_DELETION_COMMAND
        ]
    );
    println!(
        "TASK1454_PRO_RAN setup_accounts={} schedules={} deleted={} activity_entries={}",
        setup.account_count, schedule.schedule_count, deletion.deleted_count, history.entry_count
    );
}

#[test]
fn task_1454_a_direct_invoke_is_accepted_after_the_exact_code() {
    let mut surface = locked_surface();
    surface.present_pro_code(FIXTURE_ACTIVE_PRO_CODE);

    for command in AUTOSCRUB_PRO_COMMANDS {
        let reply = run_autoscrub_pro_command(&mut surface, command, request_json_for(command));
        let parsed: serde_json::Value = serde_json::from_str(&reply).expect("json reply");
        assert_eq!(parsed["ok"], true, "{command} must run for an active code");
        assert_eq!(parsed["command"], command);
        assert!(parsed["result"].is_object());
        println!("TASK1454_DIRECT_INVOKE_ACCEPTED {command} reply={reply}");
    }
}

// ---- 4. Only that code changes them ----

#[test]
fn task_1454_only_the_exact_active_code_changes_what_is_available() {
    let directory = fixture_pro_code_directory();

    for (candidate, expected) in NEAR_MISS_CODES {
        // From locked: no near miss opens anything.
        assert_eq!(
            classify_pro_code(&directory, candidate),
            expected,
            "verdict for {candidate:?}"
        );
        assert!(
            !expected.unlocks(),
            "{candidate:?} must not be an unlocking verdict"
        );

        let mut surface = locked_surface();
        let verdict = surface.present_pro_code(candidate);
        assert_eq!(verdict, expected);
        assert!(!surface.pro_unlocked(), "{candidate:?} must not unlock");
        assert_eq!(surface.active_code(), None);
        assert_eq!(
            surface.refused_commands().len(),
            4,
            "{candidate:?} left all four controls refused"
        );
        assert!(surface.setup(setup_request()).is_err());
        assert!(surface.activity().is_err());
        println!(
            "TASK1454_REJECTED_CODE code={candidate:?} verdict={} unlocked={} refused={}",
            verdict.as_str(),
            surface.pro_unlocked(),
            surface.refused_commands().len()
        );

        // From unlocked: no near miss takes the unlock away either. Only the
        // exact code governs the gate.
        let mut unlocked = locked_surface();
        unlocked.present_pro_code(FIXTURE_ACTIVE_PRO_CODE);
        assert!(unlocked.pro_unlocked());
        unlocked.present_pro_code(candidate);
        assert!(
            unlocked.pro_unlocked(),
            "{candidate:?} must not disturb the active code"
        );
        assert_eq!(unlocked.active_code(), Some(FIXTURE_ACTIVE_PRO_CODE));
        assert_eq!(unlocked.available_commands().len(), 4);
    }

    println!(
        "TASK1454_REJECTED_CODE_COUNT={} ACCEPTED_CODE={}",
        NEAR_MISS_CODES.len(),
        FIXTURE_ACTIVE_PRO_CODE
    );
}

#[test]
fn task_1454_only_that_codes_own_status_relocks_the_controls() {
    let mut surface = locked_surface();
    surface.present_pro_code(FIXTURE_ACTIVE_PRO_CODE);
    assert_eq!(surface.available_commands().len(), 4);

    // Another code's status changing is not this code's business.
    let mut directory = fixture_pro_code_directory();
    assert!(directory.set_status(FIXTURE_EXPIRED_PRO_CODE, ProCodeStatus::Active));
    let mut other = AutoScrubProSurface::new(directory);
    other.present_pro_code(FIXTURE_ACTIVE_PRO_CODE);
    assert_eq!(
        other.available_commands().len(),
        4,
        "a second code turning active does not change this surface's own gate"
    );

    // The held code losing Active status relocks all four, immediately.
    let mut relocking = fixture_pro_code_directory();
    assert!(relocking.set_status(FIXTURE_ACTIVE_PRO_CODE, ProCodeStatus::Revoked));
    let mut relocked = AutoScrubProSurface::new(relocking);
    let verdict = relocked.present_pro_code(FIXTURE_ACTIVE_PRO_CODE);
    assert_eq!(verdict, ProCodeVerdict::Revoked);
    assert!(!relocked.pro_unlocked());
    assert_eq!(relocked.refused_commands().len(), 4);
    println!(
        "TASK1454_RELOCK revoked_verdict={} refused={}",
        verdict.as_str(),
        relocked.refused_commands().len()
    );
}
