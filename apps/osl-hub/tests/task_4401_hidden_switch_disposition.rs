use std::process::Command;

#[test]
fn frozen_policy_covers_every_4400_row_and_rejects_every_unsafe_equivalent() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = Command::new("bash")
        .arg("apps/osl-hub/scripts/task-4401-hidden-switch-disposition.sh")
        .arg("--self-test")
        .current_dir(&root)
        .output()
        .expect("TASK 4401 checker starts");
    assert!(
        output.status.success(),
        "TASK 4401 checker failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("TASK4401_NEITHER_COUNT=0"));
    assert!(stdout.contains("TASK4401_PROHIBITED_MATRIX_CELLS=6460"));
    assert!(stdout.contains("TASK4401_UNSAFE_MUTATION_CELLS=6460"));
}

#[test]
fn a_reasoned_but_unsafe_renamed_equivalent_is_refused() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = Command::new("bash")
        .arg("apps/osl-hub/scripts/task-4401-hidden-switch-disposition.sh")
        .args([
            "--prove-unsafe",
            "authorization-or-recipient-consent-bypass",
        ])
        .current_dir(&root)
        .output()
        .expect("TASK 4401 unsafe proof starts");
    assert!(!output.status.success(), "unsafe shipping mutation passed");
    assert!(String::from_utf8_lossy(&output.stderr)
        .contains("invariant=authorization-or-recipient-consent-bypass"));
}
