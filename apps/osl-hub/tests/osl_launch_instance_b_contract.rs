use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

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

fn scratch(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time is after Unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "osl-launch-b-{label}-{}-{nanos}",
        std::process::id()
    ))
}

fn base_fixture(root: &Path) -> serde_json::Value {
    serde_json::json!({
        "bundleA": "org.oslprivacy.hub",
        "bundleB": "org.oslprivacy.hubqab",
        "tempRootA": root.join("temp-a"),
        "tempRootB": root.join("temp-b"),
        "preflightStartupAllowed": true,
        "instanceAMarkerBefore": true,
        "instanceAMarkerAfter": true,
        "instanceAIdentityShaBefore": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "instanceAIdentityShaAfter": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "keyserverIdentitiesBefore": ["identity-a"],
        "keyserverIdentitiesAfter": ["identity-a", "identity-b"]
    })
}

fn run_fixture(
    root: &Path,
    fixture: &serde_json::Value,
    confirm: bool,
) -> (Output, serde_json::Value) {
    let ps = match powershell() {
        Some(ps) => ps,
        None => {
            eprintln!("skipping osl-launch-instance-b behavior test: PowerShell is unavailable");
            let mut skip_command = if cfg!(windows) {
                let mut command = Command::new("cmd");
                command.args(["/C", "exit", "/B", "0"]);
                command
            } else {
                Command::new("true")
            };
            return (
                skip_command.output().expect("run skip command"),
                serde_json::json!({"skipped": true}),
            );
        }
    };
    fs::create_dir_all(root).expect("create launcher fixture root");
    let fixture_path = root.join("fixture.json");
    let out_path = root.join("launch-result.json");
    fs::write(
        &fixture_path,
        serde_json::to_vec(fixture).expect("serialize launcher fixture"),
    )
    .expect("write launcher fixture");

    let script = repo_root().join("scripts/qa/osl-launch-instance-b.ps1");
    let mut command = Command::new(ps);
    command.args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"]);
    command.arg(&script);
    command.arg("-ExeB");
    command.arg(root.join("unused-fixture-exe.exe"));
    command.arg("-BundleB");
    command.arg("org.oslprivacy.hubqab");
    command.arg("-JsonOut");
    command.arg(&out_path);
    command.arg("-TempRootB");
    command.arg(
        fixture["tempRootB"]
            .as_str()
            .expect("fixture tempRootB is a string"),
    );
    command.arg("-ContractFixtureJson");
    command.arg(&fixture_path);
    command.arg("-Quiet");
    if confirm {
        command.arg("-ConfirmCreatesIdentity");
    }

    let output = command.output().expect("run osl-launch-instance-b fixture");
    let payload: serde_json::Value =
        serde_json::from_slice(&fs::read(&out_path).expect("fixture writes JSON output"))
            .expect("launcher fixture output is JSON");
    (output, payload)
}

fn step<'a>(payload: &'a serde_json::Value, name: &str) -> &'a serde_json::Value {
    payload["steps"]
        .as_array()
        .expect("payload steps are an array")
        .iter()
        .filter(|step| step["step"] == name)
        .last()
        .unwrap_or_else(|| panic!("missing step {name}"))
}

#[test]
fn instance_b_launcher_uses_private_temp_root_and_preserves_instance_a() {
    let root = scratch("temp-and-a");
    let fixture = base_fixture(&root);
    let (output, payload) = run_fixture(&root, &fixture, true);
    if payload.get("skipped").and_then(|value| value.as_bool()) == Some(true) {
        return;
    }

    assert!(
        output.status.success(),
        "launcher fixture failed with status {:?}\nstdout:\n{}\nstderr:\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(payload["overall"]["verdict"], "ok");
    assert_ne!(
        payload["instanceA"]["tempRoot"],
        payload["instanceB"]["tempRoot"]
    );
    assert_eq!(payload["instanceA"]["identityUnchangedAcrossLaunch"], true);
    assert_eq!(payload["instanceA"]["touchedByThisScript"], false);
    assert_eq!(payload["instanceB"]["tempRootHonouredByChild"], true);
    assert!(root.join("temp-b/osl-startup-trace.txt").is_file());
    assert!(!root.join("temp-a/osl-startup-trace.txt").exists());

    let shared_root = scratch("shared-temp");
    let mut shared = base_fixture(&shared_root);
    shared["tempRootB"] = shared["tempRootA"].clone();
    let (shared_output, shared_payload) = run_fixture(&shared_root, &shared, true);
    assert_eq!(
        shared_output.status.code(),
        Some(2),
        "shared temp root must block\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&shared_output.stdout),
        String::from_utf8_lossy(&shared_output.stderr)
    );
    assert_eq!(shared_payload["overall"]["verdict"], "blocked");
    assert_eq!(step(&shared_payload, "temp-isolation")["result"], "failed");
    assert!(!shared_root.join("temp-a/osl-startup-trace.txt").exists());

    let changed_a_root = scratch("changed-a");
    let mut changed_a = base_fixture(&changed_a_root);
    changed_a["instanceAIdentityShaAfter"] =
        serde_json::json!("cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc");
    let (changed_output, changed_payload) = run_fixture(&changed_a_root, &changed_a, true);
    assert_eq!(
        changed_output.status.code(),
        Some(1),
        "changed instance A identity must fail\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&changed_output.stdout),
        String::from_utf8_lossy(&changed_output.stderr)
    );
    assert_eq!(changed_payload["overall"]["verdict"], "failed");
    assert_eq!(
        step(&changed_payload, "assert/instance-a-untouched")["result"],
        "failed"
    );

    let _ = fs::remove_dir_all(root);
    let _ = fs::remove_dir_all(shared_root);
    let _ = fs::remove_dir_all(changed_a_root);
}

#[test]
fn instance_b_confirm_creates_identity_registers_second_identity() {
    let refused_root = scratch("consent-refused");
    let fixture = base_fixture(&refused_root);
    let (refused_output, refused_payload) = run_fixture(&refused_root, &fixture, false);
    if refused_payload
        .get("skipped")
        .and_then(|value| value.as_bool())
        == Some(true)
    {
        return;
    }

    assert_eq!(
        refused_output.status.code(),
        Some(2),
        "missing consent must block\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&refused_output.stdout),
        String::from_utf8_lossy(&refused_output.stderr)
    );
    assert_eq!(refused_payload["overall"]["verdict"], "blocked");
    assert_eq!(step(&refused_payload, "gate/consent")["result"], "failed");
    assert!(
        !refused_payload["steps"]
            .as_array()
            .expect("payload steps are an array")
            .iter()
            .any(|step| step["step"] == "identity/keyserver-registration"),
        "missing consent must stop before registration is evaluated"
    );

    let allowed_root = scratch("consent-allowed");
    let allowed_fixture = base_fixture(&allowed_root);
    let (allowed_output, allowed_payload) = run_fixture(&allowed_root, &allowed_fixture, true);
    assert!(
        allowed_output.status.success(),
        "confirmed launcher fixture failed with status {:?}\nstdout:\n{}\nstderr:\n{}",
        allowed_output.status.code(),
        String::from_utf8_lossy(&allowed_output.stdout),
        String::from_utf8_lossy(&allowed_output.stderr)
    );
    assert_eq!(allowed_payload["overall"]["verdict"], "ok");
    let identity_step = step(&allowed_payload, "identity/keyserver-registration");
    assert_eq!(identity_step["result"], "ok");
    assert_eq!(identity_step["identitiesBefore"], 1);
    assert_eq!(identity_step["identitiesAfter"], 2);
    assert_eq!(
        allowed_payload["instanceB"]["registeredSecondIdentity"],
        true
    );

    let bad_registration_root = scratch("bad-registration");
    let mut bad_registration = base_fixture(&bad_registration_root);
    bad_registration["keyserverIdentitiesAfter"] = serde_json::json!(["identity-a"]);
    let (bad_output, bad_payload) = run_fixture(&bad_registration_root, &bad_registration, true);
    assert_eq!(
        bad_output.status.code(),
        Some(1),
        "missing second keyserver identity must fail\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&bad_output.stdout),
        String::from_utf8_lossy(&bad_output.stderr)
    );
    assert_eq!(bad_payload["overall"]["verdict"], "failed");
    assert_eq!(
        step(&bad_payload, "identity/keyserver-registration")["result"],
        "failed"
    );

    let _ = fs::remove_dir_all(refused_root);
    let _ = fs::remove_dir_all(allowed_root);
    let _ = fs::remove_dir_all(bad_registration_root);
}
