use std::process::Command;

const CHECK: &str = env!("CARGO_BIN_EXE_signal-direct-message-check");

fn run(fixture: &str) -> std::process::Output {
    Command::new(CHECK)
        .arg(fixture)
        .output()
        .expect("run Signal direct-message check")
}

#[test]
fn task_1052_only_allowed_direct_fixture_returns_direct_message_controls() {
    let direct = run("signal-direct");
    assert!(direct.status.success());
    let direct = String::from_utf8(direct.stdout).expect("direct stdout is UTF-8");
    assert_eq!(
        direct,
        "TASK1052_KIND=direct-message\nTASK1052_CONTROLS=conversation,composer\nTASK1052_CONTROL_COUNT=2\n"
    );

    for fixture in [
        "signal-group",
        "signal-note-to-self",
        "signal-unapproved-direct",
    ] {
        let output = run(fixture);
        assert!(
            !output.status.success(),
            "{fixture} must not pass the direct check"
        );
        assert!(String::from_utf8(output.stdout)
            .expect("fixture stdout is UTF-8")
            .is_empty());
    }

    let group = run("signal-group");
    assert_eq!(group.status.code(), Some(2));
    assert_eq!(
        String::from_utf8(group.stderr).expect("group stderr is UTF-8"),
        "TASK1052_REFUSAL=Signal fixture is a group, not a direct-message\n"
    );
}
