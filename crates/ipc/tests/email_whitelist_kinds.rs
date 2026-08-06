use ipc::commands::cmd_osl_list_email_whitelist_kinds;

#[test]
fn email_kinds_command_returns_exactly_two_named_kinds() {
    let kinds = cmd_osl_list_email_whitelist_kinds();
    let names: Vec<&str> = kinds.iter().map(|kind| kind.name.as_str()).collect();

    println!("email whitelist kinds: {}", names.join(", "));
    println!("TASK0165 email_kind_count={}", names.len());

    assert_eq!(names, vec!["email address", "email domain"]);
}
