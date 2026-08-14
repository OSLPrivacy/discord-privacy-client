use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

const RECORDS_JSON: &str = include_str!("../../../data/allowed-places-4203.json");

#[test]
fn task4214_telegram_story_and_whatsapp_status_refuse_registration_and_open_zero_typing_boxes() {
    let story = run_probe("Telegram story", None);
    let status = run_probe("WhatsApp Status", None);

    print!("{}", story.stdout);
    eprint!("{}", story.stderr);
    print!("{}", status.stdout);
    eprint!("{}", status.stderr);

    assert_eq!(story.code, 1);
    assert_eq!(status.code, 1);
    assert!(story
        .stdout
        .contains("TASK4214_REFUSED_WORDS=telegram story"));
    assert!(status
        .stdout
        .contains("TASK4214_REFUSED_WORDS=whatsapp status"));
    assert!(story.stdout.contains("TASK4214_REGISTER_EXIT=1"));
    assert!(status.stdout.contains("TASK4214_REGISTER_EXIT=1"));
    assert!(story.stdout.contains("TASK4214_TYPING_BOXES_OPENED=0"));
    assert!(status.stdout.contains("TASK4214_TYPING_BOXES_OPENED=0"));
    assert!(story.stdout.contains(
        "TASK4214_REFUSAL=telegram story refused: research task 1029a has to answer first"
    ));
    assert!(status.stdout.contains(
        "TASK4214_REFUSAL=whatsapp status refused: research task 1089d has to answer first"
    ));

    let total_typing_boxes =
        typing_boxes_opened(&story.stdout) + typing_boxes_opened(&status.stdout);
    println!("TASK4214_TWO_ATTEMPT_TYPING_BOXES_OPENED={total_typing_boxes}");
    assert_eq!(total_typing_boxes, 0);
}

#[test]
fn task4214_breaking_a_look_only_row_makes_the_proof_go_red_then_canonical_goes_green() {
    let dir = TempDir::new().expect("temp dir");
    let records = mutated_records(&dir, |places| {
        let story = places
            .iter_mut()
            .find(|row| row["place"] == "Telegram story")
            .expect("Telegram story row exists");
        story.as_object_mut().expect("row is an object").insert(
            "disposition".to_owned(),
            serde_json::Value::String("approved".to_owned()),
        );
    });

    let broken = run_probe("Telegram story", Some(&records));
    print!("{}", broken.stdout);
    eprint!("{}", broken.stderr);

    assert_eq!(broken.code, 1);
    assert!(broken
        .stdout
        .contains("TASK4214_ERROR=telegram story is not look-only refused"));

    let restored = run_probe("Telegram story", None);
    print!("{}", restored.stdout);
    eprint!("{}", restored.stderr);

    assert_eq!(restored.code, 1);
    assert!(restored
        .stdout
        .contains("TASK4214_REFUSED_WORDS=telegram story"));
    assert!(restored.stdout.contains(
        "TASK4214_REFUSAL=telegram story refused: research task 1029a has to answer first"
    ));
    assert_eq!(typing_boxes_opened(&restored.stdout), 0);
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

fn typing_boxes_opened(stdout: &str) -> usize {
    stdout
        .lines()
        .find_map(|line| line.strip_prefix("TASK4214_TYPING_BOXES_OPENED="))
        .expect("typing-box count is printed")
        .parse()
        .expect("typing-box count is a number")
}

struct CheckOutput {
    code: i32,
    stdout: String,
    stderr: String,
}

fn run_probe(place: &str, records: Option<&Path>) -> CheckOutput {
    let mut command = Command::new(env!("CARGO_BIN_EXE_task-4214-look-only-stays-shut"));
    command.arg("--place").arg(place);
    if let Some(records) = records {
        command.arg("--records").arg(records);
    }
    let output = command.output().expect("run task 4214 checker");
    CheckOutput {
        code: output.status.code().unwrap_or(-1),
        stdout: String::from_utf8(output.stdout).expect("stdout is UTF-8"),
        stderr: String::from_utf8(output.stderr).expect("stderr is UTF-8"),
    }
}
