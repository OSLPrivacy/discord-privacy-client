//! Task 1455 check: AutoScrub account switches are allowed only for accounts
//! the normal Scrub consent has already approved.
//!
//! Finish line: the AutoScrub account switch is unavailable before the account
//! is approved by normal Scrub consent and a direct invoke for an unapproved
//! account is refused, turning it on after the exact account approval saves
//! exactly 1 on record, and only that approval changes it.

use ipc::autoscrub_account_switches::{
    fixture_autoscrub_accounts, fixture_available_account_ids, is_scrub_account_id,
    run_autoscrub_account_switch_command, AutoScrubAccountSwitchSurface, ScrubAccountConsent,
    ScrubAccountPermissionInput, ACCOUNT_NOT_APPROVED_FOR_AUTOSCRUB,
    AUTOSCRUB_ACCOUNT_SWITCHES_COMMAND, AUTOSCRUB_ACCOUNT_SWITCH_COMMAND,
};
use ipc::autoscrub_pro_gate::{
    fixture_pro_code_directory, AutoScrubProSurface, FIXTURE_ACTIVE_PRO_CODE, PRO_CODE_REQUIRED,
};

/// The account this task switches on. `discord-pine` is its near-miss sibling
/// and `telegram-pine` the third signed-in account, both unapproved.
const TARGET_ACCOUNT: &str = "discord-maple";

fn unlocked_surface() -> AutoScrubAccountSwitchSurface {
    let mut pro = AutoScrubProSurface::new(fixture_pro_code_directory());
    assert!(
        pro.present_pro_code(FIXTURE_ACTIVE_PRO_CODE).unlocks(),
        "the Task 1454 gate must be open so this check measures the Scrub approval, not Pro"
    );
    AutoScrubAccountSwitchSurface::new(pro, fixture_autoscrub_accounts())
}

fn available_ids() -> Vec<String> {
    fixture_available_account_ids()
}

/// Run the normal Scrub consent step over the fixture account list.
fn approve(surface: &mut AutoScrubAccountSwitchSurface, selected: &[&str]) -> Vec<String> {
    let available = available_ids();
    let available: Vec<&str> = available.iter().map(String::as_str).collect();
    surface
        .save_scrub_account_permissions(ScrubAccountPermissionInput::new(&available, selected))
        .expect("normal Scrub consent save")
        .account_ids
}

fn parse(reply: &str) -> serde_json::Value {
    serde_json::from_str(reply).expect("every reply is a JSON object")
}

#[test]
fn task_1455_the_switch_is_unavailable_before_normal_scrub_consent() {
    let surface = unlocked_surface();

    assert_eq!(
        surface.scrub_consent().approved_count(),
        0,
        "no account is approved yet"
    );

    let rows = surface.switches();
    assert_eq!(rows.len(), 3, "every signed-in account is listed");
    for row in &rows {
        let refusal = row
            .refusal
            .as_ref()
            .expect("an unapproved row carries its refusal");
        println!(
            "TASK1455_SWITCH_BEFORE_CONSENT account={} label={} available={} on={} reason={}",
            row.account_id, row.label, row.available, row.on, refusal.reason
        );
        assert!(!row.available, "{} must be shut", row.account_id);
        assert!(!row.on);
        assert_eq!(refusal.reason, ACCOUNT_NOT_APPROVED_FOR_AUTOSCRUB);
        assert_eq!(refusal.account_id, row.account_id);
    }

    let target = surface.switch_for(TARGET_ACCOUNT).expect("target row");
    println!(
        "TASK1455_TARGET_SWITCH_UNAVAILABLE account={} available={} message={}",
        target.account_id,
        target.available,
        target.refusal.expect("refusal").message
    );

    let available_count = rows.iter().filter(|row| row.available).count();
    println!("TASK1455_AVAILABLE_SWITCH_COUNT_BEFORE_CONSENT={available_count}");
    assert_eq!(available_count, 0);
    assert_eq!(surface.record_count(), 0);
}

#[test]
fn task_1455_a_direct_invoke_for_an_unapproved_account_is_refused() {
    let mut surface = unlocked_surface();

    for account_id in ["discord-maple", "discord-pine", "telegram-pine"] {
        let request = format!(r#"{{"accountId":"{account_id}","on":true}}"#);
        let reply = run_autoscrub_account_switch_command(
            &mut surface,
            AUTOSCRUB_ACCOUNT_SWITCH_COMMAND,
            &request,
        );
        println!("TASK1455_DIRECT_INVOKE_REFUSED account={account_id} reply={reply}");
        let reply = parse(&reply);
        assert_eq!(reply["ok"], serde_json::Value::Bool(false));
        assert_eq!(reply["command"], AUTOSCRUB_ACCOUNT_SWITCH_COMMAND);
        assert_eq!(reply["accountId"], account_id);
        assert_eq!(reply["errorCode"], ACCOUNT_NOT_APPROVED_FOR_AUTOSCRUB);
    }

    println!(
        "TASK1455_DIRECT_INVOKE_LEFT_RECORDS={} switched_on={:?}",
        surface.record_count(),
        surface.switched_on_account_ids()
    );
    assert_eq!(
        surface.record_count(),
        0,
        "a refused direct invoke writes nothing"
    );
}

#[test]
fn task_1455_the_exact_account_approval_saves_exactly_one_record() {
    let mut surface = unlocked_surface();
    assert_eq!(surface.record_count(), 0);

    let approved = approve(&mut surface, &[TARGET_ACCOUNT]);
    println!(
        "TASK1455_SCRUB_CONSENT available={:?} selected=[{}] approved={:?}",
        available_ids(),
        TARGET_ACCOUNT,
        approved
    );
    assert_eq!(approved, vec![TARGET_ACCOUNT.to_string()]);

    let row = surface.switch_for(TARGET_ACCOUNT).expect("target row");
    println!(
        "TASK1455_SWITCH_AFTER_CONSENT account={} available={} on={}",
        row.account_id, row.available, row.on
    );
    assert!(
        row.available,
        "the approved account's switch is now movable"
    );
    assert!(!row.on, "approval alone does not switch AutoScrub on");
    assert_eq!(surface.record_count(), 0);

    let outcome = surface
        .set_switch(TARGET_ACCOUNT, true)
        .expect("the approved switch may be turned on");
    println!(
        "TASK1455_SWITCH_ON account={} record_count={} records={:?}",
        outcome.account_id,
        outcome.record_count,
        surface.switched_on_account_ids()
    );
    assert_eq!(outcome.record_count, 1, "exactly 1 on record is saved");
    assert_eq!(surface.record_count(), 1);
    assert_eq!(surface.records().len(), 1);
    assert_eq!(surface.records()[0].account_id, TARGET_ACCOUNT);
    assert_eq!(surface.records()[0].label, "Discord app");
    assert!(surface.records()[0].on);

    // Only that account is on; the other two approved-nothing accounts stay off.
    let on_rows: Vec<String> = surface
        .switches()
        .into_iter()
        .filter(|row| row.on)
        .map(|row| row.account_id)
        .collect();
    println!("TASK1455_SWITCHED_ON_ROWS={on_rows:?}");
    assert_eq!(on_rows, vec![TARGET_ACCOUNT.to_string()]);

    // Turning the same switch on twice is still exactly 1 record.
    let again = surface
        .set_switch(TARGET_ACCOUNT, true)
        .expect("idempotent");
    println!(
        "TASK1455_SWITCH_ON_AGAIN record_count={}",
        again.record_count
    );
    assert_eq!(again.record_count, 1);

    let reply = run_autoscrub_account_switch_command(
        &mut surface,
        AUTOSCRUB_ACCOUNT_SWITCHES_COMMAND,
        "{}",
    );
    println!("TASK1455_SWITCH_LISTING={reply}");
    let reply = parse(&reply);
    assert_eq!(reply["ok"], serde_json::Value::Bool(true));
    assert_eq!(reply["result"]["recordCount"], 1);
}

#[test]
fn task_1455_a_direct_invoke_is_accepted_after_the_exact_approval() {
    let mut surface = unlocked_surface();
    approve(&mut surface, &[TARGET_ACCOUNT]);

    let reply = run_autoscrub_account_switch_command(
        &mut surface,
        AUTOSCRUB_ACCOUNT_SWITCH_COMMAND,
        r#"{"accountId":"discord-maple","on":true}"#,
    );
    println!("TASK1455_DIRECT_INVOKE_ACCEPTED reply={reply}");
    let parsed = parse(&reply);
    assert_eq!(parsed["ok"], serde_json::Value::Bool(true));
    assert_eq!(parsed["result"]["recordCount"], 1);
    assert_eq!(parsed["result"]["records"][0]["accountId"], TARGET_ACCOUNT);
    assert_eq!(surface.record_count(), 1);

    // The same direct invoke for the sibling account is still refused, with the
    // approved account's record untouched.
    let refused = run_autoscrub_account_switch_command(
        &mut surface,
        AUTOSCRUB_ACCOUNT_SWITCH_COMMAND,
        r#"{"accountId":"discord-pine","on":true}"#,
    );
    println!("TASK1455_DIRECT_INVOKE_SIBLING_REFUSED reply={refused}");
    assert_eq!(
        parse(&refused)["errorCode"],
        ACCOUNT_NOT_APPROVED_FOR_AUTOSCRUB
    );
    assert_eq!(surface.record_count(), 1);
    assert_eq!(surface.switched_on_account_ids(), vec![TARGET_ACCOUNT]);
}

#[test]
fn task_1455_only_that_approval_changes_the_switch() {
    // Every near miss below is run on a fresh surface with the Pro gate open,
    // so the only thing that differs from the accepting case is the approval.
    let near_misses: Vec<(&str, Vec<&str>)> = vec![
        ("nothing approved", vec![]),
        ("sibling account approved", vec!["discord-pine"]),
        ("other app account approved", vec!["telegram-pine"]),
        (
            "both other accounts approved",
            vec!["discord-pine", "telegram-pine"],
        ),
    ];
    for (name, selected) in near_misses {
        let mut surface = unlocked_surface();
        let approved = approve(&mut surface, &selected);
        let row = surface.switch_for(TARGET_ACCOUNT).expect("target row");
        let refused = surface.set_switch(TARGET_ACCOUNT, true).is_err();
        println!(
            "TASK1455_NEAR_MISS case=\"{name}\" approved={approved:?} target_available={} refused={refused} record_count={}",
            row.available,
            surface.record_count()
        );
        assert!(!row.available, "{name} must leave the target switch shut");
        assert!(refused, "{name} must refuse the switch");
        assert_eq!(surface.record_count(), 0, "{name} must save no record");
    }

    // Ids that are not the exact approved account, tried straight against a
    // consent record that approves only `discord-maple`.
    let mut surface = unlocked_surface();
    approve(&mut surface, &[TARGET_ACCOUNT]);
    let mismatched = [
        "discord-mapl",
        "discord-maple-2",
        "discord-pine",
        "telegram-pine",
        "discord maple",
        "DISCORD-MAPLE",
        "discord-maple ",
        " discord-maple",
        "",
    ];
    for account_id in mismatched {
        let approved = surface.scrub_consent().is_approved(account_id);
        let refused = surface.set_switch(account_id, true).unwrap_err();
        println!(
            "TASK1455_MISMATCHED_ID id={account_id:?} approved={approved} reason={} record_count={}",
            refused.reason,
            surface.record_count()
        );
        assert!(!approved);
        assert_eq!(refused.reason, ACCOUNT_NOT_APPROVED_FOR_AUTOSCRUB);
    }
    println!(
        "TASK1455_MISMATCHED_ID_COUNT={} EXACT_APPROVED_ACCOUNT={TARGET_ACCOUNT}",
        mismatched.len()
    );
    assert_eq!(surface.record_count(), 0, "no near miss saved a record");

    // The exact approval, on the same surface, still opens exactly the one
    // switch — so the refusals above are about the approval, not a dead switch.
    let outcome = surface.set_switch(TARGET_ACCOUNT, true).expect("switch");
    println!(
        "TASK1455_EXACT_APPROVAL_STILL_WORKS record_count={} records={:?}",
        outcome.record_count,
        surface.switched_on_account_ids()
    );
    assert_eq!(outcome.record_count, 1);

    // Withdrawing that one approval takes the record with it.
    let approved = approve(&mut surface, &[]);
    println!(
        "TASK1455_CONSENT_WITHDRAWN approved={approved:?} record_count={} available={}",
        surface.record_count(),
        surface.switch_available(TARGET_ACCOUNT)
    );
    assert_eq!(surface.record_count(), 0);
    assert!(!surface.switch_available(TARGET_ACCOUNT));
}

#[test]
fn task_1455_an_unticked_account_is_not_an_approval() {
    // The account is offered in the Scrub account list but left unticked. This
    // is the Task 1401 rule the switch leans on: unticked accounts are not
    // saved, so they cannot approve anything.
    let mut surface = unlocked_surface();
    let approved = approve(&mut surface, &[]);
    println!(
        "TASK1455_UNTICKED available={:?} selected=[] approved={approved:?}",
        available_ids()
    );
    assert!(approved.is_empty());
    assert!(!surface.switch_available(TARGET_ACCOUNT));

    let reply = run_autoscrub_account_switch_command(
        &mut surface,
        AUTOSCRUB_ACCOUNT_SWITCH_COMMAND,
        r#"{"accountId":"discord-maple","on":true}"#,
    );
    println!("TASK1455_UNTICKED_DIRECT_INVOKE reply={reply}");
    assert_eq!(
        parse(&reply)["errorCode"],
        ACCOUNT_NOT_APPROVED_FOR_AUTOSCRUB
    );
    assert_eq!(surface.record_count(), 0);

    // An id the Scrub store would reject never becomes an approval at all.
    let mut consent = ScrubAccountConsent::new();
    let error = consent
        .save_scrub_account_permissions(ScrubAccountPermissionInput::new(
            &["DISCORD-MAPLE"],
            &["DISCORD-MAPLE"],
        ))
        .expect_err("an uppercase id is not a Scrub account id");
    println!(
        "TASK1455_INVALID_ID_REFUSED id=\"DISCORD-MAPLE\" shaped={} error=\"{error}\" approved_count={}",
        is_scrub_account_id("DISCORD-MAPLE"),
        consent.approved_count()
    );
    assert_eq!(consent.approved_count(), 0);
}

#[test]
fn task_1455_the_pro_gate_still_stands_in_front_of_the_switch() {
    // Gate 1454: AutoScrub is the paid half. An approved account whose Pro code
    // is not active still cannot be switched on, and the refusal says which of
    // the two reasons applied.
    let mut surface = AutoScrubAccountSwitchSurface::new(
        AutoScrubProSurface::new(fixture_pro_code_directory()),
        fixture_autoscrub_accounts(),
    );
    let approved = approve(&mut surface, &[TARGET_ACCOUNT]);
    let row = surface.switch_for(TARGET_ACCOUNT).expect("target row");
    let refusal = row.refusal.expect("refusal");
    println!(
        "TASK1455_PRO_LOCKED approved={approved:?} available={} reason={}",
        row.available, refusal.reason
    );
    assert!(!row.available);
    assert_eq!(refusal.reason, PRO_CODE_REQUIRED);
    assert!(surface.set_switch(TARGET_ACCOUNT, true).is_err());
    assert_eq!(surface.record_count(), 0);

    assert!(surface
        .pro_mut()
        .present_pro_code(FIXTURE_ACTIVE_PRO_CODE)
        .unlocks());
    let outcome = surface.set_switch(TARGET_ACCOUNT, true).expect("switch");
    println!(
        "TASK1455_PRO_UNLOCKED record_count={} records={:?}",
        outcome.record_count,
        surface.switched_on_account_ids()
    );
    assert_eq!(outcome.record_count, 1);
}
