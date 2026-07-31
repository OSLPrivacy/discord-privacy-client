#![cfg(windows)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn osl_p2p_pair_refuses_same_osl_user_id() {
    let scratch = isolated_scratch();
    let appdata = scratch.join("appdata");
    let bundle_a = "org.oslprivacy.selftest.a";
    let bundle_b = "org.oslprivacy.selftest.b";
    let root_a = appdata.join(bundle_a).join("osl-core");
    let root_b = appdata.join(bundle_b).join("osl-core");
    fs::create_dir_all(&root_a).expect("create A profile root");
    fs::create_dir_all(&root_b).expect("create B profile root");

    let offer = br#"{"schemaVersion":1,"osl_user_id":"same-opaque-osl-user"}"#;
    fs::write(root_a.join("discord-qa-offer.v1.json"), offer).expect("write A offer");
    fs::write(root_b.join("discord-qa-offer.v1.json"), offer).expect("write B offer");
    let json_out = scratch.join("pair-result.json");

    let script = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("scripts/qa/osl-p2p-pair.ps1");
    let output = Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
            script.to_str().expect("script path is UTF-8"),
            "-BundleA",
            bundle_a,
            "-BundleB",
            bundle_b,
            "-JsonOut",
            json_out.to_str().expect("JSON output path is UTF-8"),
            "-Quiet",
        ])
        .env("APPDATA", &appdata)
        .output()
        .expect("run osl-p2p-pair.ps1 with isolated APPDATA");

    assert_eq!(
        output.status.code(),
        Some(1),
        "same osl_user_id must be refused with exit 1\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let payload: serde_json::Value =
        serde_json::from_slice(&fs::read(&json_out).expect("refusal writes a JSON receipt"))
            .expect("refusal receipt is JSON");
    assert_eq!(payload["overall"]["verdict"], "failed");
    let last_step = payload["steps"]
        .as_array()
        .and_then(|steps| steps.last())
        .expect("receipt has a final step");
    assert_eq!(last_step["step"], "gate/two-identities");
    assert_eq!(last_step["result"], "failed");
    assert!(
        !root_a.join("discord-qa-peer-offer.v1.json").exists(),
        "same-user refusal must not copy B's offer into A"
    );
    assert!(
        !root_b.join("discord-qa-peer-offer.v1.json").exists(),
        "same-user refusal must not copy A's offer into B"
    );

    fs::remove_dir_all(&scratch).expect("remove isolated scratch");
}

fn isolated_scratch() -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time is after Unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "osl-p2p-pair-script-{}-{nanos}",
        std::process::id()
    ))
}
