//! TASK 1086's focused byte-oracle gate.  The executable verifier does the
//! streaming filesystem reads so this target cannot accidentally pass from a
//! displayed fingerprint or sender-side metadata label.

use std::process::Command;

#[test]
fn task_1086_received_bytes_are_independently_hashed_and_mutations_go_red() {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let proof = manifest.join("../../scripts/task-1086-whatsapp-attachment-byte-oracle-test.py");
    let output = Command::new("python3")
        .arg(proof)
        .output()
        .expect("run TASK 1086 byte-oracle proof");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "TASK1086 proof failed: {stdout}{stderr}");
    for required in [
        "TASK1086 PASS carrier=WhatsApp",
        "TASK1086 RED mutation_exit=1",
        "expected_sha256=",
        "received_sha256=",
        "TASK1086 RED zero_bytes_exit=1",
        "TASK1086 RED starvation=one-time-open exit=1",
        "TASK1086 RED starvation=cover-observation exit=1",
        "TASK1086 RED starvation=keep-file exit=1",
    ] {
        assert!(stdout.contains(required), "missing {required:?} in {stdout:?}");
    }
    print!("{stdout}");
}
