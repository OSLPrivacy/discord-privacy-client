use ipc::allowed_places::AllowedPlaceRecord;
use ipc::commands::{cmd_osl_new_place, cmd_osl_save_auto_whitelist_rule};
use ipc::state::AppState;

#[test]
fn direct_new_place_command_returns_pending_allow_request_for_ask_me_rule() {
    let state = AppState::new();
    cmd_osl_save_auto_whitelist_rule(&state, "discord".to_string(), "ask me".to_string(), None)
        .expect("save ask-me rule");

    let place =
        AllowedPlaceRecord::discord_direct_message("900000000000013601", "900000000000013602");
    let decision = cmd_osl_new_place(&state, place.clone(), None).expect("new place decision");
    let allow_request = decision
        .allow_request
        .as_ref()
        .expect("ask-me rule returns pending allow request");

    println!(
        "TASK_0136_NEW_PLACE command=cmd_osl_new_place rule={} status={} prompt={} request_id={} choices={}",
        decision.rule,
        decision.status,
        decision.prompt,
        allow_request.request_id,
        allow_request.allowed_choices.join(",")
    );

    assert_eq!(decision.place, place);
    assert_eq!(decision.rule, "ask me");
    assert_eq!(decision.status, "pending_allow_request");
    assert!(decision.prompt);
    assert!(allow_request.choice_required);
    assert_eq!(allow_request.allowed_choices, vec!["allow", "deny"]);
}
