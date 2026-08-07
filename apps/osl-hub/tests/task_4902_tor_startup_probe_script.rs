use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
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

fn install_fake_tools(bin: &Path, window_visible: bool) {
    fs::create_dir_all(bin).expect("fake bin can be created");
    let sleeper = r#"#!/usr/bin/env bash
trap 'exit 0' TERM INT
while true; do sleep 1; done
"#;
    write_executable(&bin.join("Xvfb"), sleeper);
    write_executable(&bin.join("matchbox-window-manager"), sleeper);
    write_executable(
        &bin.join("xdotool"),
        if window_visible {
            r#"#!/usr/bin/env bash
if [ "$1" = "search" ]; then
  printf '4902\n'
  exit 0
fi
exit 2
"#
        } else {
            r#"#!/usr/bin/env bash
if [ "$1" = "search" ]; then
  exit 1
fi
exit 2
"#
        },
    );
    write_executable(
        &bin.join("xwininfo"),
        r#"#!/usr/bin/env bash
cat <<'OUT'
xwininfo: Window id: 0x1326 "OSL Privacy"
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
width, height = 64, 64
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
    write_executable(
        &bin.join("identify"),
        r#"#!/usr/bin/env bash
printf '64'
"#,
    );
}

fn run_probe(scratch: &Scratch, window_visible: bool, disable_starter: &str) -> std::process::Output {
    let fake_bin = scratch.join("bin");
    install_fake_tools(&fake_bin, window_visible);
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
        .arg(repo_root().join("scripts/qa/tor-4902-startup-paint.sh"))
        .env("PATH", path)
        .env("OSL_TOR_4902_BIN", &fake_app)
        .env("OSL_TOR_4902_OUT", scratch.join("out"))
        .env("OSL_TOR_4902_RUN_DIR", scratch.join("run"))
        .env("OSL_TOR_4902_DISPLAY", ":92")
        .env("OSL_TOR_4902_PAINT_DEADLINE_SECONDS", "1")
        .env("OSL_TOR_4902_DISABLE_TOR_STARTER", disable_starter)
        .output()
        .expect("probe script can be launched")
}

#[test]
fn tor_4902_probe_reports_red_when_no_window_paints() {
    let scratch = Scratch::new("task-4902-red");
    let output = run_probe(&scratch, false, "0");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    println!("TOR_4902_SCRIPT_RED_EXIT={:?}", output.status.code());
    println!("{stdout}");
    assert_eq!(output.status.code(), Some(1), "stdout={stdout}; stderr={stderr}");
    assert!(stdout.contains("TOR_4902_PAINTED_WINDOWS_BY_SECOND_3=0"));
    assert!(stdout.contains("TOR-4902-RED"));
}

#[test]
fn tor_4902_probe_reports_instrument_when_disabled_starter_paints() {
    let scratch = Scratch::new("task-4902-instrument");
    let output = run_probe(&scratch, true, "1");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    println!("TOR_4902_SCRIPT_INSTRUMENT_EXIT={:?}", output.status.code());
    println!("{stdout}");
    assert_eq!(output.status.code(), Some(0), "stdout={stdout}; stderr={stderr}");
    assert!(stdout.contains("TOR_4902_PAINTED_WINDOWS_BY_SECOND_3=1"));
    assert!(stdout.contains("TOR-4902-INSTRUMENT"));
}
