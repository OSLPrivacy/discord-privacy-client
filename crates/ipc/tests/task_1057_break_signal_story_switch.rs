use ipc::allowed_places::add_allowed_place_record;
use ipc::signal_story::{
    invoke_signal_story_protection, signal_story_audience_record, SignalStoryAudienceInput,
};
use tempfile::TempDir;

const STORY: &str = "signal-story-1057";
const ACCOUNT: &str = "signal-owner-1057";
const AUDIENCE_MEMBER: &str = "signal-audience-1057";
const STORY_TIMER_CONTROL: &str = "story timer";

fn one_story_timer_control(store: &TempDir, input: &SignalStoryAudienceInput) -> Vec<&'static str> {
    let receipt = invoke_signal_story_protection(store.path(), input)
        .expect("enabled allowed Signal story exposes its OSL controls");

    receipt
        .control_names
        .into_iter()
        .filter(|name| *name == "timer")
        .map(|_| STORY_TIMER_CONTROL)
        .collect()
}

#[test]
fn task_1057_refuses_stories_off_then_returns_the_same_good_story_timer() {
    let store = TempDir::new().expect("create Task 1057 allowed-place store");
    let audience_record = signal_story_audience_record(ACCOUNT, AUDIENCE_MEMBER)
        .expect("build exact Task 1057 Signal story audience record");
    add_allowed_place_record(store.path(), &audience_record)
        .expect("allow the exact Task 1057 Signal story audience");

    let good_input = SignalStoryAudienceInput {
        account: ACCOUNT.to_owned(),
        selected_audience: vec![AUDIENCE_MEMBER.to_owned()],
        stories_enabled: true,
    };
    let good = one_story_timer_control(&store, &good_input);
    assert_eq!(good, [STORY_TIMER_CONTROL]);
    assert_eq!(
        good.len(),
        1,
        "good story must yield exactly one timer control"
    );
    println!(
        "TASK1057_GOOD story={STORY} control_count={} control_name={}",
        good.len(),
        good[0]
    );

    let stories_off_input = SignalStoryAudienceInput {
        stories_enabled: false,
        ..good_input.clone()
    };
    let refusal = invoke_signal_story_protection(store.path(), &stories_off_input)
        .expect_err("Stories state off must refuse a direct story-control request");
    assert_eq!(refusal, "Signal Stories are disabled");
    println!("TASK1057_REFUSED stories_state=off refused_by_name={refusal}");

    let unchanged = one_story_timer_control(&store, &good_input);
    assert_eq!(
        unchanged, good,
        "the refused state must not change the good story"
    );
    assert_eq!(unchanged.len(), 1);
    println!(
        "TASK1057_UNCHANGED story={STORY} control_count={} control_name={}",
        unchanged.len(),
        unchanged[0]
    );
}
