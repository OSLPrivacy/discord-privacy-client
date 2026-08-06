use ipc::commands::{cmd_osl_lookup_whitelist_rule, cmd_osl_save_whitelist_rules};
use tempfile::TempDir;

#[test]
fn direct_command_lookup_returns_allowed_denied_and_ask_for_three_fixtures() {
    let allowed_dir = TempDir::new().unwrap();
    let denied_dir = TempDir::new().unwrap();
    let ask_dir = TempDir::new().unwrap();

    cmd_osl_save_whitelist_rules(
        allowed_dir.path().to_path_buf(),
        vec!["conversation:allowed-fixture".to_string()],
        "deny".to_string(),
    )
    .unwrap();
    cmd_osl_save_whitelist_rules(denied_dir.path().to_path_buf(), vec![], "deny".to_string())
        .unwrap();
    cmd_osl_save_whitelist_rules(ask_dir.path().to_path_buf(), vec![], "ask".to_string()).unwrap();

    let allowed = cmd_osl_lookup_whitelist_rule(
        allowed_dir.path().to_path_buf(),
        "conversation:allowed-fixture".to_string(),
    )
    .unwrap();
    let denied = cmd_osl_lookup_whitelist_rule(
        denied_dir.path().to_path_buf(),
        "conversation:new-denied-fixture".to_string(),
    )
    .unwrap();
    let ask = cmd_osl_lookup_whitelist_rule(
        ask_dir.path().to_path_buf(),
        "conversation:new-ask-fixture".to_string(),
    )
    .unwrap();

    println!(
        "TASK0318 direct command lookup fixture=allowed conversation={} result={}",
        allowed.conversation_id, allowed.result
    );
    println!(
        "TASK0318 direct command lookup fixture=denied conversation={} result={}",
        denied.conversation_id, denied.result
    );
    println!(
        "TASK0318 direct command lookup fixture=ask conversation={} result={}",
        ask.conversation_id, ask.result
    );

    assert_eq!(allowed.result, "allowed");
    assert_eq!(denied.result, "denied");
    assert_eq!(ask.result, "ask");
}
