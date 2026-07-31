use std::process::Command;

#[test]
fn readback() {
    let repo_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = Command::new("python3")
        .args([
            "-m",
            "unittest",
            "-v",
            "tools/c4-native-authority-v3/test_verify.py",
        ])
        .current_dir(&repo_root)
        .output()
        .expect("run C4 native-authority v3 schema behavior tests");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}\n{stderr}");

    assert!(
        output.status.success(),
        "C4 native-authority v3 schema tests failed with status {:?}\nstdout:\n{stdout}\nstderr:\n{stderr}",
        output.status.code()
    );
    assert!(
        combined.contains("readback (tools/c4-native-authority-v3/test_verify.NativeAuthorityV3Tests.readback) ... ok"),
        "the exact readback schema test must be exercised\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        combined.contains("unittest.case.FunctionTestCase (test_verify.py) ... ok"),
        "the exact test_verify.py schema test must be exercised\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}
