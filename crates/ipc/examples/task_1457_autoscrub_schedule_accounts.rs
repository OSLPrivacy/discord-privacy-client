//! TASK 1457 direct invoke: try the AutoScrub schedule bypass with no screen in
//! the way — save `maple-daily` naming the approved `discord-maple`, then change
//! only the account name on that row to the unapproved `discord-pine`.
//!
//! Exits 0 only when every finish-line item held. Prints the exact JSON each
//! invoke returned and every count the finish line asks for.

use ipc::autoscrub_account_switches::ACCOUNT_NOT_APPROVED_FOR_AUTOSCRUB;
use ipc::autoscrub_pro_gate::AUTOSCRUB_SCHEDULE_COMMAND;
use ipc::autoscrub_schedule_accounts::{
    fixture_schedule_surface, run_autoscrub_schedule_account_command, AutoScrubScheduleSurface,
    AUTOSCRUB_SCHEDULE_ACCOUNT_COMMAND, AUTOSCRUB_SCHEDULE_ROWS_COMMAND, FIXTURE_CADENCE,
};

const SCHEDULE: &str = "maple-daily";
const APPROVED: &str = "discord-maple";
const UNAPPROVED: &str = "discord-pine";

fn rows_json(surface: &mut AutoScrubScheduleSurface) -> serde_json::Value {
    let reply =
        run_autoscrub_schedule_account_command(surface, AUTOSCRUB_SCHEDULE_ROWS_COMMAND, "{}");
    println!("  {AUTOSCRUB_SCHEDULE_ROWS_COMMAND} -> {reply}");
    serde_json::from_str(&reply).expect("a JSON reply")
}

fn row_count(surface: &mut AutoScrubScheduleSurface) -> u64 {
    rows_json(surface)["result"]["rowCount"]
        .as_u64()
        .expect("a rowCount")
}

fn main() {
    let mut surface = fixture_schedule_surface();
    println!(
        "fixture: Pro gate open, normal Scrub consent approves {:?} and nothing else",
        surface.switches().scrub_consent().approved_account_ids()
    );

    println!("\n[1a] the schedule-row count before the save");
    let before = row_count(&mut surface);
    println!("  row count before = {before}");
    assert_eq!(before, 0, "the count must be 0 before");

    println!("\n[1b] save {SCHEDULE} naming the approved {APPROVED}");
    let reply = run_autoscrub_schedule_account_command(
        &mut surface,
        AUTOSCRUB_SCHEDULE_COMMAND,
        &serde_json::json!({
            "scheduleName": SCHEDULE,
            "accountId": APPROVED,
            "cadence": FIXTURE_CADENCE,
        })
        .to_string(),
    );
    println!("  {AUTOSCRUB_SCHEDULE_COMMAND} -> {reply}");
    let saved: serde_json::Value = serde_json::from_str(&reply).expect("a JSON reply");
    assert_eq!(saved["ok"], serde_json::json!(true), "the save must be accepted");
    let after_save = row_count(&mut surface);
    println!("  row count after  = {after_save}");
    assert_eq!(after_save, 1, "the count must be 1 after");

    println!("\n[2] change ONLY the account name on {SCHEDULE} to the unapproved {UNAPPROVED}");
    let reply = run_autoscrub_schedule_account_command(
        &mut surface,
        AUTOSCRUB_SCHEDULE_ACCOUNT_COMMAND,
        &serde_json::json!({ "scheduleName": SCHEDULE, "accountId": UNAPPROVED }).to_string(),
    );
    println!("  {AUTOSCRUB_SCHEDULE_ACCOUNT_COMMAND} -> {reply}");
    let refused: serde_json::Value = serde_json::from_str(&reply).expect("a JSON reply");
    assert_eq!(refused["ok"], serde_json::json!(false), "it must be refused");
    assert_eq!(
        refused["errorCode"],
        serde_json::json!(ACCOUNT_NOT_APPROVED_FOR_AUTOSCRUB),
        "the reason must be account_not_approved_for_autoscrub"
    );
    assert_eq!(refused["accountId"], serde_json::json!(UNAPPROVED));
    println!(
        "  refused: errorCode={} accountId={}",
        refused["errorCode"], refused["accountId"]
    );

    println!("\n[3] what {SCHEDULE} names now, and the count");
    let named = surface.accounts_named_by(SCHEDULE);
    println!("  {SCHEDULE} names {named:?}");
    assert_eq!(
        named,
        vec![APPROVED.to_string()],
        "the schedule must still name only the approved account"
    );
    let after_refusal = row_count(&mut surface);
    println!("  row count after the refusal = {after_refusal}");
    assert_eq!(after_refusal, 1, "the count must stay 1");
    assert!(
        !surface.scheduled_account_ids().contains(&UNAPPROVED.to_string()),
        "no schedule may name the unapproved account"
    );

    println!("\nFINISH LINE");
    println!("  [x] schedule-row count 0 before, {after_save} after {SCHEDULE} names {APPROVED}");
    println!(
        "  [x] {UNAPPROVED} refused as {}",
        ACCOUNT_NOT_APPROVED_FOR_AUTOSCRUB
    );
    println!("  [x] {SCHEDULE} still names only {named:?}, count stays {after_refusal}");
}
