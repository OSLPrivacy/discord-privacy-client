use std::process::Command;

const X_PRIVATE_COMPOSER: &str = env!("CARGO_BIN_EXE_x-private-composer");
const STUB_READER_ENV: &str = "OSL_TASK_1106_STUB_X_PRIVATE_BOX_READER";

fn field(output: &str, name: &str) -> String {
    output
        .lines()
        .find_map(|line| line.strip_prefix(&format!("TASK1106_{name}=")))
        .unwrap_or_else(|| panic!("missing {name} in {output:?}"))
        .to_owned()
}

#[test]
fn task_1106_multibyte_count_then_command_clear_and_noop_reader_red_path() {
    let output = Command::new(X_PRIVATE_COMPOSER)
        .arg("task-1106-count-check")
        .output()
        .expect("run X private-box count check");
    assert!(
        output.status.success(),
        "X private-box count check failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let output = String::from_utf8(output.stdout).expect("count-check stdout is UTF-8");

    assert_eq!(field(&output, "FIXTURE_BYTES"), "37");
    assert_eq!(field(&output, "COUNT_AFTER_ENTER"), "37");
    assert_eq!(field(&output, "COUNT_AFTER_CLEAR"), "0");
    assert_eq!(field(&output, "X_COMPOSER_CHARS"), "0");

    let broken = Command::new(X_PRIVATE_COMPOSER)
        .arg("task-1106-count-check")
        .env(STUB_READER_ENV, "1")
        .output()
        .expect("run X private-box count check with no-op reader");
    assert!(
        !broken.status.success(),
        "a no-op X private-box reader unexpectedly passed the count check"
    );
    let broken_stderr = String::from_utf8(broken.stderr).expect("red-path stderr is UTF-8");
    assert!(
        broken_stderr.contains(
            "TASK1106_CHECK_FAILED=X private-box reader returned 0 bytes after enter; expected 37"
        ),
        "wrong no-op reader failure: {broken_stderr:?}"
    );

    println!("TASK1106 fixture_bytes=37 count_after_enter=37");
    println!("TASK1106 count_after_clear=0 x_composer_chars=0");
    println!(
        "TASK1106 noop_reader_exit={:?} check=failed",
        broken.status.code()
    );
}
