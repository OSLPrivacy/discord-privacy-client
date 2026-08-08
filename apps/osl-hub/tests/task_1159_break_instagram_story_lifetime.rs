#![cfg(feature = "core")]

use osl_privacy_hub::allowed_place_commands::{run_allowed_place_cli, HeadlessCommandResult};
use osl_privacy_hub::instagram_story::instagram_story_audience_stable_id;
use serde_json::Value;
use std::ffi::OsString;

const ACCOUNT: &str = "instagram-owner-1159";
const AUDIENCE_MEMBER: &str = "instagram-reader-1159";
const PUBLISHED_AT: i64 = 1_800_115_900;
const REQUESTED_LIFE_HOURS: i64 = 25;
const MAXIMUM_VISIBLE_LIFE_HOURS: i64 = 24;

trait InstagramStoryReader {
    fn read_command_report(&mut self, command: &HeadlessCommandResult) -> Option<Value>;
}

struct CommandStdoutInstagramStoryReader;

impl InstagramStoryReader for CommandStdoutInstagramStoryReader {
    fn read_command_report(&mut self, command: &HeadlessCommandResult) -> Option<Value> {
        serde_json::from_str(command.stdout.trim_end()).ok()
    }
}

fn run_command(arguments: Vec<String>) -> HeadlessCommandResult {
    run_allowed_place_cli(arguments.into_iter().map(OsString::from))
        .expect("allowed-place command is recognized")
}

fn allow_story_audience_member(store: &str) {
    let result = run_command(vec![
        "osl-privacy-hub".to_owned(),
        "--allowed-place".to_owned(),
        "add".to_owned(),
        "--store".to_owned(),
        store.to_owned(),
        "--app".to_owned(),
        "instagram".to_owned(),
        "--account".to_owned(),
        ACCOUNT.to_owned(),
        "--kind".to_owned(),
        "public_post".to_owned(),
        "--stable-id".to_owned(),
        instagram_story_audience_stable_id(ACCOUNT, AUDIENCE_MEMBER),
    ]);
    assert_eq!(
        result.exit_code, 0,
        "allow command failed: {}",
        result.stdout
    );
}

#[test]
fn task_1159_a_25_hour_request_reports_a_24_hour_maximum_visible_story_life() {
    let store_dir = tempfile::tempdir().expect("create task 1159 allowed-place store");
    let store = store_dir.path().to_string_lossy();
    allow_story_audience_member(&store);

    let requested_expires_at = PUBLISHED_AT + REQUESTED_LIFE_HOURS * 60 * 60;
    let instagram_expires_at = PUBLISHED_AT + MAXIMUM_VISIBLE_LIFE_HOURS * 60 * 60;
    let result = run_command(vec![
        "osl-privacy-hub".to_owned(),
        "--allowed-place".to_owned(),
        "instagram-story-publish".to_owned(),
        "--store".to_owned(),
        store.into_owned(),
        "--account".to_owned(),
        ACCOUNT.to_owned(),
        "--audience".to_owned(),
        AUDIENCE_MEMBER.to_owned(),
        "--published-at".to_owned(),
        PUBLISHED_AT.to_string(),
        "--osl-expires-at".to_owned(),
        requested_expires_at.to_string(),
        "--effective-expires-at".to_owned(),
        instagram_expires_at.to_string(),
    ]);
    assert_eq!(
        result.exit_code, 0,
        "story command failed: {}",
        result.stdout
    );

    let mut reader = CommandStdoutInstagramStoryReader;
    let report = reader
        .read_command_report(&result)
        .expect("Instagram story reader returned no command report");
    assert_eq!(report["command"], "instagramStoryPublish");
    assert_eq!(
        report["maximumVisibleStoryLifeHours"],
        MAXIMUM_VISIBLE_LIFE_HOURS
    );
    assert_eq!(report["effectiveExpiresAt"], instagram_expires_at);
    assert_eq!(report["instagramExpiresAt"], instagram_expires_at);
    assert_eq!(report["expirySource"], "instagram_24_hours");

    println!("TASK1159_REQUESTED_OSL_STORY_LIFE_HOURS={REQUESTED_LIFE_HOURS}");
    println!(
        "TASK1159_COMMAND_MAXIMUM_VISIBLE_STORY_LIFE_HOURS={}",
        report["maximumVisibleStoryLifeHours"]
    );
    println!(
        "TASK1159_COMMAND_EFFECTIVE_VISIBLE_STORY_LIFE_HOURS={}",
        (report["effectiveExpiresAt"]
            .as_i64()
            .expect("integer effective expiry")
            - PUBLISHED_AT)
            / 3_600
    );
    println!("TASK1159_COMMAND_EXPIRY_SOURCE={}", report["expirySource"]);
}
