use std::path::PathBuf;
use std::process::Command;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("repository root")
        .to_path_buf()
}

fn run(overlap: bool) -> std::process::Output {
    let temp = tempfile::tempdir().expect("temporary audit directory");
    Command::new("bash")
        .arg(root().join("scripts/qa/osl-tiny-window-audit.sh"))
        .env("OSL_TINY_AUDIT_OUT", temp.path().join("out"))
        .env("OSL_TINY_AUDIT_RUN_DIR", temp.path().join("run"))
        .env("OSL_TINY_AUDIT_DISPLAY", format!(":{}", 260 + std::process::id() % 30))
        .env("OSL_TINY_AUDIT_INJECT_OVERLAP", if overlap { "1" } else { "0" })
        .output()
        .expect("audit launches")
}

#[test]
fn tiny_osl_windows_are_complete_non_overlapping_and_focusable() {
    let output = run(false);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "stdout={stdout}");
    assert!(stdout.contains("TASK3530_SUMMARY screen_count=6 normal_positive=6 minimum_count_match=6 no_overlap=6 focusable=6 smaller_refused=6 status=ok"), "{stdout}");
    println!("{stdout}");
}

#[test]
fn tiny_osl_windows_reject_an_overlap_mutant() {
    let output = run(true);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(output.status.code(), Some(1), "{stdout}");
    assert!(stdout.contains("no_overlap=5") && stdout.contains("status=fail"), "{stdout}");
}
