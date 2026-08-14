#![cfg(feature = "core")]

use osl_privacy_hub::allowed_place_commands::{
    instagram_story_publish_control_json, publish_instagram_story_json, run_allowed_place_cli,
    HeadlessCommandResult,
};
use osl_privacy_hub::instagram_story::{
    instagram_story_audience_stable_id, instagram_story_publish_control_available,
    InstagramStoryPublishInput, INSTAGRAM_STORY_LIFETIME_SECONDS,
};
use serde_json::Value;
use std::ffi::OsString;
use tempfile::TempDir;

const ACCOUNT: &str = "instagram-owner-1157";
const MAYA: &str = "instagram-maya-1157";
const NOAH: &str = "instagram-noah-1157";
const QUINN: &str = "instagram-quinn-1157";
const PUBLISHED_AT: i64 = 1_800_000_000;
const OSL_SHORT_EXPIRY: i64 = PUBLISHED_AT + 3_600;
const INSTAGRAM_EXPIRY: i64 = PUBLISHED_AT + INSTAGRAM_STORY_LIFETIME_SECONDS;
const OSL_LONG_EXPIRY: i64 = PUBLISHED_AT + 172_800;

fn command(arguments: Vec<String>) -> (i32, Value) {
    let HeadlessCommandResult { exit_code, stdout } =
        run_allowed_place_cli(arguments.into_iter().map(OsString::from))
            .expect("allowed-place command is recognized");
    let value = serde_json::from_str(stdout.trim_end())
        .expect("allowed-place command returns one JSON object");
    (exit_code, value)
}

fn add_allowed_member(store: &str, member: &str) {
    let stable_id = instagram_story_audience_stable_id(ACCOUNT, member);
    let (exit_code, value) = command(vec![
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
        stable_id,
    ]);
    assert_eq!(exit_code, 0, "allow command failed: {value}");
    assert_eq!(value["ok"], true);
}

fn story_input(osl_expires_at: i64, effective_expires_at: i64) -> InstagramStoryPublishInput {
    InstagramStoryPublishInput {
        account: ACCOUNT.to_owned(),
        selected_audience: vec![MAYA.to_owned(), NOAH.to_owned()],
        published_at: PUBLISHED_AT,
        osl_expires_at,
        presented_effective_expires_at: Some(effective_expires_at),
    }
}

fn publish_command(
    store: &str,
    audience: &str,
    osl_expires_at: i64,
    effective_expires_at: i64,
) -> (i32, Value) {
    command(vec![
        "osl-privacy-hub".to_owned(),
        "--allowed-place".to_owned(),
        "instagram-story-publish".to_owned(),
        "--store".to_owned(),
        store.to_owned(),
        "--account".to_owned(),
        ACCOUNT.to_owned(),
        "--audience".to_owned(),
        audience.to_owned(),
        "--published-at".to_owned(),
        PUBLISHED_AT.to_string(),
        "--osl-expires-at".to_owned(),
        osl_expires_at.to_string(),
        "--effective-expires-at".to_owned(),
        effective_expires_at.to_string(),
    ])
}

#[test]
fn task_1157_story_publish_requires_exact_audience_and_earlier_expiry() {
    let dir = TempDir::new().expect("create task 1157 allowed-place store");
    let store = dir.path().to_string_lossy();
    let exact_short = story_input(OSL_SHORT_EXPIRY, OSL_SHORT_EXPIRY);
    let wrong_short = story_input(OSL_SHORT_EXPIRY, OSL_SHORT_EXPIRY + 1);

    let false_false = instagram_story_publish_control_json(dir.path(), &wrong_short)
        .expect("inspect initial unavailable control");
    assert!(!false_false.allowed_audience);
    assert!(!false_false.earlier_expiry_matches);
    assert!(!false_false.available);
    assert_eq!(false_false.publish_control, "unavailable");

    let false_true = instagram_story_publish_control_json(dir.path(), &exact_short)
        .expect("inspect exact expiry before audience allowance");
    assert!(!false_true.allowed_audience);
    assert!(false_true.earlier_expiry_matches);
    assert!(!false_true.available);

    let direct_before = publish_instagram_story_json(dir.path(), &exact_short)
        .expect_err("direct invoke must refuse an unallowed selected audience");
    assert_eq!(
        direct_before,
        format!("Instagram story audience member is not allowed: {MAYA}")
    );

    add_allowed_member(&store, MAYA);
    let one_of_two = instagram_story_publish_control_json(dir.path(), &exact_short)
        .expect("inspect partially allowed audience");
    assert_eq!(one_of_two.unallowed_audience, [NOAH]);
    assert!(!one_of_two.available);

    add_allowed_member(&store, NOAH);
    let true_false = instagram_story_publish_control_json(dir.path(), &wrong_short)
        .expect("inspect allowed audience with wrong effective expiry");
    assert!(true_false.allowed_audience);
    assert!(!true_false.earlier_expiry_matches);
    assert!(!true_false.available);

    let wrong_expiry_direct = publish_instagram_story_json(dir.path(), &wrong_short)
        .expect_err("direct invoke must refuse a non-earlier effective expiry");
    assert_eq!(
        wrong_expiry_direct,
        format!(
            "Instagram story effective expiry must equal the earlier expiry: {OSL_SHORT_EXPIRY}"
        )
    );

    let true_true = instagram_story_publish_control_json(dir.path(), &exact_short)
        .expect("inspect complete publish condition");
    assert!(true_true.allowed_audience);
    assert!(true_true.earlier_expiry_matches);
    assert!(true_true.available);
    assert_eq!(true_true.publish_control, "available");
    let direct_receipt = serde_json::to_value(
        publish_instagram_story_json(dir.path(), &exact_short)
            .expect("direct invoke accepts the complete condition"),
    )
    .expect("serialize direct receipt");
    assert_eq!(direct_receipt["effectiveExpiresAt"], OSL_SHORT_EXPIRY);

    let truth_table = [
        instagram_story_publish_control_available(false, false),
        instagram_story_publish_control_available(false, true),
        instagram_story_publish_control_available(true, false),
        instagram_story_publish_control_available(true, true),
    ];
    assert_eq!(truth_table, [false, false, false, true]);
    assert_eq!(
        truth_table.iter().filter(|available| **available).count(),
        1
    );

    let (short_exit, short_command) = publish_command(
        &store,
        &format!("{MAYA},{NOAH}"),
        OSL_SHORT_EXPIRY,
        OSL_SHORT_EXPIRY,
    );
    assert_eq!(short_exit, 0);
    assert_eq!(short_command["command"], "instagramStoryPublish");
    assert_eq!(short_command["effectiveExpiresAt"], OSL_SHORT_EXPIRY);
    assert_eq!(short_command["expirySource"], "osl_timer");

    let (capped_exit, capped_command) = publish_command(
        &store,
        &format!("{MAYA},{NOAH}"),
        OSL_LONG_EXPIRY,
        INSTAGRAM_EXPIRY,
    );
    assert_eq!(capped_exit, 0);
    assert_eq!(capped_command["effectiveExpiresAt"], INSTAGRAM_EXPIRY);
    assert_eq!(capped_command["expirySource"], "instagram_24_hours");

    let (unallowed_exit, unallowed_command) = publish_command(
        &store,
        &format!("{MAYA},{NOAH},{QUINN}"),
        OSL_SHORT_EXPIRY,
        OSL_SHORT_EXPIRY,
    );
    assert_eq!(unallowed_exit, 2);
    assert_eq!(
        unallowed_command["error"],
        format!("Instagram story audience member is not allowed: {QUINN}")
    );

    println!(
        "TASK1157_FALSE_FALSE_CONTROL={}",
        false_false.publish_control
    );
    println!("TASK1157_FALSE_TRUE_CONTROL={}", false_true.publish_control);
    println!("TASK1157_TRUE_FALSE_CONTROL={}", true_false.publish_control);
    println!("TASK1157_TRUE_TRUE_CONTROL={}", true_true.publish_control);
    println!("TASK1157_AVAILABLE_CONDITION_COUNT=1");
    println!("TASK1157_DIRECT_BEFORE_REFUSAL={direct_before}");
    println!("TASK1157_DIRECT_WRONG_EXPIRY_REFUSAL={wrong_expiry_direct}");
    println!(
        "TASK1157_OSL_EARLIER_EXPIRY={}",
        short_command["effectiveExpiresAt"]
    );
    println!(
        "TASK1157_OSL_EARLIER_SOURCE={}",
        short_command["expirySource"]
    );
    println!(
        "TASK1157_INSTAGRAM_EARLIER_EXPIRY={}",
        capped_command["effectiveExpiresAt"]
    );
    println!(
        "TASK1157_INSTAGRAM_EARLIER_SOURCE={}",
        capped_command["expirySource"]
    );
    println!("TASK1157_UNALLOWED_COMMAND_EXIT={unallowed_exit}");
    println!(
        "TASK1157_UNALLOWED_COMMAND_ERROR={}",
        unallowed_command["error"]
    );
}
