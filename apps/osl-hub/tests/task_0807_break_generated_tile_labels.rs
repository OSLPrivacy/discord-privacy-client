use std::process::{Command, Stdio};

use osl_privacy_hub::models::ServiceKind;
use osl_privacy_hub::services::{generated_tile_label, ServiceCapabilityFacts};

const TASK0807_INVENTED_LABEL_MARKER: &str = "TASK0807_INVENTED_GENERATED_LABEL=";

#[derive(Debug, Clone, Copy)]
struct TestTileData {
    id: &'static str,
    capability: ServiceCapabilityFacts,
}

fn impossible_capability_tile() -> TestTileData {
    TestTileData {
        id: "task-0807-impossible-capability-tile",
        capability: ServiceCapabilityFacts {
            service_id: ServiceKind::Email,
            placing: true,
            reading: false,
            opening: false,
            real_two_person_protected_messaging: false,
        },
    }
}

fn checked_generated_tile_label(facts: ServiceCapabilityFacts) -> Result<&'static str, String> {
    match (
        facts.placing,
        facts.reading,
        facts.opening,
        facts.real_two_person_protected_messaging,
    ) {
        (true, true, true, true) => Ok(generated_tile_label(facts)),
        (true, false, true, false) => Ok(generated_tile_label(facts)),
        (false, true, true, false) => Ok(generated_tile_label(facts)),
        (false, false, true, false) => Ok(generated_tile_label(facts)),
        (false, false, false, false) => Ok(generated_tile_label(facts)),
        (placing, reading, opening, real_two_person_protected_messaging) => Err(format!(
            "unknown capability shape: placing={placing}, reading={reading}, opening={opening}, real_two_person_protected_messaging={real_two_person_protected_messaging}"
        )),
    }
}

#[test]
fn task_0807_checked_generator_entrypoint() {
    if std::env::var_os("OSL_TASK_0807_CHILD_GENERATOR").is_none() {
        return;
    }

    let tile = impossible_capability_tile();
    match checked_generated_tile_label(tile.capability) {
        Ok(label) => {
            println!("{TASK0807_INVENTED_LABEL_MARKER}{label}");
            std::process::exit(0);
        }
        Err(error) => {
            eprintln!(
                "TASK0807_UNKNOWN_CAPABILITY_ERROR=tile {}: {error}",
                tile.id
            );
            std::process::exit(1);
        }
    }
}

#[test]
fn task_0807_unknown_capability_shape_makes_generator_exit_1() {
    for (facts, expected) in [
        ((true, true, true, true), "Ready"),
        ((true, false, true, false), "Placing only"),
        ((false, true, true, false), "Reading only"),
        ((false, false, true, false), "Opens the app"),
        ((false, false, false, false), "Not started"),
    ] {
        let facts = ServiceCapabilityFacts {
            service_id: ServiceKind::Email,
            placing: facts.0,
            reading: facts.1,
            opening: facts.2,
            real_two_person_protected_messaging: facts.3,
        };
        assert_eq!(checked_generated_tile_label(facts), Ok(expected));
    }

    let current_exe = std::env::current_exe().expect("test binary path is available");
    let output = Command::new(current_exe)
        .env("OSL_TASK_0807_CHILD_GENERATOR", "1")
        .arg("--exact")
        .arg("task_0807_checked_generator_entrypoint")
        .arg("--nocapture")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("child checked generator runs");
    let exit = output.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    println!("TASK0807_GENERATOR_EXIT={exit}");

    assert_eq!(exit, 1);
    assert!(
        stderr.contains("TASK0807_UNKNOWN_CAPABILITY_ERROR")
            && stderr.contains("placing=true")
            && stderr.contains("reading=false")
            && stderr.contains("opening=false")
            && stderr.contains("real_two_person_protected_messaging=false"),
        "child generator must describe the unknown capability facts, stderr was: {stderr}"
    );
    assert!(
        !stdout.contains(TASK0807_INVENTED_LABEL_MARKER),
        "child must not invent a generated-label output, stdout was: {stdout}"
    );
    for label in [
        "Ready",
        "Placing only",
        "Reading only",
        "Opens the app",
        "Not started",
    ] {
        assert!(
            !stdout.contains(label),
            "child must not emit the {label:?} label, stdout was: {stdout}"
        );
    }
}
