use ipc::commands::{cmd_osl_get_instagram_whitelist_kinds, cmd_osl_get_telegram_whitelist_kinds};

#[test]
fn task_4200_telegram_finish_line_names_the_full_kind_list() {
    let kinds = cmd_osl_get_telegram_whitelist_kinds().expect("telegram whitelist kinds");
    let names = kinds
        .iter()
        .map(|kind| kind.name.as_str())
        .collect::<Vec<_>>();
    println!(
        "TASK4200_TELEGRAM count={} names={}",
        names.len(),
        names.join(", ")
    );
    assert_eq!(
        names,
        vec![
            "direct message",
            "group chat",
            "channel",
            "public post",
            "supergroup",
            "saved messages",
        ]
    );
    assert_eq!(kinds.len(), names.len());
}

#[test]
fn task_4200_instagram_finish_line_names_the_full_kind_list() {
    let kinds = cmd_osl_get_instagram_whitelist_kinds().expect("instagram whitelist kinds");
    let names = kinds
        .iter()
        .map(|kind| kind.name.as_str())
        .collect::<Vec<_>>();
    println!(
        "TASK4200_INSTAGRAM count={} names={}",
        names.len(),
        names.join(", ")
    );
    assert_eq!(
        names,
        vec![
            "direct message",
            "group chat",
            "public post",
            "story",
            "reel",
        ]
    );
    assert_eq!(kinds.len(), names.len());
}
