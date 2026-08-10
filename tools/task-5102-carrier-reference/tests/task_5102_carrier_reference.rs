use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;
use std::process::{Command, Output};

fn run(case: &str, output_dir: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_carrier-reference-fixture"))
        .arg(output_dir)
        .arg(case)
        .output()
        .expect("fixture executable runs")
}

fn pngs(output_dir: &Path) -> Vec<std::path::PathBuf> {
    if !output_dir.exists() {
        return Vec::new();
    }
    fs::read_dir(output_dir)
        .expect("read output directory")
        .map(|entry| entry.expect("directory entry").path())
        .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("png"))
        .collect()
}

fn assert_refused_without_pixels(case: &str) {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let output = run(case, temporary.path());
    assert_eq!(output.status.code(), Some(1), "{case} unexpectedly passed");
    assert_eq!(pngs(temporary.path()).len(), 0, "{case} wrote pixels");
}

#[test]
fn fixture_writes_one_exact_bounded_rgb_png_and_complete_non_content_manifest() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let output = run("valid", temporary.path());
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("capture_calls=2"));
    assert!(stdout.contains("zeroized_sources=2"));
    assert!(stdout.contains("requested_source=188x48"));
    assert_eq!(pngs(temporary.path()).len(), 1);

    let png_path = pngs(temporary.path()).pop().expect("one PNG");
    let png = fs::read(&png_path).expect("read PNG");
    assert_eq!(&png[1..4], b"PNG");
    assert_eq!(u32::from_be_bytes(png[16..20].try_into().unwrap()), 188);
    assert_eq!(u32::from_be_bytes(png[20..24].try_into().unwrap()), 48);
    assert_eq!(png[24], 8, "must be 8-bit lossless PNG");
    assert_eq!(png[25], 2, "must be RGB, not indexed/greyscale");

    let manifest_path = fs::read_dir(temporary.path())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| path.extension().and_then(|value| value.to_str()) == Some("json"))
        .expect("manifest");
    let manifest: Value = serde_json::from_slice(&fs::read(manifest_path).unwrap()).unwrap();
    assert_eq!(manifest["carrier"], "Discord");
    assert_eq!(manifest["channel"], "stable");
    assert_eq!(manifest["identity"]["kind"], "signed_executable");
    assert_eq!(manifest["identity"]["signature_status"], "valid");
    assert_eq!(manifest["version"], "1.0.9251");
    assert_eq!(manifest["hwnd_generation"]["hwnd"], 0x5102);
    assert_eq!(manifest["hwnd_generation"]["generation"], 17);
    assert_eq!(manifest["windows_build"], "10.0.26100.4770");
    assert_eq!(manifest["physical_geometry"]["roi"]["left"], 120);
    assert_eq!(
        manifest["physical_geometry"]["capture_with_seam_ring"]["left"],
        116
    );
    assert_eq!(manifest["dpi"], 96);
    assert_eq!(manifest["monitor"]["monitor_id"], "DISPLAY1");
    assert_eq!(manifest["monitor"]["colour_mode"], "SDR 8-bit RGB");
    assert_eq!(manifest["appearance"]["theme"], "dark");
    assert_eq!(manifest["appearance"]["density"], "cozy");
    assert_eq!(manifest["appearance"]["zoom_percent"], 100);
    assert_eq!(manifest["appearance"]["locale"], "en-US");
    assert_eq!(manifest["row_state"], "composer-focused-empty");
    assert_eq!(manifest["pointer_state"], "outside-capture");
    assert_eq!(manifest["caret_state"], "hidden");
    assert_eq!(manifest["hover_state"], "none");
    assert_eq!(manifest["uia_bounds"]["left"], 120);
    assert_eq!(manifest["captured_at_utc"], "2026-08-10T12:34:56Z");
    assert_eq!(manifest["observations_agreeing"], 2);
    assert_eq!(manifest["frames_agreeing"], 2);
    assert_eq!(manifest["seam_ring_physical_px"], 4);
    assert_eq!(manifest["png_mode"], "RGB");
    assert_eq!(manifest["png_width"], 188);
    assert_eq!(manifest["png_height"], 48);
    assert_eq!(manifest["synthetic_test_account"], true);
    let hash: String = Sha256::digest(&png)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    assert_eq!(manifest["png_sha256"], hash);
    let manifest_text = serde_json::to_string(&manifest).unwrap();
    assert!(!manifest_text.contains("personal-conversation-marker"));
}

#[test]
fn wrong_hwnd_occlusion_disagreement_and_too_wide_refuse_before_writing_pixels() {
    for case in [
        "wrong-hwnd",
        "occluded",
        "observation-disagrees",
        "frame-disagrees",
        "too-wide",
    ] {
        assert_refused_without_pixels(case);
    }
}

#[test]
fn starving_verified_hwnd_exits_one_with_zero_pngs() {
    assert_refused_without_pixels("starved");
}

#[test]
fn desktop_sized_or_unbound_source_is_rejected_and_marker_never_reaches_output() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let output = run("desktop-source", temporary.path());
    let marker = b"PERSONAL_CONVERSATION_MARKER_5102";
    let marker_occurrences = fs::read_dir(temporary.path())
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter_map(|path| {
            let bytes = fs::read(&path).ok()?;
            if path.extension().and_then(|value| value.to_str()) == Some("png") {
                let decoder = png::Decoder::new(bytes.as_slice());
                let mut reader = decoder.read_info().ok()?;
                let mut decoded = vec![0; reader.output_buffer_size()];
                let info = reader.next_frame(&mut decoded).ok()?;
                Some(decoded[..info.buffer_size()].to_vec())
            } else {
                Some(bytes)
            }
        })
        .map(|bytes| {
            bytes
                .windows(marker.len())
                .filter(|window| *window == marker)
                .count()
        })
        .sum::<usize>();
    assert_eq!(marker_occurrences, 0, "personal marker reached output");
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(pngs(temporary.path()).len(), 0);
}
