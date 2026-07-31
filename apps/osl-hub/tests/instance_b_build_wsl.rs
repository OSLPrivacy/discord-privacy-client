use serde_json::Value;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

struct Scratch {
    path: PathBuf,
}

impl Scratch {
    fn new(name: &str) -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock is after unix epoch")
            .as_nanos();
        let path = env::temp_dir().join(format!("{name}-{}-{nonce}", std::process::id()));
        fs::create_dir_all(&path).expect("scratch directory can be created");
        Self { path }
    }

    fn join(&self, relative: &str) -> PathBuf {
        self.path.join(relative)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("apps/osl-hub has a repository root")
        .to_path_buf()
}

fn run_script(script: &Path, arg: &str, envs: &[(&str, &Path)]) -> Output {
    let mut command = Command::new("bash");
    command.arg(script).arg(arg);
    // The script serialises its build behind the GLOBAL osl-cargo lock. This test
    // runs under osl-cargo, which already holds that lock, so letting the script
    // take it deadlocks the whole proof pass. Point it at a per-test lock; the
    // script's behaviour is unchanged, it just no longer contends with its runner.
    let isolated_lock = std::env::temp_dir().join(format!(
        "osl-cargo-test-{}-{}.lock",
        std::process::id(),
        arg.replace(['/', ' '], "_")
    ));
    command.env("OSL_CARGO_LOCK", &isolated_lock);
    for (key, value) in envs {
        command.env(key, value);
    }
    command
        .output()
        .expect("instance-B build script can be launched")
}

fn read_json(path: &Path) -> Value {
    let text = fs::read_to_string(path).expect("JSON receipt is written");
    serde_json::from_str(&text).expect("JSON receipt is valid")
}

fn write_file(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("parent directory can be created");
    }
    fs::write(path, contents).expect("fixture file can be written");
}

fn touch_at(path: &Path, when: &str) {
    let status = Command::new("touch")
        .arg("-d")
        .arg(when)
        .arg(path)
        .status()
        .expect("touch can set fixture mtime");
    assert!(status.success(), "touch failed for {}", path.display());
}

#[test]
fn instance_b_build_requires_distinct_identifier() {
    let root = repo_root();
    let script = root.join("scripts/qa/osl-instance-b-build-wsl.sh");
    let scratch = Scratch::new("osl-instance-b-build-wsl");

    let same_id_json = scratch.join("same-id.json");
    let same_id_log = scratch.join("same-id.log");
    let same_id = run_script(
        &script,
        "org.oslprivacy.hub",
        &[
            ("JSON_OUT", &same_id_json),
            ("LOG", &same_id_log),
            ("BUNDLE_A", Path::new("org.oslprivacy.hub")),
        ],
    );
    assert_eq!(
        same_id.status.code(),
        Some(2),
        "same identifier must be refused before any build; stdout={}, stderr={}",
        String::from_utf8_lossy(&same_id.stdout),
        String::from_utf8_lossy(&same_id.stderr)
    );
    let same_id_receipt = read_json(&same_id_json);
    assert_eq!(same_id_receipt["identifier"], "org.oslprivacy.hub");
    assert_eq!(same_id_receipt["overall"]["verdict"], "blocked");
    assert!(
        same_id_receipt["overall"]["diagnosis"]
            .as_str()
            .expect("diagnosis is a string")
            .contains("Identifier equals instance A"),
        "same-id refusal must explain the identity collision: {same_id_receipt:?}"
    );
    assert!(
        same_id_receipt["diffKey"]
            .as_str()
            .expect("diffKey is a string")
            .starts_with("BLOCKED:"),
        "blocked receipt must not look like a successful diff key: {same_id_receipt:?}"
    );

    let fake_repo = scratch.join("repo");
    let fake_bin = scratch.join("bin");
    let stage = scratch.join("stage");
    let temp_root = scratch.join("win-temp");
    let dll = scratch.join("WebView2Loader.dll");
    let cargo_args = scratch.join("cargo-args.txt");
    let cargo_cwd = scratch.join("cargo-cwd.txt");
    let tauri_config = scratch.join("tauri-config.json");
    fs::create_dir_all(&fake_bin).expect("fake bin directory can be created");
    fs::create_dir_all(&stage).expect("stage directory can be created");
    fs::create_dir_all(&temp_root).expect("temp root directory can be created");
    write_file(&fake_repo.join("apps/osl-hub/tauri.conf.json"), "{}\n");
    write_file(
        &fake_repo.join("apps/osl-hub-ui/src/App.tsx"),
        "source-v1\n",
    );
    write_file(
        &fake_repo.join("apps/osl-hub-ui/dist/index.html"),
        "dist-v1\n",
    );
    write_file(&dll, "dll\n");
    touch_at(
        &fake_repo.join("apps/osl-hub-ui/src/App.tsx"),
        "2026-01-01 00:00:00 UTC",
    );
    touch_at(
        &fake_repo.join("apps/osl-hub-ui/dist/index.html"),
        "2026-01-02 00:00:00 UTC",
    );

    let fake_cargo = fake_bin.join("cargo");
    write_file(
        &fake_cargo,
        r#"#!/usr/bin/env bash
set -eu
: "${FAKE_CARGO_ARGS:?}"
: "${FAKE_CARGO_CWD:?}"
: "${FAKE_TAURI_CONFIG:?}"
printf '%s\n' "$PWD" > "$FAKE_CARGO_CWD"
printf '%s\n' "$@" > "$FAKE_CARGO_ARGS"
printf '%s\n' "${TAURI_CONFIG:-}" > "$FAKE_TAURI_CONFIG"
mkdir -p target/x86_64-pc-windows-gnu/debug
printf '%s\n' "${TAURI_CONFIG:-}" > target/x86_64-pc-windows-gnu/debug/osl-privacy-hub.exe
"#,
    );
    #[cfg(unix)]
    {
        let mut permissions = fs::metadata(&fake_cargo)
            .expect("fake cargo metadata is readable")
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&fake_cargo, permissions).expect("fake cargo can be made executable");
    }

    let distinct_json = scratch.join("distinct-id.json");
    let distinct_log = scratch.join("distinct-id.log");
    let mut path_value = fake_bin.into_os_string();
    path_value.push(":");
    path_value.push(env::var_os("PATH").unwrap_or_default());

    let mut command = Command::new("bash");
    let distinct = command
        .arg(&script)
        .arg("org.oslprivacy.hubqab")
        .env("REPO", &fake_repo)
        .env("JSON_OUT", &distinct_json)
        .env("LOG", &distinct_log)
        .env("STAGE", &stage)
        .env("OSL_WIN_TEMP_ROOT", &temp_root)
        .env("DLL_SRC", &dll)
        .env("FAKE_CARGO_ARGS", &cargo_args)
        .env("FAKE_CARGO_CWD", &cargo_cwd)
        .env("FAKE_TAURI_CONFIG", &tauri_config)
        // This case builds its own Command instead of going through run_script, so it
        // needs the isolated lock too. Without it the script takes the GLOBAL
        // osl-cargo lock that this very test already holds via osl-cargo, and hangs
        // forever -- even though the build itself is a fake cargo.
        .env(
            "OSL_CARGO_LOCK",
            std::env::temp_dir().join(format!("osl-cargo-test-{}-distinct.lock", std::process::id())),
        )
        .env("PATH", path_value)
        .output()
        .expect("distinct identifier build can be launched");
    assert!(
        distinct.status.success(),
        "distinct identifier fake build must pass; stdout={}, stderr={}",
        String::from_utf8_lossy(&distinct.stdout),
        String::from_utf8_lossy(&distinct.stderr)
    );

    let staged_exe =
        fs::read_to_string(stage.join("osl-privacy-hub.exe")).expect("staged exe is copied");
    assert!(
        staged_exe.contains(r#""identifier":"org.oslprivacy.hubqab""#),
        "staged exe must contain the B identifier overlay: {staged_exe}"
    );
    assert!(
        !staged_exe.contains(r#""identifier":"org.oslprivacy.hub""#),
        "staged exe must not retain instance A's identifier: {staged_exe}"
    );
    assert_eq!(
        fs::read_to_string(&cargo_cwd)
            .expect("fake cargo cwd is captured")
            .trim_end(),
        fake_repo
            .join("apps/osl-hub")
            .to_str()
            .expect("fixture path is utf-8"),
        "cargo must run from apps/osl-hub so the hub-local target directory is searched"
    );
    let captured_args = fs::read_to_string(&cargo_args).expect("fake cargo args are captured");
    assert_eq!(
        captured_args.lines().collect::<Vec<_>>(),
        [
            "build",
            "--features",
            "desktop,discord-qa-shell",
            "--bin",
            "osl-privacy-hub",
            "--target",
            "x86_64-pc-windows-gnu",
        ],
        "instance B build must use the desktop QA shell Windows GNU target"
    );
    let overlay = read_json(&tauri_config);
    assert_eq!(
        overlay,
        serde_json::json!({ "identifier": "org.oslprivacy.hubqab" }),
        "TAURI_CONFIG must carry only the distinct B identifier"
    );
    let distinct_receipt = read_json(&distinct_json);
    assert_eq!(distinct_receipt["overall"]["verdict"], "ok");
    assert_eq!(distinct_receipt["identifier"], "org.oslprivacy.hubqab");
    assert_ne!(distinct_receipt["identifier"], "org.oslprivacy.hub");
    assert!(
        distinct_receipt["diffKey"]
            .as_str()
            .expect("diffKey is a string")
            .starts_with("OK:org.oslprivacy.hubqab:"),
        "success receipt must be keyed by the B identifier: {distinct_receipt:?}"
    );
}
