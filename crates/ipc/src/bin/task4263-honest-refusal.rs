//! TASK 4263 check - the three refuse honestly while they are still empty.
//!
//! Makes 3 real attempts to send and 3 real attempts to receive, one per half
//! restored service, through `cmd_osl_run_allowed_place_action`, the command the
//! trusted UI calls to act at a place.
//!
//! Every attempt is sorted into exactly one of three buckets:
//!
//! * `refused_by_name` - the call returned an error whose words contain both the
//!   app's display name and the name of the missing part;
//! * `reported_success` - the call returned Ok, i.e. it told the caller the send
//!   or receive worked when nothing is behind it;
//! * `quiet_nothing`   - anything else: an empty refusal, or a refusal that does
//!   not say which app or which part. To the person reading it that is the same
//!   as the service silently doing nothing, which is what this task forbids.
//!
//! Two things stop this from being a check that passes with the feature absent:
//!
//! 1. A DISCORD control place is whitelisted and driven through the same command
//!    in the same run. If the command refused everything, the control would fail
//!    and so would this check. The control has to succeed for the check to pass.
//! 2. The three refusals are required NOT to be the allowed-place store's own
//!    refusal. Without the 4263 guard the store answers "place not allowed"
//!    (its stable-id shape is discord-only), which names neither the app nor the
//!    missing part - so it lands in `quiet_nothing` and this check exits 1.
//!
//! Exits 0 only when refused_by_name is 6, quiet_nothing is 0, reported_success
//! is 0 and the control succeeded both ways. Exits 1 naming every attempt that
//! broke the rule.

use ipc::allowed_places::{add_allowed_place_record, is_allowed_place_record, AllowedPlaceRecord};
use ipc::commands::{cmd_osl_run_allowed_place_action, AllowedPlaceAction};
use ipc::half_restored_surface::{SurfaceDirection, HALF_RESTORED_APPS};
use std::path::PathBuf;
use std::process::ExitCode;

const EXPECTED_ATTEMPTS_PER_DIRECTION: usize = 3;

/// The two directions, paired with the command action that performs each.
const DIRECTIONS: [(AllowedPlaceAction, SurfaceDirection); 2] = [
    (AllowedPlaceAction::Place, SurfaceDirection::Send),
    (AllowedPlaceAction::Read, SurfaceDirection::Receive),
];

/// Words that mean the allowed-place store refused, not the 4263 guard.
const STORE_REFUSAL_MARKERS: [&str; 2] = ["place not allowed", "stable_id is invalid"];

struct Attempt {
    app_id: &'static str,
    display_name: &'static str,
    direction: SurfaceDirection,
    missing_part_id: &'static str,
    missing_part_name: &'static str,
    gating_task: &'static str,
    outcome: Outcome,
    words: String,
}

enum Outcome {
    RefusedByName,
    ReportedSuccess,
    QuietNothing(&'static str),
}

impl Outcome {
    fn as_str(&self) -> &'static str {
        match self {
            Self::RefusedByName => "refused_by_name",
            Self::ReportedSuccess => "reported_success",
            Self::QuietNothing(_) => "quiet_nothing",
        }
    }
}

fn main() -> ExitCode {
    let dir = scratch_dir();
    if let Err(error) = std::fs::create_dir_all(&dir) {
        eprintln!("TASK4263 could not make a scratch directory: {error}");
        return ExitCode::from(2);
    }

    let mut failures = Vec::new();

    // ---- control: a real service at a whitelisted place still works ----------
    let control = AllowedPlaceRecord::discord_direct_message("900000000000004263", "LANTERN-4263");
    let mut control_ok = 0usize;
    if let Err(error) = add_allowed_place_record(&dir, control.clone()) {
        eprintln!("TASK4263 could not whitelist the Discord control place: {error}");
        let _ = std::fs::remove_dir_all(&dir);
        return ExitCode::from(2);
    }
    match is_allowed_place_record(&dir, &control) {
        Ok(true) => {}
        other => {
            eprintln!("TASK4263 Discord control place did not save: {other:?}");
            let _ = std::fs::remove_dir_all(&dir);
            return ExitCode::from(2);
        }
    }
    for (action, direction) in DIRECTIONS {
        match cmd_osl_run_allowed_place_action(dir.clone(), action, control.clone()) {
            Ok(receipt) => {
                control_ok += 1;
                println!(
                    "TASK4263_CONTROL app=Discord direction={} outcome=ok action={} item_name={:?}",
                    direction.as_str(),
                    receipt.action,
                    receipt.item_name
                );
            }
            Err(refusal) => {
                println!(
                    "TASK4263_CONTROL app=Discord direction={} outcome=refused words={refusal:?}",
                    direction.as_str()
                );
                failures.push(format!(
                    "the Discord control {} was refused ({refusal}), so this run cannot tell an honest refusal from a command that refuses everything",
                    direction.as_str()
                ));
            }
        }
    }

    // ---- the three ----------------------------------------------------------
    let mut attempts = Vec::new();
    for app in HALF_RESTORED_APPS {
        let record = place_for(app.app_id);
        for (action, direction) in DIRECTIONS {
            let part = app.missing_part(direction);
            let result = cmd_osl_run_allowed_place_action(dir.clone(), action, record.clone());
            let (outcome, words) = match result {
                Ok(receipt) => (
                    Outcome::ReportedSuccess,
                    format!(
                        "Ok(action={} item_name={:?} stable_id={})",
                        receipt.action, receipt.item_name, receipt.stable_id
                    ),
                ),
                Err(refusal) if refusal.trim().is_empty() => {
                    (Outcome::QuietNothing("refusal has no words"), refusal)
                }
                Err(refusal)
                    if STORE_REFUSAL_MARKERS
                        .iter()
                        .any(|marker| refusal.contains(marker)) =>
                {
                    (
                        Outcome::QuietNothing(
                            "the allowed-place store refused first, so the reason given is not the missing part",
                        ),
                        refusal,
                    )
                }
                Err(refusal) if !refusal.contains(app.display_name) => (
                    Outcome::QuietNothing("refusal does not name the app"),
                    refusal,
                ),
                Err(refusal) if !refusal.contains(part.name) => (
                    Outcome::QuietNothing("refusal does not name the missing part"),
                    refusal,
                ),
                Err(refusal) => (Outcome::RefusedByName, refusal),
            };
            attempts.push(Attempt {
                app_id: app.app_id,
                display_name: app.display_name,
                direction,
                missing_part_id: part.id,
                missing_part_name: part.name,
                gating_task: part.gating_task,
                outcome,
                words,
            });
        }
    }

    let _ = std::fs::remove_dir_all(&dir);

    for attempt in &attempts {
        println!(
            "TASK4263_ATTEMPT direction={} app_id={} app={} missing_part={} missing_part_name={:?} gate={} outcome={} words={:?}",
            attempt.direction.as_str(),
            attempt.app_id,
            attempt.display_name,
            attempt.missing_part_id,
            attempt.missing_part_name,
            attempt.gating_task,
            attempt.outcome.as_str(),
            attempt.words,
        );
    }

    let send_attempts = count(&attempts, |a| a.direction == SurfaceDirection::Send);
    let receive_attempts = count(&attempts, |a| a.direction == SurfaceDirection::Receive);
    let send_refused = count(&attempts, |a| {
        a.direction == SurfaceDirection::Send && matches!(a.outcome, Outcome::RefusedByName)
    });
    let receive_refused = count(&attempts, |a| {
        a.direction == SurfaceDirection::Receive && matches!(a.outcome, Outcome::RefusedByName)
    });
    let quiet = count(&attempts, |a| matches!(a.outcome, Outcome::QuietNothing(_)));
    let reported_success = count(&attempts, |a| matches!(a.outcome, Outcome::ReportedSuccess));

    println!("TASK4263_CONTROL_OK_COUNT={control_ok}");
    println!("TASK4263_SEND_ATTEMPT_COUNT={send_attempts}");
    println!("TASK4263_RECEIVE_ATTEMPT_COUNT={receive_attempts}");
    println!("TASK4263_SEND_REFUSED_BY_NAME_COUNT={send_refused}");
    println!("TASK4263_RECEIVE_REFUSED_BY_NAME_COUNT={receive_refused}");
    println!(
        "TASK4263_NAMED_APP_COUNT={}",
        distinct(&attempts, |a| a.display_name)
    );
    println!(
        "TASK4263_NAMED_MISSING_PART_COUNT={}",
        distinct(&attempts, |a| a.missing_part_id)
    );
    println!("TASK4263_QUIET_NOTHING_COUNT={quiet}");
    println!("TASK4263_REPORTED_SUCCESS_COUNT={reported_success}");

    if send_attempts != EXPECTED_ATTEMPTS_PER_DIRECTION {
        failures.push(format!(
            "{send_attempts} send attempts were made, expected {EXPECTED_ATTEMPTS_PER_DIRECTION}"
        ));
    }
    if receive_attempts != EXPECTED_ATTEMPTS_PER_DIRECTION {
        failures.push(format!(
            "{receive_attempts} receive attempts were made, expected {EXPECTED_ATTEMPTS_PER_DIRECTION}"
        ));
    }
    for attempt in &attempts {
        match &attempt.outcome {
            Outcome::RefusedByName => {}
            Outcome::ReportedSuccess => failures.push(format!(
                "the {} {} reported success with nothing behind it: {}",
                attempt.display_name,
                attempt.direction.as_str(),
                attempt.words
            )),
            Outcome::QuietNothing(why) => failures.push(format!(
                "the {} {} quietly did nothing: {why} ({})",
                attempt.display_name,
                attempt.direction.as_str(),
                attempt.words
            )),
        }
    }
    // A refusal that never names the gating task leaves the person with nothing
    // to look up, so the gate name is part of the contract too.
    for attempt in &attempts {
        if matches!(attempt.outcome, Outcome::RefusedByName)
            && !attempt.words.contains(attempt.gating_task)
        {
            failures.push(format!(
                "the {} {} refusal does not name {}",
                attempt.display_name,
                attempt.direction.as_str(),
                attempt.gating_task
            ));
        }
    }

    if failures.is_empty() {
        println!("TASK4263_RESULT=pass");
        ExitCode::SUCCESS
    } else {
        for failure in &failures {
            println!("TASK4263_FAILURE {failure}");
        }
        println!("TASK4263_RESULT=fail");
        ExitCode::from(1)
    }
}

fn count(attempts: &[Attempt], predicate: impl Fn(&Attempt) -> bool) -> usize {
    attempts.iter().filter(|a| predicate(a)).count()
}

fn distinct(attempts: &[Attempt], key: impl Fn(&Attempt) -> &'static str) -> usize {
    let mut seen: Vec<&str> = attempts
        .iter()
        .filter(|a| matches!(a.outcome, Outcome::RefusedByName))
        .map(key)
        .collect();
    seen.sort_unstable();
    seen.dedup();
    seen.len()
}

fn place_for(app_id: &str) -> AllowedPlaceRecord {
    AllowedPlaceRecord {
        app: app_id.to_owned(),
        account: format!("{app_id}-account-4263"),
        kind: "direct_message".to_owned(),
        stable_id: format!("{app_id}:{app_id}-account-4263:direct_message:place-4263"),
        place_name: format!("{app_id} direct message 4263"),
        person_name: "TASK 4263 fixture".to_owned(),
    }
}

fn scratch_dir() -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!(
        "osl-task4263-honest-refusal-{}-{nanos}",
        std::process::id()
    ))
}
