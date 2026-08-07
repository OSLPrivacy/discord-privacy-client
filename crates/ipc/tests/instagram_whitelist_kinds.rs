use ipc::commands::cmd_osl_get_instagram_whitelist_kinds;

#[test]
fn instagram_kinds_command_returns_exactly_five_named_kinds() {
    let kinds = cmd_osl_get_instagram_whitelist_kinds().unwrap();
    let ids: Vec<String> = kinds.iter().map(|kind| kind.id.clone()).collect();
    let names: Vec<String> = kinds.iter().map(|kind| kind.name.clone()).collect();

    println!(
        "TASK3744_INSTAGRAM_KIND_LIST count={} names={}",
        kinds.len(),
        names.join(", ")
    );

    assert_eq!(kinds.len(), 5);
    assert_eq!(
        ids,
        vec![
            "direct_message",
            "group_chat",
            "public_post",
            "comment",
            "story"
        ]
    );
    assert_eq!(
        names,
        vec![
            "direct message",
            "group chat",
            "public post",
            "comment",
            "story"
        ]
    );
}
