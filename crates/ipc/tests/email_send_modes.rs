use ipc::commands::cmd_osl_list_email_send_modes;

#[test]
fn email_send_modes_command_lists_exactly_five_email_choices() {
    let modes = cmd_osl_list_email_send_modes();
    let names: Vec<&str> = modes.iter().map(|mode| mode.name.as_str()).collect();

    println!("email choices count: {}", names.len());
    println!("email choices: {}", names.join(", "));

    assert_eq!(
        names,
        vec![
            "Manual",
            "Double Enter",
            "Experimental Single Enter",
            "Instant",
            "Match typing",
        ]
    );
}
