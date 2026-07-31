use std::process::Command;

#[test]
fn scripts_audit_hub_release_py() {
    let repo_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = Command::new("python3")
        .args(["-m", "unittest", "-v", "scripts/audit_hub_release.py"])
        .current_dir(&repo_root)
        .output()
        .expect("run hub release supply-chain audit behavior tests");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}\n{stderr}");

    assert!(
        output.status.success(),
        "hub release supply-chain audit tests failed with status {:?}\nstdout:\n{stdout}\nstderr:\n{stderr}",
        output.status.code()
    );
    assert!(
        combined.contains("scripts/audit_hub_release.py (scripts.audit_hub_release.HubReleaseAuditTests.scripts/audit_hub_release.py) ... ok"),
        "the exact release-audit test name must be exercised\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        combined.contains("Refuse release supply-chain drift before any candidate is signed. (scripts.audit_hub_release.HubReleaseAuditTests.Refuse release supply-chain drift before any candidate is signed.) ... ok"),
        "the release-audit suite must exercise refusal before candidate signing\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}
