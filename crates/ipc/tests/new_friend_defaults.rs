use ipc::commands::cmd_osl_get_new_friend_defaults;
use ipc::AppState;

#[test]
fn direct_defaults_query_returns_all_three_new_friend_values() {
    let state = AppState::new();

    let defaults = cmd_osl_get_new_friend_defaults(&state).expect("direct defaults query");
    println!(
        "TASK0247 new_friend_defaults.account_reach={}",
        defaults.account_reach
    );
    println!(
        "TASK0247 new_friend_defaults.auto_whitelist={}",
        defaults.auto_whitelist
    );
    println!(
        "TASK0247 new_friend_defaults.verification_warnings={}",
        defaults.verification_warnings
    );

    assert_eq!(defaults.account_reach, "approved_chats_only");
    assert_eq!(defaults.auto_whitelist, "never");
    assert_eq!(defaults.verification_warnings, "enabled");
}
