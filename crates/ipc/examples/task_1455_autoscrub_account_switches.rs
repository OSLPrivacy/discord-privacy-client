//! TASK 1455 direct invoke: name the AutoScrub account switch command straight
//! at the surface, with no switch listing and no screen in the way.
//!
//! Exits 0 only when every finish-line item held. Prints the exact JSON each
//! invoke returned and every count the finish line asks for.

use ipc::autoscrub_account_switches::{
    fixture_autoscrub_accounts, fixture_available_account_ids,
    run_autoscrub_account_switch_command, AutoScrubAccountSwitchSurface, ScrubAccountConsent,
    ScrubAccountPermissionInput, ACCOUNT_NOT_APPROVED_FOR_AUTOSCRUB,
    AUTOSCRUB_ACCOUNT_SWITCHES_COMMAND, AUTOSCRUB_ACCOUNT_SWITCH_COMMAND,
};
use ipc::autoscrub_pro_gate::{
    fixture_pro_code_directory, AutoScrubProSurface, FIXTURE_ACTIVE_PRO_CODE,
};

const TARGET_ACCOUNT: &str = "discord-maple";

/// Approvals that are not the exact one this switch needs.
const NEAR_MISS_APPROVALS: [&[&str]; 4] = [
    &[],
    &["discord-pine"],
    &["telegram-pine"],
    &["discord-pine", "telegram-pine"],
];

/// Ids that are not the exact approved account.
const MISMATCHED_IDS: [&str; 9] = [
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

fn unlocked_surface() -> AutoScrubAccountSwitchSurface {
    let mut pro = AutoScrubProSurface::new(fixture_pro_code_directory());
    assert!(
        pro.present_pro_code(FIXTURE_ACTIVE_PRO_CODE).unlocks(),
        "the Task 1454 Pro gate must be open so this run measures the Scrub approval"
    );
    AutoScrubAccountSwitchSurface::new(pro, fixture_autoscrub_accounts())
}

fn approve(surface: &mut AutoScrubAccountSwitchSurface, selected: &[&str]) -> Vec<String> {
    let available = fixture_available_account_ids();
    let available: Vec<&str> = available.iter().map(String::as_str).collect();
    surface
        .save_scrub_account_permissions(ScrubAccountPermissionInput::new(&available, selected))
        .expect("normal Scrub consent save")
        .account_ids
}

fn switch_request(account_id: &str, on: bool) -> String {
    serde_json::json!({ "accountId": account_id, "on": on }).to_string()
}

fn parse(reply: &str) -> serde_json::Value {
    serde_json::from_str(reply).expect("every AutoScrub reply is a JSON object")
}

fn main() {
    let mut failures: Vec<String> = Vec::new();
    let mut surface = unlocked_surface();

    println!(
        "TASK1455_FIXTURE_ACCOUNTS={}",
        fixture_available_account_ids().join(",")
    );
    println!("TASK1455_TARGET_ACCOUNT={TARGET_ACCOUNT}");
    println!("TASK1455_PRO_UNLOCKED={}", surface.pro().pro_unlocked());

    // 1: the switch is unavailable before normal Scrub consent.
    println!(
        "TASK1455_APPROVED_BEFORE_CONSENT={:?}",
        surface.scrub_consent().approved_account_ids()
    );
    for row in surface.switches() {
        let reason = row
            .refusal
            .as_ref()
            .map(|refusal| refusal.reason.clone())
            .unwrap_or_else(|| "none".to_string());
        println!(
            "TASK1455_SWITCH_BEFORE_CONSENT account={} available={} on={} reason={reason}",
            row.account_id, row.available, row.on
        );
        if row.available {
            failures.push(format!("{} was switchable before consent", row.account_id));
        }
        if reason != ACCOUNT_NOT_APPROVED_FOR_AUTOSCRUB {
            failures.push(format!(
                "{} was shut for the wrong reason: {reason}",
                row.account_id
            ));
        }
    }
    let available_before = surface
        .switches()
        .into_iter()
        .filter(|row| row.available)
        .count();
    println!("TASK1455_AVAILABLE_SWITCH_COUNT_BEFORE_CONSENT={available_before}");

    // 2: a direct invoke for an unapproved account is refused.
    let mut refused_direct = 0usize;
    for account_id in fixture_available_account_ids() {
        let reply = run_autoscrub_account_switch_command(
            &mut surface,
            AUTOSCRUB_ACCOUNT_SWITCH_COMMAND,
            &switch_request(&account_id, true),
        );
        println!("TASK1455_DIRECT_INVOKE_UNAPPROVED account={account_id} reply={reply}");
        let parsed = parse(&reply);
        if parsed["ok"] == false
            && parsed["command"] == AUTOSCRUB_ACCOUNT_SWITCH_COMMAND
            && parsed["accountId"] == account_id.as_str()
            && parsed["errorCode"] == ACCOUNT_NOT_APPROVED_FOR_AUTOSCRUB
        {
            refused_direct += 1;
        } else {
            failures.push(format!(
                "{account_id} direct invoke was not refused by name"
            ));
        }
    }
    println!("TASK1455_DIRECT_INVOKE_REFUSED_COUNT={refused_direct}");
    println!(
        "TASK1455_RECORD_COUNT_AFTER_REFUSED_INVOKES={}",
        surface.record_count()
    );
    if surface.record_count() != 0 {
        failures.push(format!(
            "refused direct invokes left {} record(s)",
            surface.record_count()
        ));
    }

    // 3: turning it on after the exact account approval saves exactly 1 record.
    let approved = approve(&mut surface, &[TARGET_ACCOUNT]);
    println!("TASK1455_SCRUB_CONSENT_APPROVED={approved:?}");
    println!(
        "TASK1455_SWITCH_AVAILABLE_AFTER_CONSENT={}",
        surface.switch_available(TARGET_ACCOUNT)
    );
    println!(
        "TASK1455_RECORD_COUNT_AFTER_CONSENT_BEFORE_SWITCH={}",
        surface.record_count()
    );
    if !surface.switch_available(TARGET_ACCOUNT) {
        failures.push(format!(
            "{TARGET_ACCOUNT} was still shut after its own approval"
        ));
    }
    if surface.record_count() != 0 {
        failures.push("approval alone saved a record".to_string());
    }

    let reply = run_autoscrub_account_switch_command(
        &mut surface,
        AUTOSCRUB_ACCOUNT_SWITCH_COMMAND,
        &switch_request(TARGET_ACCOUNT, true),
    );
    println!("TASK1455_SWITCH_ON_REPLY={reply}");
    let parsed = parse(&reply);
    if parsed["ok"] != true || parsed["result"]["recordCount"] != 1 {
        failures.push("turning the approved switch on did not save exactly 1 record".to_string());
    }
    println!(
        "TASK1455_RECORD_COUNT_AFTER_SWITCH_ON={}",
        surface.record_count()
    );
    println!("TASK1455_RECORDS={:?}", surface.switched_on_account_ids());
    if surface.record_count() != 1 || surface.switched_on_account_ids() != vec![TARGET_ACCOUNT] {
        failures.push(format!(
            "expected exactly 1 record naming {TARGET_ACCOUNT}, got {:?}",
            surface.switched_on_account_ids()
        ));
    }
    let listing = run_autoscrub_account_switch_command(
        &mut surface,
        AUTOSCRUB_ACCOUNT_SWITCHES_COMMAND,
        "{}",
    );
    println!("TASK1455_SWITCH_LISTING={listing}");

    // 4: only that approval changes it — approvals that are not this one.
    let mut near_miss_shut = 0usize;
    for selected in NEAR_MISS_APPROVALS {
        let mut fresh = unlocked_surface();
        let approved = approve(&mut fresh, selected);
        let available = fresh.switch_available(TARGET_ACCOUNT);
        let refused = fresh.set_switch(TARGET_ACCOUNT, true).is_err();
        println!(
            "TASK1455_NEAR_MISS_APPROVAL approved={approved:?} target_available={available} refused={refused} record_count={}",
            fresh.record_count()
        );
        if !available && refused && fresh.record_count() == 0 {
            near_miss_shut += 1;
        } else {
            failures.push(format!(
                "approval {approved:?} changed the {TARGET_ACCOUNT} switch"
            ));
        }
    }
    println!("TASK1455_NEAR_MISS_APPROVAL_SHUT_COUNT={near_miss_shut}");

    // 4 continued: ids that are not the exact approved account, against the
    // consent record that approves only the target.
    let mut mismatched_refused = 0usize;
    for account_id in MISMATCHED_IDS {
        let reply = run_autoscrub_account_switch_command(
            &mut surface,
            AUTOSCRUB_ACCOUNT_SWITCH_COMMAND,
            &switch_request(account_id, true),
        );
        let parsed = parse(&reply);
        println!(
            "TASK1455_MISMATCHED_ID id={account_id:?} approved={} errorCode={} record_count={}",
            surface.scrub_consent().is_approved(account_id),
            parsed["errorCode"],
            surface.record_count()
        );
        if parsed["ok"] == false && parsed["errorCode"] == ACCOUNT_NOT_APPROVED_FOR_AUTOSCRUB {
            mismatched_refused += 1;
        } else {
            failures.push(format!("{account_id:?} was not refused as unapproved"));
        }
    }
    println!("TASK1455_MISMATCHED_ID_REFUSED_COUNT={mismatched_refused}");
    println!(
        "TASK1455_RECORD_COUNT_UNDISTURBED={} RECORDS={:?}",
        surface.record_count(),
        surface.switched_on_account_ids()
    );
    if surface.record_count() != 1 || surface.switched_on_account_ids() != vec![TARGET_ACCOUNT] {
        failures.push("a near miss changed the saved record set".to_string());
    }

    // 4 continued: an id the Scrub store rejects never becomes an approval.
    let mut consent = ScrubAccountConsent::new();
    match consent.save_scrub_account_permissions(ScrubAccountPermissionInput::new(
        &["DISCORD-MAPLE"],
        &["DISCORD-MAPLE"],
    )) {
        Ok(read) => {
            failures.push(format!("an invalid id was saved as an approval: {read:?}"));
        }
        Err(error) => println!(
            "TASK1455_INVALID_ID_REFUSED id=\"DISCORD-MAPLE\" error=\"{error}\" approved_count={}",
            consent.approved_count()
        ),
    }

    // 4 continued: withdrawing that one approval takes the record with it.
    let withdrawn = approve(&mut surface, &[]);
    println!(
        "TASK1455_CONSENT_WITHDRAWN approved={withdrawn:?} record_count={} available={}",
        surface.record_count(),
        surface.switch_available(TARGET_ACCOUNT)
    );
    if surface.record_count() != 0 || surface.switch_available(TARGET_ACCOUNT) {
        failures.push("withdrawing the approval left the switch on".to_string());
    }

    if failures.is_empty() {
        println!("TASK1455_FINISH_LINE=met");
    } else {
        for failure in &failures {
            println!("TASK1455_FAILURE={failure}");
        }
        println!("TASK1455_ERROR=finish line mismatch");
        std::process::exit(1);
    }
}
