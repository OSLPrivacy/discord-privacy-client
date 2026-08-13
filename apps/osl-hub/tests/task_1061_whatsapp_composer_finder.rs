use std::process::Command;

const WHATSAPP_WINDOW_COMPOSER: &str = env!("CARGO_BIN_EXE_whatsapp-window-composer");
const BOX_ID: &str = "whatsapp-box-1061";
const MESSAGE_BOX: &str = "Type a message";

fn run(fixture: &str) -> std::process::Output {
    Command::new(WHATSAPP_WINDOW_COMPOSER)
        .arg(fixture)
        .output()
        .expect("run WhatsApp window/composer fixture")
}

fn field(output: &str, name: &str) -> String {
    output
        .lines()
        .find_map(|line| line.strip_prefix(&format!("TASK1060_{name}=")))
        .unwrap_or_else(|| panic!("missing {name} in {output:?}"))
        .to_owned()
}

fn one_message_box(fixture: &str) -> Vec<String> {
    let output = run(fixture);
    assert!(output.status.success(), "{fixture} failed: {output:?}");
    let stdout = String::from_utf8(output.stdout).expect("command stdout is UTF-8");
    assert_eq!(field(&stdout, "FOUND_COUNT"), "3");
    vec![field(&stdout, "TYPING_BOX")]
}

#[test]
fn task_1061_whatsapp_finder_refuses_search_focused_and_closed_states() {
    let good = one_message_box(BOX_ID);
    assert_eq!(
        good,
        [MESSAGE_BOX],
        "good box must yield exactly one WhatsApp message box"
    );

    for state in ["search-focused", "closed"] {
        let output = run(&format!("whatsapp-{state}"));
        assert!(!output.status.success(), "{state} must be refused by name");
        let stderr = String::from_utf8(output.stderr).expect("command stderr is UTF-8");
        assert!(
            stderr.contains(&format!("finder refused for state {state}")),
            "{state} must reach the finder refusal: {stderr:?}"
        );
    }

    let restored = one_message_box(BOX_ID);
    assert_eq!(
        restored, good,
        "restored box must return the same one message box"
    );

    println!(
        "TASK1061 box={BOX_ID} result_count={} result_name=WhatsApp message box ({})",
        good.len(),
        good[0]
    );
    println!("TASK1061 state=search-focused result=refused");
    println!("TASK1061 state=closed result=refused");
    println!(
        "TASK1061 restored_box={BOX_ID} result_count={} result_name=WhatsApp message box ({})",
        restored.len(),
        restored[0]
    );
}
