use std::process::Command;

const X_TEXT_PLACEMENT: &str = env!("CARGO_BIN_EXE_x-text-placement");
const EXPECTED_FIXTURE_HEX: &str = "4f534c7c587c313130337c636166c3a97cf09f9492";

fn field<'a>(output: &'a str, name: &str) -> &'a str {
    output
        .lines()
        .find_map(|line| line.strip_prefix(&format!("TASK1103_{name}=")))
        .unwrap_or_else(|| panic!("missing TASK1103_{name} in {output:?}"))
}

#[test]
fn x_uses_shared_exact_text_placement_read_back_and_clear_actions() {
    let output = Command::new(X_TEXT_PLACEMENT)
        .output()
        .expect("run direct X text-placement command");
    assert!(output.status.success(), "command failed: {output:?}");
    let stdout = String::from_utf8(output.stdout).expect("command stdout is UTF-8");

    let fixture_bytes = field(&stdout, "FIXTURE_BYTES");
    assert_eq!(
        field(&stdout, "SHARED_ACTIONS"),
        "place_text,read_back_text,clear_text,read_back_text"
    );
    assert_eq!(field(&stdout, "ORIGINAL_HEX"), EXPECTED_FIXTURE_HEX);
    assert_eq!(field(&stdout, "READBACK_HEX"), EXPECTED_FIXTURE_HEX);
    assert_eq!(field(&stdout, "PLACED_BYTES"), fixture_bytes);
    assert_eq!(field(&stdout, "READBACK_BYTES"), fixture_bytes);
    assert_eq!(field(&stdout, "CLEARED_BYTES"), "0");

    println!("TASK1103_MARKED_BYTES={fixture_bytes}");
    println!(
        "TASK1103_READBACK_BYTES={}",
        field(&stdout, "READBACK_BYTES")
    );
    println!("TASK1103_ORIGINAL_HEX={}", field(&stdout, "ORIGINAL_HEX"));
    println!("TASK1103_READBACK_HEX={}", field(&stdout, "READBACK_HEX"));
    println!("TASK1103_CLEARED_BYTES={}", field(&stdout, "CLEARED_BYTES"));
    println!(
        "TASK1103_SHARED_ACTIONS={}",
        field(&stdout, "SHARED_ACTIONS")
    );
}
