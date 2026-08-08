//! TASK 1457 - break AutoScrub new account bypass.
//!
//! The differential: add the **approved** account `discord-maple` to the
//! schedule `maple-daily`, then change **only the account name** to the
//! **unapproved** `discord-pine` and try again.
//!
//! Finish line:
//!
//! - the schedule-row count is 0 before and 1 after `maple-daily` names
//!   `discord-maple`;
//! - `discord-pine` is refused as account not approved for AutoScrub;
//! - `maple-daily` still names only `discord-maple` and the count stays 1.
//!
//! Every one of those is read off a value this file actually produced, and the
//! two requests are proved to differ in the `account` field alone — otherwise
//! "change only the account name" would be a claim rather than a measurement.

use ipc::autoscrub_account_switches::{
    fixture_autoscrub_accounts, fixture_available_account_ids, ScrubAccountPermissionInput,
    ACCOUNT_NOT_APPROVED_FOR_AUTOSCRUB,
};
use ipc::autoscrub_pro_gate::{
    fixture_pro_code_directory, AutoScrubProSurface, AutoScrubScheduleRequest,
    AUTOSCRUB_SCHEDULE_COMMAND, FIXTURE_ACTIVE_PRO_CODE,
};
use ipc::autoscrub_schedule_accounts::{
    run_autoscrub_schedule_command, task_1457_schedule_request, AutoScrubScheduleSurface,
    AUTOSCRUB_SCHEDULES_COMMAND, TASK_1457_APPROVED_ACCOUNT, TASK_1457_SCHEDULE_CADENCE,
    TASK_1457_SCHEDULE_NAME, TASK_1457_UNAPPROVED_ACCOUNT,
};
use serde_json::Value;

/// A surface with the Pro gate held open by the exact active fixture code, so
/// what these runs measure is the *account approval* alone and never the Pro
/// gate standing in for it.
fn unlocked_surface() -> AutoScrubScheduleSurface {
    let mut surface = AutoScrubScheduleSurface::new(
        AutoScrubProSurface::new(fixture_pro_code_directory()),
        fixture_autoscrub_accounts(),
    );
    surface.pro_mut().present_pro_code(FIXTURE_ACTIVE_PRO_CODE);
    assert!(
        surface.pro().pro_unlocked(),
        "the fixture Pro code must hold the gate open"
    );
    surface
}

/// Walk the normal Scrub account step: every fixture account is on offer, and
/// only `discord-maple` is ticked. `discord-pine` is offered and *not* ticked,
/// so it is unapproved because the person did not tick it — not because
/// AutoScrub never heard of it.
fn approve_only_maple(surface: &mut AutoScrubScheduleSurface) {
    let available = fixture_available_account_ids();
    let available_refs: Vec<&str> = available.iter().map(String::as_str).collect();
    let read = surface
        .save_scrub_account_permissions(ScrubAccountPermissionInput::new(
            &available_refs,
            &[TASK_1457_APPROVED_ACCOUNT],
        ))
        .expect("the normal Scrub consent step saves");
    assert_eq!(
        read.account_ids,
        vec![TASK_1457_APPROVED_ACCOUNT.to_string()],
        "only discord-maple is approved"
    );
    assert!(
        surface.account_schedulable(TASK_1457_APPROVED_ACCOUNT),
        "discord-maple is approved"
    );
    assert!(
        !surface.account_schedulable(TASK_1457_UNAPPROVED_ACCOUNT),
        "discord-pine is not approved"
    );
    println!(
        "TASK1457_APPROVED={:?} OFFERED={:?}",
        read.account_ids, available
    );
}

fn parse(reply: &str) -> Value {
    serde_json::from_str(reply).expect("every reply is a JSON object")
}

/// The good request and the bad copy, with the bad copy built from the good one
/// by changing the `account` field and nothing else.
fn good_and_bad_requests() -> (String, String) {
    let good = task_1457_schedule_request(TASK_1457_APPROVED_ACCOUNT);
    let mut bad = good.clone();
    bad["account"] = Value::String(TASK_1457_UNAPPROVED_ACCOUNT.to_string());

    let good_map = good.as_object().expect("object").clone();
    let bad_map = bad.as_object().expect("object").clone();
    assert_eq!(
        good_map.len(),
        bad_map.len(),
        "the bad copy has the same fields"
    );
    let differing: Vec<&String> = good_map
        .keys()
        .filter(|key| good_map.get(*key) != bad_map.get(*key))
        .collect();
    assert_eq!(
        differing,
        vec![&"account".to_string()],
        "the copy differs in the account name and nothing else"
    );
    println!("TASK1457_GOOD_REQUEST={good}");
    println!("TASK1457_BAD_REQUEST={bad}");
    println!(
        "TASK1457_ONLY_CHANGED_FIELD={:?} FROM={} TO={}",
        differing, TASK_1457_APPROVED_ACCOUNT, TASK_1457_UNAPPROVED_ACCOUNT
    );
    (good.to_string(), bad.to_string())
}

#[test]
fn task_1457_the_schedule_row_count_is_zero_before_and_one_after_the_approved_account() {
    let mut surface = unlocked_surface();
    approve_only_maple(&mut surface);

    let before = surface.schedule_row_count();
    println!("TASK1457_SCHEDULE_ROW_COUNT_BEFORE={before}");
    assert_eq!(before, 0, "nothing is scheduled before the good save");

    let (good, _bad) = good_and_bad_requests();
    let reply = run_autoscrub_schedule_command(&mut surface, AUTOSCRUB_SCHEDULE_COMMAND, &good);
    println!("TASK1457_GOOD_SAVE_REPLY={reply}");
    let reply = parse(&reply);
    assert_eq!(reply["ok"], Value::Bool(true), "the approved account saves");
    assert_eq!(reply["result"]["scheduleCount"], 1);
    assert_eq!(
        reply["result"]["saved"]["scheduleName"],
        TASK_1457_SCHEDULE_NAME
    );
    assert_eq!(
        reply["result"]["saved"]["account"],
        TASK_1457_APPROVED_ACCOUNT
    );
    assert_eq!(
        reply["result"]["saved"]["cadence"],
        TASK_1457_SCHEDULE_CADENCE
    );

    let after = surface.schedule_row_count();
    println!(
        "TASK1457_SCHEDULE_ROW_COUNT_AFTER={after} NAMED_ACCOUNTS={:?}",
        surface.scheduled_account_ids()
    );
    assert_eq!(after, 1, "the good save leaves exactly one schedule row");
    assert_eq!(
        surface.accounts_named_by(TASK_1457_SCHEDULE_NAME),
        vec![TASK_1457_APPROVED_ACCOUNT.to_string()]
    );
}

#[test]
fn task_1457_the_unapproved_account_is_refused_by_name() {
    let mut surface = unlocked_surface();
    approve_only_maple(&mut surface);
    let (good, bad) = good_and_bad_requests();

    // The good save first, so the bad copy is refused against a surface that is
    // already holding the schedule it would rewrite.
    let good_reply = parse(&run_autoscrub_schedule_command(
        &mut surface,
        AUTOSCRUB_SCHEDULE_COMMAND,
        &good,
    ));
    assert_eq!(good_reply["ok"], Value::Bool(true));

    let reply = run_autoscrub_schedule_command(&mut surface, AUTOSCRUB_SCHEDULE_COMMAND, &bad);
    println!("TASK1457_BAD_SAVE_REPLY={reply}");
    let reply = parse(&reply);
    assert_eq!(reply["ok"], Value::Bool(false), "discord-pine is refused");
    assert_eq!(
        reply["errorCode"], ACCOUNT_NOT_APPROVED_FOR_AUTOSCRUB,
        "refused as account not approved for AutoScrub"
    );
    assert_eq!(reply["accountId"], TASK_1457_UNAPPROVED_ACCOUNT);
    assert_eq!(reply["command"], AUTOSCRUB_SCHEDULE_COMMAND);
    let message = reply["error"].as_str().expect("a message");
    assert!(
        message.contains(TASK_1457_UNAPPROVED_ACCOUNT) && message.contains("not approved"),
        "the refusal names the account and says why: {message}"
    );
    println!(
        "TASK1457_REFUSAL_CODE={} MESSAGE={message}",
        reply["errorCode"]
    );

    // And the same refusal on a clean copy that never held the good schedule,
    // so being refused is not an artefact of the name already being taken.
    let mut clean = unlocked_surface();
    approve_only_maple(&mut clean);
    let clean_reply = parse(&run_autoscrub_schedule_command(
        &mut clean,
        AUTOSCRUB_SCHEDULE_COMMAND,
        &bad,
    ));
    println!("TASK1457_CLEAN_COPY_BAD_SAVE_REPLY={clean_reply}");
    assert_eq!(clean_reply["ok"], Value::Bool(false));
    assert_eq!(clean_reply["errorCode"], ACCOUNT_NOT_APPROVED_FOR_AUTOSCRUB);
    println!(
        "TASK1457_CLEAN_COPY_ROW_COUNT={}",
        clean.schedule_row_count()
    );
    assert_eq!(
        clean.schedule_row_count(),
        0,
        "a refused save on a clean copy writes nothing"
    );
}

#[test]
fn task_1457_maple_daily_still_names_only_discord_maple_and_the_count_stays_one() {
    let mut surface = unlocked_surface();
    approve_only_maple(&mut surface);
    let (good, bad) = good_and_bad_requests();

    assert_eq!(surface.schedule_row_count(), 0);
    let good_reply = parse(&run_autoscrub_schedule_command(
        &mut surface,
        AUTOSCRUB_SCHEDULE_COMMAND,
        &good,
    ));
    assert_eq!(good_reply["ok"], Value::Bool(true));
    let listing_before =
        run_autoscrub_schedule_command(&mut surface, AUTOSCRUB_SCHEDULES_COMMAND, "{}");
    println!("TASK1457_LISTING_AFTER_GOOD_SAVE={listing_before}");

    // The bad copy, three times, through the direct-invoke surface and through
    // the typed one, so nothing about the route changes the answer.
    for attempt in 1..=3 {
        let reply = parse(&run_autoscrub_schedule_command(
            &mut surface,
            AUTOSCRUB_SCHEDULE_COMMAND,
            &bad,
        ));
        assert_eq!(reply["ok"], Value::Bool(false), "attempt {attempt}");
        assert_eq!(reply["errorCode"], ACCOUNT_NOT_APPROVED_FOR_AUTOSCRUB);
    }
    let typed: AutoScrubScheduleRequest = serde_json::from_str(&bad).expect("request");
    let refusal = surface
        .save_schedule(typed)
        .expect_err("the typed call is refused too");
    assert_eq!(refusal.reason, ACCOUNT_NOT_APPROVED_FOR_AUTOSCRUB);

    let listing_after =
        run_autoscrub_schedule_command(&mut surface, AUTOSCRUB_SCHEDULES_COMMAND, "{}");
    println!("TASK1457_LISTING_AFTER_BAD_SAVES={listing_after}");
    assert_eq!(
        listing_before, listing_after,
        "a refused save leaves the schedule rows byte-for-byte as they were"
    );

    let named = surface.accounts_named_by(TASK_1457_SCHEDULE_NAME);
    let count = surface.schedule_row_count();
    println!("TASK1457_MAPLE_DAILY_NAMES={named:?}");
    println!("TASK1457_SCHEDULE_ROW_COUNT_STAYS={count}");
    assert_eq!(
        named,
        vec![TASK_1457_APPROVED_ACCOUNT.to_string()],
        "maple-daily still names only discord-maple"
    );
    assert_eq!(count, 1, "the count stays 1");
    assert!(
        !surface
            .scheduled_account_ids()
            .contains(&TASK_1457_UNAPPROVED_ACCOUNT.to_string()),
        "no schedule row anywhere names discord-pine"
    );
}

#[test]
fn task_1457_approving_discord_pine_is_the_only_thing_that_lets_it_be_scheduled() {
    let mut surface = unlocked_surface();
    approve_only_maple(&mut surface);
    let (_good, bad) = good_and_bad_requests();

    // Every near-miss that is *not* approving discord-pine leaves it refused.
    let near_misses: [(&str, Vec<&str>); 3] = [
        (
            "telegram-pine approved instead",
            vec!["discord-maple", "telegram-pine"],
        ),
        ("nothing approved", vec![]),
        ("only the sibling app account", vec!["telegram-pine"]),
    ];
    for (label, selected) in near_misses {
        let available = fixture_available_account_ids();
        let available_refs: Vec<&str> = available.iter().map(String::as_str).collect();
        surface
            .save_scrub_account_permissions(ScrubAccountPermissionInput::new(
                &available_refs,
                &selected,
            ))
            .expect("consent save");
        let reply = parse(&run_autoscrub_schedule_command(
            &mut surface,
            AUTOSCRUB_SCHEDULE_COMMAND,
            &bad,
        ));
        println!(
            "TASK1457_NEAR_MISS case={label:?} approved={selected:?} ok={} code={}",
            reply["ok"], reply["errorCode"]
        );
        assert_eq!(reply["ok"], Value::Bool(false), "{label}");
        assert_eq!(
            reply["errorCode"], ACCOUNT_NOT_APPROVED_FOR_AUTOSCRUB,
            "{label}"
        );
    }

    // Ticking discord-pine in the normal Scrub account list — and only that —
    // lets the very same request through. The check is the approval, not a
    // blocklist on the string "discord-pine".
    let available = fixture_available_account_ids();
    let available_refs: Vec<&str> = available.iter().map(String::as_str).collect();
    surface
        .save_scrub_account_permissions(ScrubAccountPermissionInput::new(
            &available_refs,
            &[TASK_1457_UNAPPROVED_ACCOUNT],
        ))
        .expect("consent save");
    let reply = parse(&run_autoscrub_schedule_command(
        &mut surface,
        AUTOSCRUB_SCHEDULE_COMMAND,
        &bad,
    ));
    println!("TASK1457_AFTER_APPROVING_PINE={reply}");
    assert_eq!(
        reply["ok"],
        Value::Bool(true),
        "the approval is what decides"
    );
    assert_eq!(
        reply["result"]["saved"]["account"],
        TASK_1457_UNAPPROVED_ACCOUNT
    );
}
