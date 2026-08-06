use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

const MISSING_TARGET: &str = "missing-screen-target-0066";

struct Scratch {
    path: PathBuf,
}

impl Scratch {
    fn new(name: &str) -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock is after unix epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("{name}-{}-{nonce}", std::process::id()));
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

fn write_executable(path: &Path, contents: &str) {
    fs::write(path, contents).expect("fixture executable can be written");
    #[cfg(unix)]
    {
        let mut permissions = fs::metadata(path)
            .expect("fixture metadata is readable")
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions).expect("fixture executable can be chmodded");
    }
}

fn install_fake_tools(bin: &Path) {
    fs::create_dir_all(bin).expect("fake bin can be created");
    let sleeper = r#"#!/usr/bin/env bash
trap 'exit 0' TERM INT
while true; do sleep 1; done
"#;
    write_executable(&bin.join("Xvfb"), sleeper);
    write_executable(&bin.join("matchbox-window-manager"), sleeper);
    write_executable(
        &bin.join("xdotool"),
        r#"#!/usr/bin/env bash
if [ "$1" = "search" ]; then
  exit 1
fi
exit 2
"#,
    );
    write_executable(
        &bin.join("xwininfo"),
        r#"#!/usr/bin/env bash
printf 'xwininfo should not run for a missing target\n' >&2
exit 3
"#,
    );
    write_executable(
        &bin.join("import"),
        r#"#!/usr/bin/env bash
printf 'import should not run for a missing target\n' >&2
exit 4
"#,
    );
    write_executable(
        &bin.join("identify"),
        r#"#!/usr/bin/env bash
printf 'identify should not run for a missing target\n' >&2
exit 5
"#,
    );
}

fn run_missing_target_starter(scratch: &Scratch) -> Output {
    let root = repo_root();
    let fake_bin = scratch.join("bin");
    install_fake_tools(&fake_bin);
    let fake_app = scratch.join("fake-osl");
    write_executable(
        &fake_app,
        r#"#!/usr/bin/env bash
trap 'exit 0' TERM INT
while true; do sleep 1; done
"#,
    );
    let mut path = fake_bin.into_os_string();
    path.push(":");
    path.push(std::env::var_os("PATH").unwrap_or_default());

    Command::new("bash")
        .arg(root.join("scripts/qa/osl-fixed-screen-test-starter.sh"))
        .env("PATH", path)
        .env("OSL_FIXED_SCREEN_BIN", &fake_app)
        .env("OSL_FIXED_SCREEN_OUT", scratch.join("out"))
        .env("OSL_FIXED_SCREEN_RUN_DIR", scratch.join("run"))
        .env("OSL_FIXED_SCREEN_DISPLAY", ":66")
        .env("OSL_FIXED_SCREEN_WAIT_SECONDS", "1")
        .env("OSL_FIXED_SCREEN_WINDOW_NAME", MISSING_TARGET)
        .output()
        .expect("starter script can be launched")
}

#[test]
fn fixed_screen_starter_records_missing_target_and_cleans_up() {
    let scratch = Scratch::new("task-0066-missing-screen-target");
    let output = run_missing_target_starter(&scratch);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    println!("TASK0066_STARTER_EXIT={:?}", output.status.code());
    println!("TASK0066_FAILED_TARGET={MISSING_TARGET}");

    assert_eq!(
        output.status.code(),
        Some(1),
        "missing screen target must make the starter exit 1; stdout={stdout}; stderr={stderr}"
    );
    assert!(
        stderr.contains("TASK0063_FAILED_TARGET")
            && stderr.contains("target=missing-screen-target-0066")
            && stderr.contains("reason=fixed-screen\\ window\\ not\\ found"),
        "starter must record the failed target; stderr={stderr}"
    );
    let failed_target_line = stderr
        .lines()
        .find(|line| line.starts_with("TASK0063_FAILED_TARGET"))
        .expect("starter printed failed target line");
    println!("TASK0066_STARTER_FAILED_TARGET_LINE={failed_target_line}");
    assert!(
        stderr.contains("TASK0063_ERROR fixed-screen window not found: missing-screen-target-0066"),
        "starter must name the missing target in the fatal error; stderr={stderr}"
    );
    assert!(
        stdout.contains("TASK0063_CLEANUP fake_screen_alive=no display=:66"),
        "missing target failure must clean up the fake screen; stdout={stdout}"
    );
    let cleanup_line = stdout
        .lines()
        .find(|line| line.starts_with("TASK0063_CLEANUP"))
        .expect("starter printed cleanup line");
    println!("TASK0066_STARTER_CLEANUP_LINE={cleanup_line}");

    let image_count = fs::read_dir(scratch.join("out"))
        .expect("out dir exists")
        .filter(|entry| {
            entry
                .as_ref()
                .expect("dir entry")
                .path()
                .extension()
                .is_some_and(|ext| ext == "png")
        })
        .count();
    println!("TASK0066_IMAGE_COUNT={image_count}");
    assert_eq!(
        image_count, 0,
        "missing target must not write a blank successful image"
    );

    let failure_record_path = scratch.join("out/osl-fixed-screen-failure.json");
    let failure_record: Value = serde_json::from_str(
        &fs::read_to_string(&failure_record_path).expect("failure metadata is written"),
    )
    .expect("failure metadata is valid JSON");
    println!(
        "TASK0066_FAILURE_RECORD_SCHEMA={}",
        failure_record
            .get("schema")
            .and_then(Value::as_str)
            .expect("schema is present")
    );
    println!(
        "TASK0066_FAILURE_RECORD_TARGET={}",
        failure_record
            .get("targetWindowName")
            .and_then(Value::as_str)
            .expect("targetWindowName is present")
    );
    println!(
        "TASK0066_FAILURE_RECORD_REASON={}",
        failure_record
            .get("failure")
            .and_then(Value::as_str)
            .expect("failure is present")
    );
    assert_eq!(
        failure_record.get("schema").and_then(Value::as_str),
        Some("osl-fixed-screen-starter-failure-v1")
    );
    assert_eq!(
        failure_record
            .get("targetWindowName")
            .and_then(Value::as_str),
        Some(MISSING_TARGET)
    );
    assert_eq!(
        failure_record.get("failure").and_then(Value::as_str),
        Some("fixed-screen window not found")
    );
}
