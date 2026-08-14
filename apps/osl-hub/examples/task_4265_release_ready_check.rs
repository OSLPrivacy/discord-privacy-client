//! TASK 4265 - the release Ready check.
//!
//! Reads the release label table (`data/release-ready-labels.txt`), recomputes
//! every label from the real product module `src/release_ready_labels.rs`, and
//! exits 1 when a release claims a label it has not earned - including a Ready
//! written by hand beside one of the three restored but empty services.
//!
//! Run:
//!   rustc apps/osl-hub/examples/task_4265_release_ready_check.rs -o /tmp/task4265
//!   /tmp/task4265 [--table <path>]
//!
//! Cargo cannot reach this crate in this lane: `apps/osl-hub` does not parse at
//! HEAD (five merge-damaged modules, none of them this task's). The module under
//! test has no dependencies for exactly that reason, so the check runs the real
//! product code rather than a copy of it.

#[path = "../src/release_ready_labels.rs"]
mod release_ready_labels;

use release_ready_labels::{
    catalogue_service, outstanding_own_checks, release_ready_decision, CatalogueService,
    DeliveryProof, ReadyDecision, NOT_READY_LABEL, READY_LABEL, RELEASE_READY_CATALOGUE, THE_THREE,
};
use std::path::PathBuf;

const DEFAULT_TABLE: &str = "data/release-ready-labels.txt";
const SERVICES_SOURCE: &str = "src/services.rs";

struct ClaimedRow {
    service_id: String,
    claimed_label: String,
    proof: Option<DeliveryProof>,
}

fn main() {
    let table_path = table_path_from_args();
    let table = std::fs::read_to_string(&table_path).unwrap_or_else(|error| {
        fail_now(&format!(
            "TASK4265_FAILURE unreadable_table path={} error={error}",
            table_path.display()
        ))
    });

    let rows = parse_table(&table)
        .unwrap_or_else(|error| fail_now(&format!("TASK4265_FAILURE unreadable_table {error}")));
    println!("TASK4265_TABLE={}", table_path.display());
    println!("TASK4265_CATALOGUE_COUNT={}", RELEASE_READY_CATALOGUE.len());
    println!("TASK4265_CLAIMED_ROW_COUNT={}", rows.len());

    let mut failures: Vec<String> = Vec::new();

    // Every catalogue service is labelled, and nothing else is.
    for service in RELEASE_READY_CATALOGUE {
        let claims = rows
            .iter()
            .filter(|row| row.service_id == service.service_id)
            .count();
        if claims != 1 {
            failures.push(format!(
                "TASK4265_FAILURE service_claimed_{claims}_times service={} - every catalogue service is labelled exactly once",
                service.service_id
            ));
        }
    }
    for row in &rows {
        if catalogue_service(&row.service_id).is_none() {
            failures.push(format!(
                "TASK4265_FAILURE unknown_service service={} - not a service in the catalogue",
                row.service_id
            ));
        }
    }

    let proofs: Vec<DeliveryProof> = rows.iter().filter_map(|row| row.proof.clone()).collect();

    let mut ready_count = 0usize;
    let mut three_count = 0usize;
    let mut three_not_ready_count = 0usize;
    let mut three_reason_names_missing_part_count = 0usize;
    let mut own_checks_passed_ready_count = 0usize;
    let mut empty_service_ready_count = 0usize;
    let mut hand_set_ready_count = 0usize;
    let mut stale_label_count = 0usize;

    for service in RELEASE_READY_CATALOGUE {
        let decision = release_ready_decision(&service, &proofs);
        let outstanding = outstanding_own_checks(&service);
        let claimed = rows
            .iter()
            .find(|row| row.service_id == service.service_id)
            .map(|row| row.claimed_label.clone())
            .unwrap_or_else(|| "<unlabelled>".to_owned());

        println!(
            "TASK4265_LABEL service={} name={} claimed=\"{claimed}\" label=\"{}\" own_checks_passed={} outstanding_parts={} refusal={} reason=\"{}\"",
            service.service_id,
            service.display_name,
            decision.label,
            decision.own_checks_passed,
            outstanding.len(),
            decision.refusal.unwrap_or("none"),
            decision.reason.clone().unwrap_or_default()
        );

        if decision.is_ready() {
            ready_count += 1;
            if decision.own_checks_passed {
                own_checks_passed_ready_count += 1;
            }
        }

        // 4265b: a service that still owes a build check of its own must never
        // come back Ready. If the label rule ever skips such a service, this is
        // what catches it.
        if decision.is_ready() && !outstanding.is_empty() {
            empty_service_ready_count += 1;
            failures.push(format!(
                "TASK4265_FAILURE empty_service_reached_ready service={} - {} still owes {} ({} has not passed) and was labelled {READY_LABEL}",
                service.service_id,
                service.display_name,
                outstanding[0].part_name,
                outstanding[0].gating_task
            ));
        }

        if THE_THREE.contains(&service.service_id) {
            three_count += 1;
            if decision.label == NOT_READY_LABEL {
                three_not_ready_count += 1;
            } else {
                failures.push(format!(
                    "TASK4265_FAILURE one_of_the_three_is_ready service={} - a restored but empty service reached {READY_LABEL}",
                    service.service_id
                ));
            }
            if reason_names_every_missing_part(&service, &decision) {
                three_reason_names_missing_part_count += 1;
            } else {
                failures.push(format!(
                    "TASK4265_FAILURE reason_does_not_name_the_missing_part service={} reason=\"{}\"",
                    service.service_id,
                    decision.reason.clone().unwrap_or_default()
                ));
            }
        }

        // The claim in the table has to match what the rule computes.
        if claimed == READY_LABEL && !decision.is_ready() {
            hand_set_ready_count += 1;
            failures.push(format!(
                "TASK4265_FAILURE hand_set_ready service={} - the table claims {READY_LABEL} but {}",
                service.service_id,
                decision.reason.clone().unwrap_or_else(|| "it has not earned it".to_owned())
            ));
        } else if claimed != READY_LABEL && decision.is_ready() {
            stale_label_count += 1;
            failures.push(format!(
                "TASK4265_FAILURE stale_label service={} - the table claims \"{claimed}\" but the service has earned {READY_LABEL}",
                service.service_id
            ));
        } else if claimed != READY_LABEL && claimed != NOT_READY_LABEL {
            failures.push(format!(
                "TASK4265_FAILURE unreadable_label service={} claimed=\"{claimed}\" - a label reads \"{READY_LABEL}\" or \"{NOT_READY_LABEL}\"",
                service.service_id
            ));
        }
    }

    // The three must actually be in the catalogue: a check that stopped asking
    // about them would otherwise pass by silence.
    if three_count != THE_THREE.len() {
        failures.push(format!(
            "TASK4265_FAILURE the_three_are_not_counted found={three_count} expected={} - X, Instagram and Messenger must be labelled like everything else",
            THE_THREE.len()
        ));
    }

    // And a service that has done the work must still get through, so the check
    // cannot pass by refusing everything.
    if ready_count == 0 {
        failures.push(
            "TASK4265_FAILURE nothing_reached_ready - a service whose own checks have passed must still reach Ready".to_owned(),
        );
    }

    check_gate_0808_words(&mut failures);

    println!("TASK4265_THREE_COUNT={three_count}");
    println!("TASK4265_THREE_NOT_READY_COUNT={three_not_ready_count}");
    println!(
        "TASK4265_THREE_REASON_NAMES_MISSING_PART_COUNT={three_reason_names_missing_part_count}"
    );
    println!("TASK4265_READY_COUNT={ready_count}");
    println!("TASK4265_OWN_CHECKS_PASSED_READY_COUNT={own_checks_passed_ready_count}");
    println!("TASK4265_EMPTY_SERVICE_READY_COUNT={empty_service_ready_count}");
    println!("TASK4265_HAND_SET_READY_COUNT={hand_set_ready_count}");
    println!("TASK4265_STALE_LABEL_COUNT={stale_label_count}");

    if failures.is_empty() {
        println!("TASK4265_RESULT=pass");
        std::process::exit(0);
    }
    for failure in &failures {
        println!("{failure}");
    }
    println!("TASK4265_FAILURE_COUNT={}", failures.len());
    println!("TASK4265_RESULT=fail");
    std::process::exit(1);
}

fn reason_names_every_missing_part(service: &CatalogueService, decision: &ReadyDecision) -> bool {
    let Some(reason) = decision.reason.as_deref() else {
        return false;
    };
    let outstanding = outstanding_own_checks(service);
    if outstanding.is_empty() {
        return false;
    }
    reason.contains(service.display_name)
        && outstanding
            .iter()
            .all(|check| reason.contains(check.part_name) && reason.contains(check.gating_task))
}

/// Gate 0808 owns the words `Ready` refuses with. If `services.rs` renames them,
/// the two halves of the label have drifted apart and this check says so rather
/// than quietly labelling in a private language.
fn check_gate_0808_words(failures: &mut Vec<String>) {
    let path = manifest_dir().join(SERVICES_SOURCE);
    let Ok(source) = std::fs::read_to_string(&path) else {
        failures.push(format!(
            "TASK4265_FAILURE unreadable_services_source path={}",
            path.display()
        ));
        return;
    };
    for word in [
        release_ready_labels::READY_REQUIRES_REAL_TWO_PERSON_CAPABILITY,
        release_ready_labels::READY_REQUIRES_MATCHING_DELIVERY_PROOF,
    ] {
        if source.contains(word) {
            println!("TASK4265_GATE0808_WORD_SHARED={word}");
        } else {
            failures.push(format!(
                "TASK4265_FAILURE gate_0808_word_missing word={word} - services.rs no longer refuses in these words"
            ));
        }
    }
}

fn parse_table(table: &str) -> Result<Vec<ClaimedRow>, String> {
    let mut rows = Vec::new();
    for (index, line) in table.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let fields: Vec<&str> = line.split('|').map(str::trim).collect();
        if fields.len() != 3 {
            return Err(format!(
                "line {} has {} fields, expected 3",
                index + 1,
                fields.len()
            ));
        }
        rows.push(ClaimedRow {
            service_id: fields[0].to_owned(),
            claimed_label: fields[1].to_owned(),
            proof: parse_proof(fields[2])
                .map_err(|error| format!("line {}: {error}", index + 1))?,
        });
    }
    Ok(rows)
}

fn parse_proof(field: &str) -> Result<Option<DeliveryProof>, String> {
    if field == "no proof" {
        return Ok(None);
    }
    let rest = field.strip_prefix("proof ").ok_or_else(|| {
        format!("proof field reads \"no proof\" or \"proof ...\", got \"{field}\"")
    })?;
    let mut protected_message_id = String::new();
    let mut sender_person_id = String::new();
    let mut recipient_person_id = String::new();
    let mut protected_message_received = false;
    let mut received_by_real_other_person = false;
    for pair in rest.split_whitespace() {
        let (key, value) = pair
            .split_once('=')
            .ok_or_else(|| format!("proof field \"{pair}\" is not key=value"))?;
        match key {
            "id" => protected_message_id = value.to_owned(),
            "sender" => sender_person_id = value.to_owned(),
            "recipient" => recipient_person_id = value.to_owned(),
            "received" => protected_message_received = value == "yes",
            "real_other_person" => received_by_real_other_person = value == "yes",
            other => return Err(format!("unknown proof field \"{other}\"")),
        }
    }
    Ok(Some(DeliveryProof {
        // The proof carries the service it is for; a proof for the wrong service
        // is exactly what gate 0808 refuses, so it is not filled in from the row.
        service_id: sender_service_id_from(&protected_message_id),
        protected_message_id,
        sender_person_id,
        recipient_person_id,
        protected_message_received,
        received_by_real_other_person,
    }))
}

/// A protected message id reads `protected-<task>-<service>`; the service it was
/// delivered on is the last part. Keeping the service inside the proof id means a
/// row cannot claim someone else's proof by sitting next to it in the table.
fn sender_service_id_from(protected_message_id: &str) -> String {
    protected_message_id
        .rsplit('-')
        .next()
        .unwrap_or_default()
        .to_owned()
}

fn table_path_from_args() -> PathBuf {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--table" {
            let value = args
                .next()
                .unwrap_or_else(|| fail_now("TASK4265_FAILURE --table needs a path"));
            return PathBuf::from(value);
        }
        fail_now(&format!("TASK4265_FAILURE unknown argument \"{arg}\""));
    }
    manifest_dir().join(DEFAULT_TABLE)
}

fn manifest_dir() -> PathBuf {
    option_env!("CARGO_MANIFEST_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("apps/osl-hub"))
}

fn fail_now(message: &str) -> ! {
    println!("{message}");
    println!("TASK4265_RESULT=fail");
    std::process::exit(1);
}
