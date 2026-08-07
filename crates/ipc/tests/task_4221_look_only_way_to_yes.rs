use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

const TODO_DIR: &str = "/home/liamw/osl-plan/OSL-AUDITS/todo";
const RECORDS_JSON: &str = include_str!("../../../data/allowed-places-4203.json");

#[test]
fn task4221_real_look_only_rows_have_live_research_and_build_tasks() {
    let output = run_checker(None);

    print!("{}", output.stdout);
    eprint!("{}", output.stderr);

    assert_eq!(output.code, 0);
    assert!(output.stdout.contains("TASK4221_LOOK_ONLY_ROWS=2"));
    assert!(output
        .stdout
        .contains("TASK4221_LOOK_ONLY_ROWS_WITH_EXISTING_RESEARCH_AND_BUILD=2"));
    assert!(output
        .stdout
        .contains("TASK4221_REFUSED_ROWS_WITH_NO_NAMED_WAY_TO_YES=0"));
    assert!(output
        .stdout
        .contains("TASK4221_REFUSED_ROWS_WITH_DONE_BUILD_TASK=0"));
    assert!(output.stdout.contains(
        "TASK4221_LOOK_ONLY_ROW place=\"Telegram story\" research_task=1029a research_task_exists=true research_task_done=false build_task=4218 build_task_exists=true build_task_done=false"
    ));
    assert!(output.stdout.contains(
        "TASK4221_LOOK_ONLY_ROW place=\"WhatsApp Status\" research_task=1089d research_task_exists=true research_task_done=false build_task=4220 build_task_exists=true build_task_done=false"
    ));
}

#[test]
fn task4221_blank_build_task_names_whatsapp_status_with_no_way_to_yes() {
    let dir = TempDir::new().expect("temp dir");
    let records = mutated_records(&dir, |places| {
        let status = places
            .iter_mut()
            .find(|row| row["place"] == "WhatsApp Status")
            .expect("WhatsApp Status row exists");
        status.as_object_mut().expect("row is an object").insert(
            "build_task".to_owned(),
            serde_json::Value::String(String::new()),
        );
    });
    let output = run_checker(Some(&records));

    print!("{}", output.stdout);
    eprint!("{}", output.stderr);

    assert_eq!(output.code, 1);
    assert!(output
        .stdout
        .contains("TASK4221_REFUSED_ROW=whatsapp status with no way to yes"));
    assert!(output
        .stdout
        .contains("TASK4221_REFUSED_ROWS_WITH_NO_NAMED_WAY_TO_YES=1"));
}

#[test]
fn task4221_missing_task_number_exits_one_and_names_the_number() {
    let dir = TempDir::new().expect("temp dir");
    let records = mutated_records(&dir, |places| {
        let story = places
            .iter_mut()
            .find(|row| row["place"] == "Telegram story")
            .expect("Telegram story row exists");
        story.as_object_mut().expect("row is an object").insert(
            "research_task".to_owned(),
            serde_json::Value::String("999999".to_owned()),
        );
    });
    let output = run_checker(Some(&records));

    print!("{}", output.stdout);
    eprint!("{}", output.stderr);

    assert_eq!(output.code, 1);
    assert!(output
        .stdout
        .contains("TASK4221_NAMED_TASK_NOT_FOUND=999999"));
    assert!(output.stdout.contains("TASK4221_MISSING_TASK_NUMBERS=1"));
}

#[test]
fn task4221_done_build_task_keeps_refused_row_red() {
    let dir = TempDir::new().expect("temp dir");
    let records = mutated_records(&dir, |places| {
        let story = places
            .iter_mut()
            .find(|row| row["place"] == "Telegram story")
            .expect("Telegram story row exists");
        story.as_object_mut().expect("row is an object").insert(
            "build_task".to_owned(),
            serde_json::Value::String("4203".to_owned()),
        );
    });
    let output = run_checker(Some(&records));

    print!("{}", output.stdout);
    eprint!("{}", output.stderr);

    assert_eq!(output.code, 1);
    assert!(output
        .stdout
        .contains("TASK4221_REFUSED_ROWS_WITH_DONE_BUILD_TASK=1"));
}

fn mutated_records(dir: &TempDir, mutate: impl FnOnce(&mut Vec<serde_json::Value>)) -> PathBuf {
    let mut value: serde_json::Value =
        serde_json::from_str(RECORDS_JSON).expect("canonical records JSON parses");
    let places = value["places"].as_array_mut().expect("places is an array");
    mutate(places);

    let path = dir.path().join("allowed-places-4203.json");
    std::fs::write(
        &path,
        serde_json::to_vec_pretty(&value).expect("mutated JSON serializes"),
    )
    .expect("write mutated records");
    path
}

struct CheckOutput {
    code: i32,
    stdout: String,
    stderr: String,
}

fn run_checker(records: Option<&Path>) -> CheckOutput {
    let mut command = Command::new(env!("CARGO_BIN_EXE_task-4221-look-only-way-to-yes"));
    command.arg("--todo").arg(TODO_DIR);
    if let Some(records) = records {
        command.arg("--records").arg(records);
    }
    let output = command.output().expect("run task 4221 checker");
    CheckOutput {
        code: output.status.code().unwrap_or(-1),
        stdout: String::from_utf8(output.stdout).expect("stdout is UTF-8"),
        stderr: String::from_utf8(output.stderr).expect("stderr is UTF-8"),
    }
}
