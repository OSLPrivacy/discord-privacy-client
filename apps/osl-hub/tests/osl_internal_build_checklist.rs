use std::path::{Path, PathBuf};
use std::process::Command;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn keep_compact_reports_and_trap_ledger_rules_current_after_each_wave() {
    let root = repo_root();
    let script = root.join("scripts/qa/test_osl_internal_build_checklist.py");
    let output = Command::new("python3")
        .arg(&script)
        .current_dir(&root)
        .output()
        .expect("run internal build checklist behavior test");

    assert!(
        output.status.success(),
        "internal build checklist behavior test failed with status {:?}\nstdout:\n{}\nstderr:\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("Keep compact reports and trap-ledger rules current after each wave."),
        "checklist test output must identify the J24 behavior under proof\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
