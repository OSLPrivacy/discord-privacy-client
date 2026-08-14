use ipc::allowed_places::{
    add_allowed_place_record, list_allowed_place_records, AllowedPlaceRecord,
};
use ipc::auto_whitelist_rules::SignalWhitelistKind;
use ipc::signal_story::{
    inspect_signal_story_controls, invoke_signal_story_protection, run_signal_story_command,
    signal_story_audience_record, SignalStoryAudienceInput, SIGNAL_STORY_CONTROL_NAMES,
};
use serde_json::Value;
use std::ffi::OsString;
use tempfile::TempDir;

const ACCOUNT: &str = "signal-owner-1056";
const MAYA: &str = "signal-maya-1056";
const NOAH: &str = "signal-noah-1056";
const QUINN: &str = "signal-quinn-1056";

fn command(store: &str, audience: &str) -> (i32, Value) {
    let result = run_signal_story_command(
        [
            "--store",
            store,
            "--account",
            ACCOUNT,
            "--audience",
            audience,
        ]
        .into_iter()
        .map(OsString::from),
    );
    let value = serde_json::from_str(result.stdout.trim_end())
        .expect("direct command returns one JSON object");
    (result.exit_code, value)
}

#[test]
fn task_1056_only_exact_signal_story_audience_records_enable_controls() {
    let store = TempDir::new().expect("create Task 1056 allowed-place store");
    let input = SignalStoryAudienceInput {
        account: ACCOUNT.to_owned(),
        selected_audience: vec![MAYA.to_owned()],
        stories_enabled: true,
    };

    let before = inspect_signal_story_controls(store.path(), &input)
        .expect("inspect controls before allowance");
    assert_eq!(before.status, "unavailable");
    assert!(!before.available);
    assert_eq!(before.control_names.len(), 0);
    assert_eq!(before.unallowed_audience, [MAYA]);

    let before_refusal = invoke_signal_story_protection(store.path(), &input)
        .expect_err("direct invoke refuses before the selected audience is allowed");
    assert_eq!(
        before_refusal,
        format!("Signal story audience member is not allowed: {MAYA}")
    );

    let distractors = [
        AllowedPlaceRecord::signal(ACCOUNT, SignalWhitelistKind::Story, NOAH),
        AllowedPlaceRecord::signal("signal-other-owner-1056", SignalWhitelistKind::Story, MAYA),
        AllowedPlaceRecord::signal(ACCOUNT, SignalWhitelistKind::DirectMessage, MAYA),
    ];
    for distractor in &distractors {
        add_allowed_place_record(store.path(), distractor)
            .expect("add nonmatching allowed-place record");
        let unchanged = inspect_signal_story_controls(store.path(), &input)
            .expect("inspect after nonmatching record");
        assert_eq!(unchanged.status, "unavailable");
        assert_eq!(unchanged.control_names.len(), 0);
    }

    let exact = signal_story_audience_record(ACCOUNT, MAYA)
        .expect("build exact Signal story audience record");
    add_allowed_place_record(store.path(), &exact).expect("allow exact Signal story audience");
    let after = inspect_signal_story_controls(store.path(), &input)
        .expect("inspect controls after exact allowance");
    assert_eq!(after.status, "available");
    assert!(after.available);
    assert_eq!(after.control_names, SIGNAL_STORY_CONTROL_NAMES);
    assert!(after.unallowed_audience.is_empty());

    let direct = invoke_signal_story_protection(store.path(), &input)
        .expect("direct invoke accepts exact allowed audience");
    assert_eq!(direct.audience_count, 1);
    assert_eq!(direct.control_names.len(), 7);

    let store_path = store.path().to_string_lossy();
    let (allowed_exit, allowed_command) = command(&store_path, MAYA);
    assert_eq!(allowed_exit, 0);
    assert_eq!(allowed_command["status"], "available");
    assert_eq!(allowed_command["audienceCount"], 1);

    let (unallowed_exit, unallowed_command) = command(&store_path, &format!("{MAYA},{QUINN}"));
    assert_eq!(unallowed_exit, 2);
    assert_eq!(
        unallowed_command["error"],
        format!("Signal story audience member is not allowed: {QUINN}")
    );

    let record_count = list_allowed_place_records(store.path())
        .expect("list Task 1056 records")
        .len();
    assert_eq!(record_count, 4);

    println!("TASK1056_CONTROLS_BEFORE={}", before.control_names.len());
    println!("TASK1056_DIRECT_BEFORE_REFUSAL={before_refusal}");
    println!("TASK1056_DISTRACTOR_RECORDS={}", distractors.len());
    println!("TASK1056_DISTRACTOR_CONTROLS=0");
    println!("TASK1056_EXACT_STABLE_ID={}", exact.stable_id);
    println!("TASK1056_CONTROLS_AFTER={}", after.control_names.len());
    println!("TASK1056_ALLOWED_COMMAND_EXIT={allowed_exit}");
    println!("TASK1056_ALLOWED_COMMAND_AUDIENCE_COUNT=1");
    println!("TASK1056_UNALLOWED_COMMAND_EXIT={unallowed_exit}");
    println!(
        "TASK1056_UNALLOWED_COMMAND_ERROR={}",
        unallowed_command["error"]
    );
}
