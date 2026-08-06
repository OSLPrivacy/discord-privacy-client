use std::process::Command;

const BUILD_SWITCH_METADATA: &str = env!("CARGO_BIN_EXE_build-switch-metadata");

#[test]
fn direct_metadata_command_prints_version_and_complete_switch_list() {
    let output = Command::new(BUILD_SWITCH_METADATA)
        .arg("--require-switch-list")
        .arg("sender_keys_enabled,rn_wire_in_enabled")
        .output()
        .expect("run build-switch-metadata");

    assert!(
        output.status.success(),
        "metadata command should succeed, stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("stdout is UTF-8");

    assert!(stdout.contains("one_build_version=0.0.1"));
    assert!(stdout.contains("runtime_switches=2"));
    assert!(stdout.contains("runtime_switch: sender_keys_enabled=true"));
    assert!(stdout.contains("runtime_switch: rn_wire_in_enabled=false"));
}

#[test]
fn direct_metadata_command_exits_1_when_a_switch_is_omitted() {
    let output = Command::new(BUILD_SWITCH_METADATA)
        .arg("--require-switch-list")
        .arg("sender_keys_enabled")
        .output()
        .expect("run build-switch-metadata");

    assert_eq!(output.status.code(), Some(1));
    let stdout = String::from_utf8(output.stdout).expect("stdout is UTF-8");
    let stderr = String::from_utf8(output.stderr).expect("stderr is UTF-8");

    assert!(stdout.contains("one_build_version=0.0.1"));
    assert!(stdout.contains("runtime_switch: sender_keys_enabled=true"));
    assert!(stdout.contains("runtime_switch: rn_wire_in_enabled=false"));
    assert!(
        stderr.contains("missing runtime switch: rn_wire_in_enabled"),
        "stderr={stderr}"
    );
}
