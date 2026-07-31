use std::path::{Path, PathBuf};
use std::process::Command;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn powershell() -> Option<&'static str> {
    for candidate in ["pwsh", "powershell.exe", "powershell"] {
        if Command::new(candidate)
            .args(["-NoProfile", "-Command", "$PSVersionTable.PSVersion.Major"])
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
        {
            return Some(candidate);
        }
    }
    None
}

#[test]
fn b6_controllers_read_the_retained_preflight_before_consent_or_drive() {
    let Some(ps) = powershell() else {
        eprintln!("skipping osl-p2p-loop B6 behavior test: PowerShell is unavailable");
        return;
    };

    let script = repo_root().join("scripts/qa/osl-p2p-loop.ps1");
    let output = Command::new(ps)
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(&script)
        .arg("-BundleB")
        .arg("org.oslprivacy.hub.selftest.b")
        .arg("-ExeB")
        .arg(&script)
        .arg("-TempRootB")
        .arg(std::env::temp_dir().join("osl-p2p-loop-selftest-b"))
        .arg("-RunScriptSelfTests")
        .output()
        .expect("run osl-p2p-loop B6 self-tests");

    assert!(
        output.status.success(),
        "retained B6 preflight self-tests failed with status {:?}\nstdout:\n{}\nstderr:\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout)
            .contains("ok - b6_controllers_read_the_retained_preflight_before_consent_or_drive"),
        "self-test output must identify the exact behavior under proof\nstdout:\n{}",
        String::from_utf8_lossy(&output.stdout)
    );
}
