use std::process::{Command, Stdio};

use osl_privacy_hub::services::{
    generated_tile_label, service_capability_facts, ServiceCapabilityFacts,
};

#[derive(Debug, Clone, Copy)]
struct TestTileData {
    id: &'static str,
    label: &'static str,
    capability: ServiceCapabilityFacts,
}

fn check_tile_data(tile: TestTileData) -> Result<(), String> {
    let generated = generated_tile_label(tile.capability);
    if tile.label == generated {
        Ok(())
    } else {
        Err(format!(
            "tile {} has hand-written label {:?}; capability facts generate {:?}",
            tile.id, tile.label, generated
        ))
    }
}

fn opening_only_tile(label: &'static str) -> TestTileData {
    let capability = service_capability_facts("email").expect("Email has a service capability row");
    assert!(!capability.placing);
    assert!(!capability.reading);
    assert!(capability.opening);
    assert!(!capability.real_two_person_protected_messaging);
    TestTileData {
        id: "email",
        label,
        capability,
    }
}

#[test]
fn task_0803_tile_data_check_entrypoint() {
    if std::env::var_os("OSL_TASK_0803_CHILD_CHECK").is_none() {
        return;
    }

    let tile = opening_only_tile("Ready");
    match check_tile_data(tile) {
        Ok(()) => std::process::exit(0),
        Err(error) => {
            eprintln!("TASK0803_TILE_DATA_CHECK_ERROR={error}");
            std::process::exit(1);
        }
    }
}

#[test]
fn task_0803_hand_written_ready_label_makes_tile_data_check_exit_1() {
    let generated = generated_tile_label(opening_only_tile("Opens the app").capability);
    println!("TASK0803_OPENING_ONLY_GENERATED_LABEL={generated}");
    assert_eq!(generated, "Opens the app");

    let valid_tile = opening_only_tile(generated);
    check_tile_data(valid_tile).expect("generated label should pass the tile-data check");

    let current_exe = std::env::current_exe().expect("test binary path is available");
    let output = Command::new(current_exe)
        .env("OSL_TASK_0803_CHILD_CHECK", "1")
        .arg("--exact")
        .arg("task_0803_tile_data_check_entrypoint")
        .arg("--nocapture")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("child tile-data check runs");
    let exit = output.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    println!("TASK0803_TILE_DATA_CHECK_EXIT={exit}");
    if !stdout.trim().is_empty() {
        println!("{stdout}");
    }
    if !stderr.trim().is_empty() {
        eprintln!("{stderr}");
    }

    assert_eq!(exit, 1);
    assert!(
        stderr.contains("hand-written label \"Ready\"")
            && stderr.contains("capability facts generate \"Opens the app\""),
        "child check must fail for the hand-written Ready label, stderr was: {stderr}"
    );
}
