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
  printf '4242\n'
  exit 0
fi
exit 2
"#,
    );
    write_executable(
        &bin.join("xwininfo"),
        r#"#!/usr/bin/env bash
cat <<'OUT'
xwininfo: Window id: 0x1092 "OSL Privacy"
  Width: 1440
  Height: 900
  Map State: IsViewable
OUT
"#,
    );
    write_executable(
        &bin.join("import"),
        r#"#!/usr/bin/env bash
python3 - "$3" <<'PY'
import binascii
import struct
import sys
import zlib

path = sys.argv[1]
width, height = 1440, 900
def chunk(kind, body):
    return struct.pack(">I", len(body)) + kind + body + struct.pack(">I", binascii.crc32(kind + body) & 0xffffffff)
rows = bytearray()
for y in range(height):
    rows.append(0)
    for x in range(width):
        rows.extend((x & 255, y & 255, (x + y) & 255))
png = b"\x89PNG\r\n\x1a\n"
png += chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0))
png += chunk(b"IDAT", zlib.compress(bytes(rows), 6))
png += chunk(b"IEND", b"")
open(path, "wb").write(png)
PY
"#,
    );
}

fn run_starter(scratch: &Scratch, capture: &str) -> Output {
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
        .env("OSL_FIXED_SCREEN_DISPLAY", ":63")
        .env("OSL_FIXED_SCREEN_WAIT_SECONDS", "2")
        .env("OSL_FIXED_SCREEN_CAPTURE", capture)
        .output()
        .expect("starter script can be launched")
}

#[test]
fn fixed_screen_starter_saves_one_image_and_metadata_then_cleans_up() {
    let scratch = Scratch::new("task-0063-starter-green");
    let output = run_starter(&scratch, "1");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "starter must pass; stdout={stdout}; stderr={stderr}"
    );
    assert!(
        stdout.contains("TASK0063_SWITCHES display=:63 screen=1440x900x24 capture=1"),
        "starter must print its fixed-screen switches: {stdout}"
    );
    assert!(
        stdout.contains("TASK0063_CAPTURE_CHECK status=ok image_count=1 metadata_count=1"),
        "starter must save exactly one image and one metadata file: {stdout}"
    );
    assert!(
        stdout.contains("TASK0063_CLEANUP fake_screen_alive=no display=:63"),
        "starter must clean up the fake screen: {stdout}"
    );
    assert_eq!(
        fs::read_dir(scratch.join("out"))
            .expect("out dir exists")
            .filter(|entry| {
                entry
                    .as_ref()
                    .expect("dir entry")
                    .path()
                    .extension()
                    .is_some_and(|ext| ext == "png")
            })
            .count(),
        1
    );
    let metadata = fs::read_to_string(scratch.join("out/osl-fixed-screen.json"))
        .expect("metadata json is written");
    assert!(metadata.contains(r#""schema": "osl-fixed-screen-starter-v1""#));
    assert!(metadata.contains(r#""captureEnabled": true"#));
}

#[test]
fn fixed_screen_starter_fails_when_capture_is_disabled() {
    let scratch = Scratch::new("task-0063-starter-red");
    let output = run_starter(&scratch, "0");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        output.status.code(),
        Some(1),
        "disabled capture must fail; stdout={stdout}; stderr={stderr}"
    );
    assert!(
        stdout.contains("TASK0063_CAPTURE_SKIPPED capture=0"),
        "disabled capture path must be explicit: {stdout}"
    );
    assert!(
        stderr.contains(
            "TASK0063_CAPTURE_CHECK status=fail image_count=0 metadata_count=0 capture_status=64 bytes=0 colors=0"
        ),
        "disabled capture must make the check red: {stderr}"
    );
    assert!(
        stdout.contains("TASK0063_CLEANUP fake_screen_alive=no display=:63"),
        "failure path must still clean up the fake screen: {stdout}"
    );
}
