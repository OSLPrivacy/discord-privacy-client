use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const MARKER: &[u8] = b"PERSONAL_CONVERSATION_MARKER_5102";
const MARKER_ASSERTION_TOKEN: &str = "TASK5102_ASSERT_PERSONAL_MARKER_ZERO";

fn pngs(output_dir: &Path) -> Vec<PathBuf> {
    if !output_dir.exists() {
        return Vec::new();
    }
    fs::read_dir(output_dir)
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("png"))
        .collect()
}

#[test]
fn privacy_guards_refuse_broad_or_unbound_capture_before_any_pixel_write() {
    // These preflight assertions deliberately precede fixture execution. The
    // 5102b mutation run removes each actual guard together with its marker and
    // proves this privacy test goes red while its artifact directory is empty.
    let library_source = include_str!("../src/lib.rs");
    assert!(library_source.contains("PRIVACY_GUARD_VERIFIED_HWND_CROP"));
    assert!(library_source.contains("PRIVACY_GUARD_SECOND_UIA_OBSERVATION"));
    let this_test = include_str!("task_5102_privacy_boundary.rs");
    assert_eq!(
        this_test.matches(MARKER_ASSERTION_TOKEN).count(),
        2,
        "the break-it marker assertion was starved"
    );

    let temporary;
    let output_dir = if let Some(path) = std::env::var_os("TASK_5102_PRIVACY_OUTPUT") {
        PathBuf::from(path)
    } else {
        temporary = tempfile::tempdir().unwrap();
        temporary.path().to_path_buf()
    };
    fs::create_dir_all(&output_dir).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_carrier-reference-fixture"))
        .arg(&output_dir)
        .arg("desktop-source")
        .output()
        .unwrap();
    let marker_occurrences = fs::read_dir(&output_dir)
        .unwrap()
        .filter_map(Result::ok)
        .filter_map(|entry| fs::read(entry.path()).ok())
        .map(|bytes| {
            bytes
                .windows(MARKER.len())
                .filter(|window| *window == MARKER)
                .count()
        })
        .sum::<usize>();
    assert_eq!(
        marker_occurrences, 0,
        "TASK5102_ASSERT_PERSONAL_MARKER_ZERO"
    );
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(pngs(&output_dir).len(), 0);
}
