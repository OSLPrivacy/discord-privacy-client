//! TASK 4263 - the three refuse honestly while they are still empty.
//!
//! X, Instagram and Messenger are back in the catalogue and on the picker but
//! nothing behind them is built. Every attempt to send or receive on one of them
//! must come back naming the app and naming the missing part - never a success,
//! and never a shrug.
//!
//! The Discord control in each test is not decoration: it drives the same
//! command in the same run and must succeed, so a command that refused
//! everything could not make these tests pass.

use ipc::allowed_places::{add_allowed_place_record, is_allowed_place_record, AllowedPlaceRecord};
use ipc::commands::{
    cmd_osl_run_allowed_place_action, cmd_osl_trace_allowed_place_protected_message_path,
    AllowedPlaceAction, ProtectedPlaceAction,
};
use ipc::half_restored_surface::{
    half_restored_app, require_receive_ready, require_send_ready, SurfaceDirection,
    HALF_RESTORED_APPS,
};
use std::path::PathBuf;

fn scratch_dir(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!(
        "osl-task4263-{tag}-{}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
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

/// A refusal that is really the allowed-place store talking says nothing about
/// which part is missing, so it does not count as an honest refusal here.
fn is_store_refusal(words: &str) -> bool {
    words.contains("place not allowed") || words.contains("stable_id is invalid")
}

#[test]
fn three_sends_and_three_receives_each_refuse_naming_the_app_and_the_missing_part() {
    let dir = scratch_dir("place-action");

    // Control: Discord at a whitelisted place must still work in this run.
    let control = AllowedPlaceRecord::discord_direct_message("900000000000004263", "LANTERN-4263");
    add_allowed_place_record(&dir, control.clone()).expect("whitelist the control place");
    assert!(is_allowed_place_record(&dir, &control).expect("control place readable"));
    let mut control_ok = 0usize;
    for action in [AllowedPlaceAction::Place, AllowedPlaceAction::Read] {
        let receipt = cmd_osl_run_allowed_place_action(dir.clone(), action, control.clone())
            .unwrap_or_else(|error| {
                panic!("TASK4263 Discord control must still work: {error}")
            });
        println!(
            "TASK4263_TEST_CONTROL action={} item_name={:?}",
            receipt.action, receipt.item_name
        );
        control_ok += 1;
    }
    assert_eq!(control_ok, 2);

    let mut send_refused = 0usize;
    let mut receive_refused = 0usize;
    let mut quiet_nothing = 0usize;
    let mut reported_success = 0usize;

    for app in HALF_RESTORED_APPS {
        let record = place_for(app.app_id);
        for (action, direction) in [
            (AllowedPlaceAction::Place, SurfaceDirection::Send),
            (AllowedPlaceAction::Read, SurfaceDirection::Receive),
        ] {
            let part = app.missing_part(direction);
            match cmd_osl_run_allowed_place_action(dir.clone(), action, record.clone()) {
                Ok(receipt) => {
                    reported_success += 1;
                    println!(
                        "TASK4263_TEST_ATTEMPT app={} direction={} outcome=reported_success receipt={:?}",
                        app.display_name,
                        direction.as_str(),
                        receipt
                    );
                }
                Err(words) => {
                    let honest = !words.trim().is_empty()
                        && !is_store_refusal(&words)
                        && words.contains(app.display_name)
                        && words.contains(part.name)
                        && words.contains(part.gating_task);
                    println!(
                        "TASK4263_TEST_ATTEMPT app={} direction={} outcome={} words={words:?}",
                        app.display_name,
                        direction.as_str(),
                        if honest { "refused_by_name" } else { "quiet_nothing" }
                    );
                    if !honest {
                        quiet_nothing += 1;
                    } else if direction == SurfaceDirection::Send {
                        send_refused += 1;
                    } else {
                        receive_refused += 1;
                    }
                }
            }
        }
    }

    let _ = std::fs::remove_dir_all(&dir);

    println!("TASK4263_TEST_SEND_REFUSED_BY_NAME_COUNT={send_refused}");
    println!("TASK4263_TEST_RECEIVE_REFUSED_BY_NAME_COUNT={receive_refused}");
    println!("TASK4263_TEST_QUIET_NOTHING_COUNT={quiet_nothing}");
    println!("TASK4263_TEST_REPORTED_SUCCESS_COUNT={reported_success}");

    assert_eq!(send_refused, 3, "3 send attempts must refuse by name");
    assert_eq!(receive_refused, 3, "3 receive attempts must refuse by name");
    assert_eq!(quiet_nothing, 0, "nothing may quietly do nothing");
    assert_eq!(reported_success, 0, "nothing may report success");
}

#[test]
fn the_protected_message_path_refuses_the_three_before_it_is_reached() {
    let dir = scratch_dir("protected-path");

    // Control first: the protected-message path is reachable in this run.
    let control = AllowedPlaceRecord::discord_direct_message("900000000000004263", "LANTERN-4263");
    add_allowed_place_record(&dir, control.clone()).expect("whitelist the control place");
    let trace = cmd_osl_trace_allowed_place_protected_message_path(
        dir.clone(),
        ProtectedPlaceAction::Send,
        control.clone(),
    )
    .expect("Discord control must reach the protected-message path");
    assert!(trace
        .iter()
        .any(|line| line.contains("protected-message path reached")));

    let mut refused = 0usize;
    for app in HALF_RESTORED_APPS {
        let record = place_for(app.app_id);
        for (action, direction) in [
            (ProtectedPlaceAction::Send, SurfaceDirection::Send),
            (ProtectedPlaceAction::Type, SurfaceDirection::Send),
            (ProtectedPlaceAction::Read, SurfaceDirection::Receive),
            (ProtectedPlaceAction::Show, SurfaceDirection::Receive),
        ] {
            let part = app.missing_part(direction);
            let words = cmd_osl_trace_allowed_place_protected_message_path(
                dir.clone(),
                action,
                record.clone(),
            )
            .expect_err("a half restored surface must never reach the protected-message path");
            println!("TASK4263_TEST_PATH app={} words={words:?}", app.display_name);
            assert!(!is_store_refusal(&words), "{words}");
            assert!(words.contains(app.display_name), "{words}");
            assert!(words.contains(part.name), "{words}");
            refused += 1;
        }
    }

    let _ = std::fs::remove_dir_all(&dir);

    println!("TASK4263_TEST_PROTECTED_PATH_REFUSED_COUNT={refused}");
    assert_eq!(refused, 12);
}

#[test]
fn the_guard_names_the_three_and_only_the_three() {
    let ids: Vec<&str> = HALF_RESTORED_APPS.iter().map(|app| app.app_id).collect();
    println!("TASK4263_TEST_APPS={}", ids.join(","));
    assert_eq!(ids, vec!["x", "instagram", "messenger"]);

    let mut refused = 0usize;
    for app in HALF_RESTORED_APPS {
        for direction in [SurfaceDirection::Send, SurfaceDirection::Receive] {
            let refusal = ipc::half_restored_surface::require_surface_ready(app.app_id, direction)
                .expect_err("must refuse");
            println!("TASK4263_TEST_GUARD {}", refusal.evidence_line());
            assert!(!refusal.missing_part.name.is_empty());
            assert!(!refusal.missing_part.gating_task.is_empty());
            refused += 1;
        }
    }
    assert_eq!(refused, 6);

    for live in ["discord", "telegram", "signal", "whatsapp", "email"] {
        assert!(require_send_ready(live).is_ok(), "{live}");
        assert!(require_receive_ready(live).is_ok(), "{live}");
    }
    for near_miss in ["X", "Instagram", "instagram ", "instagram.com", "messenger/"] {
        assert!(half_restored_app(near_miss).is_none(), "{near_miss}");
    }
}
