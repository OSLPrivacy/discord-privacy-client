use osl_privacy_hub::burn_review_state::{
    BurnReviewState, BurnScreenState, BurnScreenStateCommand,
};
use tempfile::TempDir;

#[test]
fn task_0544_each_named_burn_screen_command_persists_fake_review_state() {
    let temp = TempDir::new().expect("temporary review directory");
    let state_path = temp.path().join("burn-review-state.json");
    let review = BurnReviewState::load(state_path.clone());

    let cases = [
        (
            "chat",
            "after reading",
            "Chat burn removes this local OSL chat.",
        ),
        (
            "app",
            "after this session",
            "App burn removes indexed local OSL data.",
        ),
        (
            "account",
            "immediately",
            "Account burn removes this local OSL account.",
        ),
    ];

    for (scope, selected_burn_time, warning_text) in cases {
        let saved = review
            .save_burn_screen_command(BurnScreenStateCommand {
                scope: scope.to_owned(),
                selected_burn_time: selected_burn_time.to_owned(),
                warning_text: warning_text.to_owned(),
            })
            .expect("valid named screen command saves fake review state");
        assert_eq!(
            saved,
            BurnScreenState {
                selected_burn_time: selected_burn_time.to_owned(),
                warning_text: warning_text.to_owned(),
            }
        );
        assert_eq!(
            review
                .get_burn_screen_command(scope)
                .expect("valid named screen reads"),
            Some(saved)
        );
    }

    let reloaded = BurnReviewState::load(state_path);
    for (scope, selected_burn_time, warning_text) in cases {
        let saved = reloaded
            .get_burn_screen_command(scope)
            .expect("persisted named screen reads")
            .expect("named screen state is present");
        println!(
            "TASK0544 scope={scope} selected_burn_time={} warning_text={}",
            saved.selected_burn_time, saved.warning_text
        );
        assert_eq!(saved.selected_burn_time, selected_burn_time);
        assert_eq!(saved.warning_text, warning_text);
    }

    let refusal = reloaded
        .get_burn_screen_command("unknown")
        .expect_err("unknown scope is refused");
    println!("TASK0544 unknown_scope_error={refusal}");
    assert_eq!(refusal, "unknown burn screen scope: unknown");
}
