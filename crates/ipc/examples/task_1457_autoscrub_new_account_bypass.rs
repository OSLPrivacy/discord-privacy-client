//! TASK 1457 direct invoke: break the AutoScrub new-account bypass.
//!
//! Runs the whole differential through the direct-invoke surface — the route a
//! caller reaches when it skips the schedule screen entirely — and prints every
//! number the finish line asks for. Exits 1 if any of them is not what the
//! finish line says, so this is a check and not a printout.
//!
//! ```sh
//! CARGO_TARGET_DIR=/mnt/d/osl-lane-targets/c \
//!   cargo run --locked --quiet -p ipc \
//!   --example task_1457_autoscrub_new_account_bypass
//! ```

use ipc::autoscrub_account_switches::{
    fixture_autoscrub_accounts, fixture_available_account_ids, ScrubAccountPermissionInput,
    ACCOUNT_NOT_APPROVED_FOR_AUTOSCRUB,
};
use ipc::autoscrub_pro_gate::{
    fixture_pro_code_directory, AutoScrubProSurface, AUTOSCRUB_SCHEDULE_COMMAND,
    FIXTURE_ACTIVE_PRO_CODE,
};
use ipc::autoscrub_schedule_accounts::{
    run_autoscrub_schedule_command, task_1457_schedule_request, AutoScrubScheduleSurface,
    AUTOSCRUB_SCHEDULES_COMMAND, TASK_1457_APPROVED_ACCOUNT, TASK_1457_SCHEDULE_NAME,
    TASK_1457_UNAPPROVED_ACCOUNT,
};
use serde_json::Value;

fn main() {
    let mut failures: Vec<String> = Vec::new();
    let mut check = |condition: bool, failure: String| {
        if !condition {
            println!("TASK1457_FAILURE={failure}");
            failures.push(failure);
        }
    };

    let mut surface = AutoScrubScheduleSurface::new(
        AutoScrubProSurface::new(fixture_pro_code_directory()),
        fixture_autoscrub_accounts(),
    );
    surface.pro_mut().present_pro_code(FIXTURE_ACTIVE_PRO_CODE);
    println!(
        "TASK1457_PRO_UNLOCKED={} CODE={FIXTURE_ACTIVE_PRO_CODE}",
        surface.pro().pro_unlocked()
    );
    check(
        surface.pro().pro_unlocked(),
        "the fixture Pro code did not hold the gate open".to_string(),
    );

    // The normal Scrub account step: everything on offer, only discord-maple
    // ticked. discord-pine is offered and left unticked.
    let available = fixture_available_account_ids();
    let available_refs: Vec<&str> = available.iter().map(String::as_str).collect();
    let read = surface
        .save_scrub_account_permissions(ScrubAccountPermissionInput::new(
            &available_refs,
            &[TASK_1457_APPROVED_ACCOUNT],
        ))
        .expect("the normal Scrub consent step saves");
    println!("TASK1457_OFFERED_ACCOUNTS={available:?}");
    println!("TASK1457_APPROVED_ACCOUNTS={:?}", read.account_ids);
    check(
        read.account_ids == vec![TASK_1457_APPROVED_ACCOUNT.to_string()],
        format!("approved set was {:?}", read.account_ids),
    );

    // The two requests: the bad one is the good one with the account name
    // changed and nothing else.
    let good = task_1457_schedule_request(TASK_1457_APPROVED_ACCOUNT);
    let mut bad = good.clone();
    bad["account"] = Value::String(TASK_1457_UNAPPROVED_ACCOUNT.to_string());
    let good_map = good.as_object().expect("object");
    let bad_map = bad.as_object().expect("object");
    let differing: Vec<String> = good_map
        .keys()
        .filter(|key| good_map.get(*key) != bad_map.get(*key))
        .cloned()
        .collect();
    println!("TASK1457_GOOD_REQUEST={good}");
    println!("TASK1457_BAD_REQUEST={bad}");
    println!("TASK1457_ONLY_CHANGED_FIELD={differing:?}");
    check(
        differing == vec!["account".to_string()] && good_map.len() == bad_map.len(),
        format!("the copy changed {differing:?}, not the account name alone"),
    );

    let before = surface.schedule_row_count();
    println!("TASK1457_SCHEDULE_ROW_COUNT_BEFORE={before}");
    check(before == 0, format!("row count before was {before}"));

    // The good save.
    let good_reply =
        run_autoscrub_schedule_command(&mut surface, AUTOSCRUB_SCHEDULE_COMMAND, &good.to_string());
    println!("TASK1457_GOOD_SAVE_REPLY={good_reply}");
    let good_reply: Value = serde_json::from_str(&good_reply).expect("json");
    check(
        good_reply["ok"] == Value::Bool(true),
        format!("the approved account was refused: {good_reply}"),
    );
    let after = surface.schedule_row_count();
    println!(
        "TASK1457_SCHEDULE_ROW_COUNT_AFTER={after} MAPLE_DAILY_NAMES={:?}",
        surface.accounts_named_by(TASK_1457_SCHEDULE_NAME)
    );
    check(
        after == 1,
        format!("row count after the good save was {after}"),
    );
    check(
        surface.accounts_named_by(TASK_1457_SCHEDULE_NAME)
            == vec![TASK_1457_APPROVED_ACCOUNT.to_string()],
        format!(
            "maple-daily named {:?} after the good save",
            surface.accounts_named_by(TASK_1457_SCHEDULE_NAME)
        ),
    );

    let listing_before =
        run_autoscrub_schedule_command(&mut surface, AUTOSCRUB_SCHEDULES_COMMAND, "{}");
    println!("TASK1457_LISTING_AFTER_GOOD_SAVE={listing_before}");

    // The bad copy, three times.
    let mut refused = 0;
    for _ in 0..3 {
        let reply = run_autoscrub_schedule_command(
            &mut surface,
            AUTOSCRUB_SCHEDULE_COMMAND,
            &bad.to_string(),
        );
        let parsed: Value = serde_json::from_str(&reply).expect("json");
        if parsed["ok"] == Value::Bool(false)
            && parsed["errorCode"] == ACCOUNT_NOT_APPROVED_FOR_AUTOSCRUB
        {
            refused += 1;
        }
        println!("TASK1457_BAD_SAVE_REPLY={reply}");
    }
    println!("TASK1457_BAD_SAVE_REFUSED_COUNT={refused}");
    check(
        refused == 3,
        format!("only {refused}/3 bad saves were refused"),
    );

    let listing_after =
        run_autoscrub_schedule_command(&mut surface, AUTOSCRUB_SCHEDULES_COMMAND, "{}");
    println!("TASK1457_LISTING_AFTER_BAD_SAVES={listing_after}");
    check(
        listing_before == listing_after,
        "the refused saves changed the schedule rows".to_string(),
    );

    let named = surface.accounts_named_by(TASK_1457_SCHEDULE_NAME);
    let stays = surface.schedule_row_count();
    println!("TASK1457_MAPLE_DAILY_NAMES={named:?}");
    println!("TASK1457_SCHEDULE_ROW_COUNT_STAYS={stays}");
    println!(
        "TASK1457_ALL_SCHEDULED_ACCOUNTS={:?}",
        surface.scheduled_account_ids()
    );
    check(
        named == vec![TASK_1457_APPROVED_ACCOUNT.to_string()],
        format!("maple-daily names {named:?}"),
    );
    check(stays == 1, format!("row count stayed at {stays}"));
    check(
        !surface
            .scheduled_account_ids()
            .contains(&TASK_1457_UNAPPROVED_ACCOUNT.to_string()),
        "a schedule row names discord-pine".to_string(),
    );

    // A clean copy that never held the good schedule refuses the same request,
    // so the refusal is the approval and not the name already being taken.
    let mut clean = AutoScrubScheduleSurface::new(
        AutoScrubProSurface::new(fixture_pro_code_directory()),
        fixture_autoscrub_accounts(),
    );
    clean.pro_mut().present_pro_code(FIXTURE_ACTIVE_PRO_CODE);
    clean
        .save_scrub_account_permissions(ScrubAccountPermissionInput::new(
            &available_refs,
            &[TASK_1457_APPROVED_ACCOUNT],
        ))
        .expect("consent save");
    let clean_reply =
        run_autoscrub_schedule_command(&mut clean, AUTOSCRUB_SCHEDULE_COMMAND, &bad.to_string());
    println!("TASK1457_CLEAN_COPY_BAD_SAVE_REPLY={clean_reply}");
    let clean_parsed: Value = serde_json::from_str(&clean_reply).expect("json");
    println!(
        "TASK1457_CLEAN_COPY_ROW_COUNT={}",
        clean.schedule_row_count()
    );
    check(
        clean_parsed["errorCode"] == ACCOUNT_NOT_APPROVED_FOR_AUTOSCRUB
            && clean.schedule_row_count() == 0,
        format!("the clean copy did not refuse: {clean_reply}"),
    );

    // Only the approval decides: tick discord-pine and the identical request is
    // accepted.
    clean
        .save_scrub_account_permissions(ScrubAccountPermissionInput::new(
            &available_refs,
            &[TASK_1457_UNAPPROVED_ACCOUNT],
        ))
        .expect("consent save");
    let pine_reply =
        run_autoscrub_schedule_command(&mut clean, AUTOSCRUB_SCHEDULE_COMMAND, &bad.to_string());
    println!("TASK1457_AFTER_APPROVING_PINE={pine_reply}");
    let pine_parsed: Value = serde_json::from_str(&pine_reply).expect("json");
    check(
        pine_parsed["ok"] == Value::Bool(true),
        format!("approving discord-pine did not let it through: {pine_reply}"),
    );

    if failures.is_empty() {
        println!("TASK1457_FINISH_LINE=met");
    } else {
        println!(
            "TASK1457_ERROR=finish line mismatch ({} item(s))",
            failures.len()
        );
        std::process::exit(1);
    }
}
