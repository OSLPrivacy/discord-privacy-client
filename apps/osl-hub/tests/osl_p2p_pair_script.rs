use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn pair_script() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("scripts/qa/osl-p2p-pair.ps1")
}

/// A Windows-hosted PowerShell, and how to spell paths for it.
///
/// `osl-p2p-pair.ps1` dot-sources `osl-p2p-win32.ps1`, which P/Invokes user32
/// before any gate runs, so a native Unix `pwsh` cannot execute this script at
/// all (it dies in `SetProcessDPIAware`). Only a Windows-hosted interpreter is
/// accepted here. Reached from WSL, that interpreter resolves a Linux-style
/// absolute path against its *current location* instead of treating it as
/// rooted, so every path handed to it is translated first.
struct WindowsShell {
    command: &'static str,
    translate_paths: bool,
}

const HOST_KIND_PROBE: &str =
    "if ($PSVersionTable.PSVersion.Major -le 5 -or $IsWindows) { 'windows' } else { 'unix' }";

fn windows_powershell() -> Option<WindowsShell> {
    for candidate in ["powershell.exe", "pwsh.exe", "pwsh", "powershell"] {
        let Ok(output) = Command::new(candidate)
            .args(["-NoProfile", "-Command", HOST_KIND_PROBE])
            .output()
        else {
            continue;
        };
        if !output.status.success() {
            continue;
        }
        if String::from_utf8_lossy(&output.stdout)
            .trim()
            .to_lowercase()
            != "windows"
        {
            continue;
        }
        return Some(WindowsShell {
            command: candidate,
            translate_paths: !cfg!(windows),
        });
    }
    None
}

impl WindowsShell {
    fn path(&self, path: &Path) -> String {
        if !self.translate_paths {
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

/// Windows PowerShell's `-Encoding utf8` prepends a byte-order mark, which is
/// not valid JSON input for a strict parser.
fn parse_json(bytes: &[u8]) -> serde_json::Value {
    let text = String::from_utf8_lossy(bytes);
    serde_json::from_str(text.trim_start_matches('\u{feff}')).expect("refusal receipt is JSON")
}

/// The refusal must be provable even where no Windows PowerShell can be reached,
/// so the gate is first audited in the script's own source: the same-`osl_user_id`
/// comparison must fail closed, and it must be reached *before* any peer offer is
/// copied between the two profile roots.
#[test]
fn osl_p2p_pair_source_refuses_same_osl_user_id_before_any_copy() {
    let source = fs::read_to_string(pair_script()).expect("read osl-p2p-pair.ps1");

    let gate = source
        .find("if ($jA.osl_user_id -eq $jB.osl_user_id) {")
        .expect("script compares the two offers' osl_user_id values");
    let refusal = source[gate..]
        .find("Add-S 'gate/two-identities' 'failed'")
        .map(|offset| gate + offset)
        .expect("same osl_user_id records a failed gate/two-identities step");
    let finish = source[refusal..]
        .find("Finish 'failed'")
        .map(|offset| refusal + offset)
        .expect("same osl_user_id finishes with the failed verdict (exit 1)");

    let first_copy = source
        .find("Copy-Item -LiteralPath $pair.src")
        .expect("script copies the peer offers once the gate passes");
    assert!(
        finish < first_copy,
        "the same-identity refusal must fail closed before any peer offer is copied \
         (gate at {gate}, refusal at {refusal}, finish at {finish}, first copy at {first_copy})"
    );

    assert!(
        source.contains(
            "if ($Verdict -eq 'ok') { exit 0 } elseif ($Verdict -eq 'failed') { exit 1 }"
        ),
        "the failed verdict must exit non-zero"
    );
}

#[test]
fn osl_p2p_pair_refuses_same_osl_user_id() {
    let Some(shell) = windows_powershell() else {
        eprintln!(
            "skipping osl-p2p-pair behaviour test: no Windows-hosted PowerShell is reachable; \
             the source-level gate audit still covers the refusal"
        );
        return;
    };

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

    let script = pair_script();
    let mut command = Command::new(shell.command);
    command.args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"]);
    command.arg(shell.path(&script));
    command.arg("-BundleA");
    command.arg(bundle_a);
    command.arg("-BundleB");
    command.arg(bundle_b);
    command.arg("-JsonOut");
    command.arg(shell.path(&json_out));
    command.arg("-Quiet");
    command.env("APPDATA", &appdata);
    if shell.translate_paths {
        // WSL hands no environment to a Windows process unless WSLENV names it;
        // `/wp` means "outbound to Win32, translate it as a path".
        command.env("WSLENV", "APPDATA/wp");
    }

    let output = command
        .output()
        .expect("run osl-p2p-pair.ps1 with isolated APPDATA");

    assert_eq!(
        output.status.code(),
        Some(1),
        "same osl_user_id must be refused with exit 1\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let payload = parse_json(&fs::read(&json_out).expect("refusal writes a JSON receipt"));
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
