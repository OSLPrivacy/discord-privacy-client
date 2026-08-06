use ipc::commands::cmd_osl_list_telegram_whitelist_kinds;

#[test]
fn telegram_kinds_command_returns_exactly_four_named_kinds() {
    let kinds = cmd_osl_list_telegram_whitelist_kinds();
    let names: Vec<&str> = kinds.iter().map(|kind| kind.name.as_str()).collect();

    println!("TASK0147 telegram_whitelist_kinds.count={}", kinds.len());
    for (index, kind) in kinds.iter().enumerate() {
        println!("TASK0147 telegram_whitelist_kinds.{index}.app={}", kind.app);
        println!(
            "TASK0147 telegram_whitelist_kinds.{index}.name={}",
            kind.name
        );
    }

    assert_eq!(kinds.len(), 4);
    assert!(kinds.iter().all(|kind| kind.app == "telegram"));
    assert_eq!(
        names,
        vec!["direct_message", "group_chat", "channel", "public_post"]
    );
}
