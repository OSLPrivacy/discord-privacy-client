use ipc::auto_whitelist_rules::{AutoWhitelistAppKind, AutoWhitelistChoice};
use ipc::commands::{cmd_osl_query_auto_whitelist_rule, cmd_osl_save_auto_whitelist_rule};
use ipc::state::AppState;

#[test]
fn direct_rule_query_prints_each_valid_choice() {
    let state = AppState::new();
    let saved = cmd_osl_save_auto_whitelist_rule(
        &state,
        AutoWhitelistAppKind::Chat,
        AutoWhitelistChoice::OnlyIfAFriend,
    )
    .expect("saving an app-kind rule must succeed");
    assert_eq!(saved.app_kind, AutoWhitelistAppKind::Chat);
    assert_eq!(saved.choice, AutoWhitelistChoice::OnlyIfAFriend);

    let query = cmd_osl_query_auto_whitelist_rule(&state, AutoWhitelistAppKind::Chat)
        .expect("direct rule query must succeed");
    assert_eq!(query.saved_choice, Some(AutoWhitelistChoice::OnlyIfAFriend));

    let choices = query.valid_choice_labels();
    println!("direct rule query valid choices: {}", choices.join(", "));
    assert_eq!(
        choices,
        vec!["never", "ask me", "always", "only if a friend"]
    );
}
