use ipc::commands::cmd_osl_list_signal_whitelist_kinds;

#[test]
fn signal_kinds_command_returns_exactly_two_named_kinds() {
    let kinds = cmd_osl_list_signal_whitelist_kinds().expect("signal whitelist kinds");
    let names: Vec<&str> = kinds.iter().map(|kind| kind.name).collect();
    println!(
        "signal whitelist kinds count={} names={}",
        names.len(),
        names.join(", ")
    );
    assert_eq!(names, vec!["direct message", "group chat"]);
    assert_eq!(
        kinds
            .iter()
            .map(|kind| kind.auto_rule_app_kind)
            .collect::<Vec<_>>(),
        vec!["signal_direct_message", "signal_group_chat"]
    );
    assert_eq!(
        kinds
            .iter()
            .map(|kind| kind.allowed_place_kind)
            .collect::<Vec<_>>(),
        vec!["direct_message", "group_chat"]
    );
}
