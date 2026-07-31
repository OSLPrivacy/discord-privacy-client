use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const TEST_NAME: &str = "disposable_discord_accounts_load_from_key_vault_without_logging_secrets";

#[test]
fn disposable_discord_accounts_load_from_key_vault_without_logging_secrets() {
    let script = repo_root().join("scripts/qa/osl-vm-unlock-from-key-vault.ps1");

    let named = run_powershell(&script, &["-SelfTest", TEST_NAME]);
    assert!(
        named.status.success(),
        "named Key Vault disposable Discord account self-test failed with status {:?}, stdout bytes {}, stderr bytes {}",
        named.status.code(),
        named.stdout.len(),
        named.stderr.len()
    );
    let expected_ok = format!("ok - {TEST_NAME}");
    assert_eq!(
        as_text(&named.stdout).trim(),
        expected_ok.as_str(),
        "named self-test must return only the bounded public success line"
    );
    assert_no_secret_bearing_material(&named);

    let receipt = run_powershell(&script, &["-RunInternalSelfTest"]);
    assert!(
        receipt.status.success(),
        "internal Key Vault disposable Discord account self-test failed with status {:?}, stdout bytes {}, stderr bytes {}",
        receipt.status.code(),
        receipt.stdout.len(),
        receipt.stderr.len()
    );
    assert_no_secret_bearing_material(&receipt);

    let json: serde_json::Value =
        serde_json::from_str(as_text(&receipt.stdout).trim()).expect("self-test receipt is JSON");
    assert_eq!(
        json.get("Status").and_then(|value| value.as_str()),
        Some("passed"),
        "internal self-test must prove the loader accepted the disposable Key Vault fixture"
    );
    assert_eq!(
        json.get("Test").and_then(|value| value.as_str()),
        Some(TEST_NAME),
        "internal self-test receipt must identify the behavior under proof"
    );
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repository root is reachable from apps/osl-hub")
}

fn run_powershell(script: &Path, args: &[&str]) -> Output {
    let shell = powershell();
    let script = powershell_script_path(script, shell);
    let mut command = Command::new(shell);
    command.args([
        "-NoLogo",
        "-NoProfile",
        "-NonInteractive",
        "-ExecutionPolicy",
        "Bypass",
        "-File",
    ]);
    command.arg(script);
    command.args(args);
    command.output().expect("launch Windows PowerShell")
}

fn powershell() -> &'static str {
    for candidate in ["pwsh", "powershell.exe", "powershell"] {
        if Command::new(candidate)
            .args(["-NoProfile", "-Command", "$PSVersionTable.PSVersion.Major"])
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
        {
            return candidate;
        }
    }
    panic!("PowerShell is required for Key Vault disposable Discord account behavior test");
}

fn powershell_script_path(script: &Path, shell: &str) -> PathBuf {
    if shell != "powershell.exe" || cfg!(windows) {
        return script.to_path_buf();
    }

    let output = Command::new("wslpath")
        .arg("-w")
        .arg(script)
        .output()
        .expect("convert WSL path for Windows PowerShell");
    assert!(
        output.status.success(),
        "wslpath failed while converting {}: {}",
        script.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    PathBuf::from(as_text(&output.stdout).trim())
}

fn assert_no_secret_bearing_material(output: &Output) {
    let public = format!("{}\n{}", as_text(&output.stdout), as_text(&output.stderr));
    for forbidden in [
        "fixture-discord-credential-not-real",
        "fixture-key-vault-token-not-real",
        "fixture-user@example.invalid",
        "osl-test-discord-01",
        "osl-test-discord-02",
        "osl-test-discord-03",
    ] {
        assert!(
            !public.contains(forbidden),
            "script self-test output exposed secret-bearing material"
        );
    }
}

fn as_text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}
