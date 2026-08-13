//! TASK 6843 runs the TASK 6842 test target against independently mutated
//! throwaway copies.  Keeping the launcher as an integration test makes the
//! red-proof part of the normal package test surface as well as a script that
//! can be retained verbatim in audit evidence.

use std::path::PathBuf;
use std::process::Command;

#[test]
fn task_6843_throwaway_mutations_make_task_6842_red_and_restore_green() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let script = root.join("scripts/task-6843-selective-visibility-proof.sh");
    let output = Command::new("bash")
        .arg(&script)
        .current_dir(&root)
        .output()
        .expect("TASK6843 mutation proof starts");
    assert!(
        output.status.success(),
        "TASK6843 proof failed (exit {:?})\nstdout:\n{}\nstderr:\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("TASK6843_OVERALL_EXIT=0"));
    assert!(stdout.contains("TASK6843_COPIES_DISCARDED=true"));
}
