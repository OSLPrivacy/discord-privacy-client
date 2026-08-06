use ipc::commands::cmd_osl_get_new_friend_defaults;
use ipc::state::AppState;

#[test]
fn direct_defaults_query_returns_all_three_new_friend_values() {
    let state = AppState::new();
    let defaults = cmd_osl_get_new_friend_defaults(&state).unwrap();

    println!(
        "new-friend defaults account_reach={} auto_whitelist={} verification_warnings={}",
        defaults.account_reach, defaults.auto_whitelist, defaults.verification_warnings
    );

    assert_eq!(defaults.account_reach, "approved_chats_only");
    assert_eq!(defaults.auto_whitelist, "never");
    assert_eq!(defaults.verification_warnings, "enabled");
}
