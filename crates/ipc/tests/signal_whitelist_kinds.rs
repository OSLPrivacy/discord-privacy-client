use ipc::commands::cmd_osl_list_signal_whitelist_kinds;

#[test]
fn signal_kinds_command_returns_the_full_named_kind_list() {
    let kinds = cmd_osl_list_signal_whitelist_kinds().expect("signal whitelist kinds");
    let names: Vec<&str> = kinds.iter().map(|kind| kind.name).collect();
    println!(
        "signal whitelist kinds count={} names={}",
        names.len(),
        names.join(", ")
    );
    assert_eq!(names, vec!["direct message", "group chat", "story"]);
    assert_eq!(kinds.len(), names.len());
}
