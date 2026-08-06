use ipc::commands::{
    cmd_osl_get_auto_whitelist_rule_choices, cmd_osl_new_place, cmd_osl_read_auto_whitelist_rule,
    cmd_osl_save_auto_whitelist_rule,
};
use ipc::state::AppState;

#[test]
fn direct_rule_query_prints_each_valid_choice() {
    let choices = cmd_osl_get_auto_whitelist_rule_choices().unwrap();
    let labels: Vec<String> = choices.into_iter().map(|choice| choice.label).collect();
    println!("direct rule query valid choices: {}", labels.join(", "));
    assert_eq!(
        labels,
        vec!["never", "ask me", "always", "only if a friend"]
    );
}

#[test]
fn save_and_read_returns_different_choices_per_app_kind() {
    let state = AppState::new();

    cmd_osl_save_auto_whitelist_rule(&state, "discord".to_string(), "always".to_string(), None)
        .unwrap();
    cmd_osl_save_auto_whitelist_rule(
        &state,
        "telegram".to_string(),
        "only if a friend".to_string(),
        None,
    )
    .unwrap();

    let discord = cmd_osl_read_auto_whitelist_rule(&state, "discord".to_string()).unwrap();
    let telegram = cmd_osl_read_auto_whitelist_rule(&state, "telegram".to_string()).unwrap();

    println!(
        "two app kinds saved choices: {}={}, {}={}",
        discord.app_kind, discord.choice, telegram.app_kind, telegram.choice
    );
    assert_eq!(discord.choice, "always");
    assert_eq!(telegram.choice, "only if a friend");
    assert_ne!(discord.choice, telegram.choice);
}

#[test]
fn direct_new_place_command_returns_unlisted_with_no_prompt_when_rule_is_never() {
    let state = AppState::new();
    cmd_osl_save_auto_whitelist_rule(&state, "discord".to_string(), "always".to_string(), None)
        .unwrap();
    let non_never_place = ipc::allowed_places::AllowedPlaceRecord::discord_direct_message(
        "900000000000000135",
        "900000000000000136",
    );
    let non_never = cmd_osl_new_place(&state, non_never_place).unwrap_err();
    assert!(non_never.contains("always"));

    cmd_osl_save_auto_whitelist_rule(&state, "discord".to_string(), "never".to_string(), None)
        .unwrap();
    let never_place = ipc::allowed_places::AllowedPlaceRecord::discord_direct_message(
        "900000000000000135",
        "900000000000000136",
    );
    let result = cmd_osl_new_place(&state, never_place).unwrap();

    println!(
        "TASK0135 direct_new_place rule={} result={} prompt={}",
        result.rule_choice, result.result, result.prompt
    );
    assert_eq!(result.app_kind, "discord");
    assert_eq!(result.rule_choice, "never");
    assert_eq!(
        result.stable_id,
        "discord:900000000000000135:direct_message:900000000000000136"
    );
    assert_eq!(result.result, "unlisted");
    assert!(!result.prompt);
}
