use std::{collections::BTreeSet, env, fs, path::PathBuf};

use osl_privacy_hub::telegram_attachment_tray::{
    TelegramAttachmentTray, TelegramAttachmentTrayError, TelegramPickedFile,
    TELEGRAM_MAX_ATTACHMENTS, TELEGRAM_MAX_ATTACHMENT_BYTES,
};
use serde::Deserialize;

const REQUIRED_INVALID_CASES: [&str; 3] = ["17-files", "over-8-MB", "folder"];

#[derive(Deserialize)]
struct Fixture {
    initial_drop: FixtureDrop,
    invalid_drop_attempts: Vec<FixtureDrop>,
}

#[derive(Deserialize)]
struct FixtureDrop {
    #[serde(default)]
    case_name: Option<String>,
    files: Vec<FixtureFile>,
}

#[derive(Deserialize)]
struct FixtureFile {
    name: String,
    media_type: String,
    size: u64,
    is_directory: bool,
}

impl From<FixtureFile> for TelegramPickedFile {
    fn from(file: FixtureFile) -> Self {
        Self {
            name: file.name,
            media_type: file.media_type,
            size: file.size,
            is_directory: file.is_directory,
        }
    }
}

fn fixture_path() -> PathBuf {
    env::var_os("OSL_TASK1024_FIXTURE").map_or_else(
        || {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/task_1024/adversarial.json")
        },
        PathBuf::from,
    )
}

fn expected_refusal(case_name: &str) -> TelegramAttachmentTrayError {
    match case_name {
        "17-files" => TelegramAttachmentTrayError::TooManyFiles {
            // One valid file is already preserved in the private box before
            // this independent 17-file drop is attempted.
            requested: 18,
            max: TELEGRAM_MAX_ATTACHMENTS,
        },
        "over-8-MB" => TelegramAttachmentTrayError::TooLarge {
            size: 8_388_609,
            max: TELEGRAM_MAX_ATTACHMENT_BYTES,
        },
        "folder" => TelegramAttachmentTrayError::FolderNotAllowed {
            name: "photos".to_owned(),
        },
        other => panic!("TASK1024 unsupported invalid case name: {other}"),
    }
}

#[test]
fn task_1024_break_telegram_attachment_limits() {
    let path = fixture_path();
    let fixture: Fixture = serde_json::from_slice(
        &fs::read(&path)
            .unwrap_or_else(|error| panic!("TASK1024 read fixture {}: {error}", path.display())),
    )
    .unwrap_or_else(|error| panic!("TASK1024 parse fixture {}: {error}", path.display()));

    let case_names = fixture
        .invalid_drop_attempts
        .iter()
        .filter_map(|attempt| attempt.case_name.as_deref())
        .collect::<BTreeSet<_>>();
    let missing_cases = REQUIRED_INVALID_CASES
        .iter()
        .filter(|case_name| !case_names.contains(**case_name))
        .copied()
        .collect::<Vec<_>>();
    println!(
        "TASK1024 fixture_required_case_assertion={}",
        missing_cases.is_empty()
    );
    assert!(
        missing_cases.is_empty(),
        "TASK1024 fixture missing required invalid case names: {}",
        missing_cases.join(",")
    );

    let mut tray = TelegramAttachmentTray::default();
    let initial = tray
        .drop(fixture.initial_drop.files.into_iter().map(Into::into))
        .expect("TASK1024 valid initial attachment must be accepted");
    assert_eq!(
        initial.len(),
        1,
        "TASK1024 fixture must begin with one valid attachment"
    );
    let initial_rows = tray.rows().to_vec();

    let sent_file_or_message_count = 0_u8;
    for attempt in fixture.invalid_drop_attempts {
        let case_name = attempt
            .case_name
            .as_deref()
            .expect("TASK1024 invalid drop attempt must have a case name");
        match case_name {
            "17-files" => {
                println!("TASK1024 attempted_count={}", attempt.files.len());
                assert_eq!(attempt.files.len(), 17);
            }
            "over-8-MB" => {
                assert_eq!(attempt.files.len(), 1);
                println!("TASK1024 oversized_byte_count={}", attempt.files[0].size);
                assert_eq!(attempt.files[0].size, 8_388_609);
            }
            "folder" => {
                assert_eq!(attempt.files.len(), 1);
                println!("TASK1024 folder_attempt={}", attempt.files[0].is_directory);
                assert!(attempt.files[0].is_directory);
            }
            other => panic!("TASK1024 unsupported invalid case name: {other}"),
        }

        let refusal = tray.drop(attempt.files.into_iter().map(Into::into));
        assert_eq!(refusal, Err(expected_refusal(case_name)));
        println!("TASK1024 refusal case={case_name}");
        assert_eq!(
            tray.rows(),
            initial_rows,
            "TASK1024 {case_name} must be atomic"
        );
        assert_eq!(
            sent_file_or_message_count, 0,
            "TASK1024 refusing a drop cannot send"
        );
    }

    let valid = &tray.rows()[0];
    println!(
        "TASK1024 final_valid_count={} final_valid_name={}",
        tray.rows().len(),
        valid.name
    );
    println!("TASK1024 sent_count={sent_file_or_message_count}");
    assert_eq!(tray.rows(), initial_rows);
    assert_eq!(sent_file_or_message_count, 0);
}
