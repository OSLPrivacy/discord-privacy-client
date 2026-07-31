use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{json, Value};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Which interpreter runs the script, and whether it needs Windows-visible paths.
///
/// A Windows PowerShell reached from WSL resolves a Linux-style absolute path
/// (`/home/...`) against its *current location* instead of treating it as rooted,
/// so `Out-File -LiteralPath /tmp/x.json` silently lands under the UNC working
/// directory and the write fails. A native Unix `pwsh` must be given the Linux
/// path unchanged. Ask the interpreter which world it lives in rather than
/// guessing from the executable name.
struct Shell {
    command: &'static str,
    windows_paths: bool,
}

impl Shell {
    fn path(&self, path: &Path) -> String {
        if !self.windows_paths || cfg!(windows) {
            return path.display().to_string();
        }
        Command::new("wslpath")
            .arg("-w")
            .arg(path)
            .output()
            .ok()
            .filter(|output| output.status.success())
            .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
            .filter(|translated| !translated.is_empty())
            .unwrap_or_else(|| path.display().to_string())
    }
}

const HOST_KIND_PROBE: &str =
    "if ($PSVersionTable.PSVersion.Major -le 5 -or $IsWindows) { 'windows' } else { 'unix' }";

fn powershell() -> Option<Shell> {
    for candidate in ["pwsh", "powershell.exe", "powershell"] {
        let Ok(output) = Command::new(candidate)
            .args(["-NoProfile", "-Command", HOST_KIND_PROBE])
            .output()
        else {
            continue;
        };
        if !output.status.success() {
            continue;
        }
        let kind = String::from_utf8_lossy(&output.stdout).trim().to_lowercase();
        if kind != "windows" && kind != "unix" {
            continue;
        }
        return Some(Shell {
            command: candidate,
            windows_paths: kind == "windows",
        });
    }
    None
}

#[test]
fn b6_controllers_read_the_retained_preflight_before_consent_or_drive() {
    let Some(ps) = powershell() else {
        eprintln!("skipping osl-p2p-loop B6 behavior test: PowerShell is unavailable");
        return;
    };

    let temp_root = unique_temp_root("osl-p2p-loop-b6");
    let missing_a = temp_root.join("missing-a");
    let missing_b = temp_root.join("missing-b");
    let valid_a = temp_root.join("valid-a");
    let valid_b = temp_root.join("valid-b");
    std::fs::create_dir_all(&missing_a).expect("create missing A temp root");
    std::fs::create_dir_all(&missing_b).expect("create missing B temp root");
    std::fs::create_dir_all(&valid_a).expect("create valid A temp root");
    std::fs::create_dir_all(&valid_b).expect("create valid B temp root");

    let script = repo_root().join("scripts/qa/osl-p2p-loop.ps1");
    let missing_json = temp_root.join("missing.json");
    let missing = run_precondition_child(&ps, &script, &missing_a, &missing_b, &missing_json);
    assert_blocked(&missing, "b6-preflight");
    assert_eq!(
        missing["preconditionGate"][0]["gate"], "b6-preflight",
        "missing retained receipts must evaluate the B6 gate first"
    );
    assert_eq!(
        missing["preconditionGate"][0]["ok"], false,
        "missing retained receipts must refuse at B6 before consent"
    );
    assert_no_drive_requests(&missing_a);
    assert_no_drive_requests(&missing_b);

    write_b6_receipt(&valid_a);
    write_b6_receipt(&valid_b);
    let valid_json = temp_root.join("valid-no-consent.json");
    let valid = run_precondition_child(&ps, &script, &valid_a, &valid_b, &valid_json);
    assert_blocked(&valid, "consent");
    assert_eq!(
        valid["preconditionGate"][0]["gate"], "b6-preflight",
        "complete retained receipts must pass the B6 gate before consent"
    );
    assert_eq!(valid["preconditionGate"][0]["ok"], true);
    assert_eq!(
        valid["preconditionGate"][1]["gate"], "consent",
        "the next refusal after a valid B6 preflight must be consent"
    );
    assert_eq!(valid["preconditionGate"][1]["ok"], false);
    assert_no_drive_requests(&valid_a);
    assert_no_drive_requests(&valid_b);

    let _ = std::fs::remove_dir_all(temp_root);
}

fn run_precondition_child(
    ps: &Shell,
    script: &Path,
    temp_root_a: &Path,
    temp_root_b: &Path,
    json_out: &Path,
) -> Value {
    let output = Command::new(ps.command)
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(ps.path(script))
        .arg("-BundleB")
        .arg("org.oslprivacy.hub.selftest.b")
        .arg("-ExeB")
        .arg(ps.path(script))
        .arg("-TempRootA")
        .arg(ps.path(temp_root_a))
        .arg("-TempRootB")
        .arg(ps.path(temp_root_b))
        .arg("-JsonOut")
        .arg(ps.path(json_out))
        .arg("-SelfTestPreconditionChild")
        .arg("-Quiet")
        .output()
        .expect("run osl-p2p-loop precondition child");

    assert_eq!(
        output.status.code(),
        Some(2),
        "precondition child must stop with the blocked exit code\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let bytes = std::fs::read(json_out).expect("precondition child writes JSON verdict");
    serde_json::from_slice(&bytes).expect("precondition child JSON is valid")
}

fn assert_blocked(report: &Value, blocked_by: &str) {
    assert_eq!(report["overall"]["verdict"], "blocked");
    assert_eq!(report["overall"]["blockedBy"], blocked_by);
    assert_eq!(report["overall"]["stepsRun"], false);
    assert!(
        report["steps"].as_array().is_some_and(Vec::is_empty),
        "blocked precondition must not run measurement steps: {report:#}"
    );
}

fn assert_no_drive_requests(root: &Path) {
    let drive_requests = std::fs::read_dir(root)
        .expect("temp root is readable")
        .filter_map(Result::ok)
        .filter(|entry| {
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                return false;
            };
            name.starts_with("osl-qa-selftest") && name.ends_with(".request")
        })
        .count();
    assert_eq!(
        drive_requests,
        0,
        "precondition refusal must not write a self-test drive request in {}",
        root.display()
    );
}

fn write_b6_receipt(root: &Path) {
    let receipt = json!({
        "schemaVersion": 2,
        "b6Preflight": {
            "schemaVersion": 2,
            "startupAllowed": true,
            "startupBlockers": [],
            "sourceCommit": "0123456789abcdef0123456789abcdef01234567",
            "binarySha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "serverDeploymentIdentity": "dedicated-qa-selftest",
            "identityPublicFingerprintsSha256": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            "identityKeystoreRootFingerprintsSha256": "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
            "runtime": {
                "distinctIdentityAndKeystoreRoots": true,
                "bidirectionalCiphertextAndPlaintext": true,
                "offlineEnqueueAndDelivery": true,
                "persistedRatchetRestart": true,
                "exactlyOnceDrain": true,
                "independentPeerAttribution": true,
                "negativeCrossPeerIsolation": true
            }
        }
    });
    std::fs::write(
        root.join("osl-discord-qa-b6-preflight.v2.json"),
        serde_json::to_vec_pretty(&receipt).expect("serialize B6 receipt fixture"),
    )
    .expect("write B6 receipt fixture");
}

fn unique_temp_root(label: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock is after Unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("{label}-{}-{nanos}", std::process::id()))
}
