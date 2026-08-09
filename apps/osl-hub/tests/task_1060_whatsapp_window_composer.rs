use std::process::Command;

const WHATSAPP_WINDOW_COMPOSER: &str = env!("CARGO_BIN_EXE_whatsapp-window-composer");

fn run(fixture: &str) -> std::process::Output {
    Command::new(WHATSAPP_WINDOW_COMPOSER)
        .arg(fixture)
        .output()
        .expect("run WhatsApp window/composer direct command")
}

fn field(output: &str, name: &str) -> String {
    output
        .lines()
        .find_map(|line| line.strip_prefix(&format!("TASK1060_{name}=")))
        .unwrap_or_else(|| panic!("missing {name} in {output:?}"))
        .to_owned()
}

#[test]
fn direct_command_returns_active_window_direct_conversation_and_typing_box() {
    let output = run("whatsapp-direct");
    assert!(output.status.success(), "direct fixture failed: {output:?}");
    let stdout = String::from_utf8(output.stdout).expect("stdout is UTF-8");
    assert_eq!(field(&stdout, "WINDOW"), "WhatsApp");
    assert_eq!(field(&stdout, "CONVERSATION"), "OSL QA Peer");
    assert_eq!(field(&stdout, "TYPING_BOX"), "Type a message");
    assert_eq!(field(&stdout, "FOUND_COUNT"), "3");

    let signed_out = run("whatsapp-signed-out");
    assert!(
        !signed_out.status.success(),
        "a phone-number field must not be accepted as a typing box"
    );

    println!(
        "TASK1060 direct_command=whatsapp-window-composer window={} conversation={} typing_box={} found_count={}",
        field(&stdout, "WINDOW"),
        field(&stdout, "CONVERSATION"),
        field(&stdout, "TYPING_BOX"),
        field(&stdout, "FOUND_COUNT")
    );
    println!("TASK1060 signed_out_phone_box_refused=true");
}
