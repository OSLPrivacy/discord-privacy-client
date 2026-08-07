use ipc::commands::cmd_osl_get_instagram_whitelist_kinds;

#[test]
fn instagram_kinds_command_returns_exactly_three_named_kinds() {
    let kinds = cmd_osl_get_instagram_whitelist_kinds().unwrap();
    let ids: Vec<String> = kinds.iter().map(|kind| kind.id.clone()).collect();
    let names: Vec<String> = kinds.iter().map(|kind| kind.name.clone()).collect();

    println!(
        "instagram whitelist kinds count={} names={}",
        kinds.len(),
        names.join(", ")
    );

    assert_eq!(kinds.len(), 3);
    assert_eq!(ids, vec!["direct_message", "group_chat", "public_post"]);
    assert_eq!(names, vec!["direct message", "group chat", "public post"]);
}
