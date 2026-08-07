//! TASK 0668: the image comparison screen shows the private original and the
//! prepared post copy side by side with the quality result, and a Linux
//! screenshot contains both pictures and a passed quality result.
//!
//! The prepared post copy is produced by the real image-hidden encoder
//! (TASK 0660) and the quality result is the real read-back check from the
//! 0664/0665 chain: the hidden pointer must decode from the prepared copy and
//! match the sent pointer. The screenshot proof is pixel-level: the private
//! original and prepared post copy regions of the captured screen are
//! byte-compared against the source pictures, and the hidden pointer is then
//! decoded again from the screenshot's prepared-copy region itself.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use stego::{
    decode_png_hidden_pointer, encode_png_hidden_pointer_copy, IMAGE_HIDDEN_CHECK_MARK_BYTES,
    IMAGE_HIDDEN_POINTER_BYTES,
};

const SENT_POINTER: [u8; IMAGE_HIDDEN_POINTER_BYTES] = [
    0x06, 0x68, 0x10, 0x20, 0x30, 0x40, 0x50, 0x60, 0x70, 0x80, 0x90, 0xa0, 0xb0, 0xc0, 0xd0, 0xe0,
    0xf0, 0x0f, 0x1e, 0x2d,
];
const SENT_CHECK_MARK: [u8; IMAGE_HIDDEN_CHECK_MARK_BYTES] = [0xc0, 0xde, 0x06, 0x68];

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crates/stego has a repository root")
        .to_path_buf()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn assert_product_comparison_markup(root: &Path) {
    let source = fs::read_to_string(root.join("apps/osl-hub-ui/src/image-comparison-screen.ts"))
        .expect("image-comparison-screen.ts is readable");
    for required in [
        r#"<h1 id="route-heading" tabindex="-1">Image comparison</h1>"#,
        r#"data-image-comparison-side="original""#,
        r#"data-image-comparison-side="prepared""#,
        r#"alt="Private original""#,
        r#"alt="Prepared post copy""#,
        "<figcaption>Private original</figcaption>",
        "<figcaption>Prepared post copy</figcaption>",
        r#"data-quality-result="${result}""#,
        r#""Quality check passed""#,
        r#""Quality check failed""#,
    ] {
        assert!(
            source.contains(required),
            "image comparison product markup must contain {required}"
        );
    }
    println!("TASK0668_PRODUCT_TITLE=Image comparison");
    println!("TASK0668_PRODUCT_FIGURE=Private original");
    println!("TASK0668_PRODUCT_FIGURE=Prepared post copy");
    println!("TASK0668_PRODUCT_QUALITY_STATES=passed,failed");
}

fn generate_private_original(path: &Path) {
    let status = Command::new("convert")
        .args([
            "-size",
            "384x288",
            "gradient:#3b82f6-#f59e0b",
            "-seed",
            "7",
            "-attenuate",
            "0.4",
            "+noise",
            "Gaussian",
            "-fill",
            "#ffffff",
            "-draw",
            "circle 120,110 120,60",
            "-fill",
            "#1f2937",
            "-draw",
            "rectangle 220,170 340,250",
            "-depth",
            "8",
        ])
        .arg(format!("PNG24:{}", path.display()))
        .status()
        .expect("ImageMagick convert can be launched");
    assert!(status.success(), "private original fixture render failed");
}

#[test]
fn comparison_screenshot_contains_both_pictures_and_a_passed_quality_result() {
    let root = repo_root();
    assert_product_comparison_markup(&root);

    let work_dir = std::env::temp_dir().join(format!("task0668-{}", std::process::id()));
    let _ = fs::remove_dir_all(&work_dir);
    fs::create_dir_all(&work_dir).expect("scratch directory can be created");
    let original = work_dir.join("private-original.png");
    let prepared = work_dir.join("prepared-post-copy.png");

    generate_private_original(&original);

    // Real prepared post copy from the TASK 0660 image-hidden encoder.
    encode_png_hidden_pointer_copy(&original, &prepared, SENT_POINTER, SENT_CHECK_MARK)
        .expect("prepared post copy encodes");

    // Real quality check (the 0664/0665 read-back): the hidden pointer must
    // decode from the prepared copy and match the sent pointer before the
    // screen is allowed to say "passed".
    let recovered = decode_png_hidden_pointer(&prepared)
        .expect("prepared post copy is decodable")
        .expect("prepared post copy carries a hidden pointer");
    assert_eq!(recovered.pointer, SENT_POINTER, "quality check pointer");
    assert_eq!(
        recovered.check_mark, SENT_CHECK_MARK,
        "quality check check-mark"
    );
    println!("TASK0668_SENT_POINTER_HEX={}", hex(&SENT_POINTER));
    println!("TASK0668_QUALITY_BEFORE_SCREEN=passed");

    let out_dir = root.join("evidence/task-0668-image-comparison-screen");
    let output = Command::new("bash")
        .arg(root.join("scripts/qa/osl-image-comparison-screen-capture.sh"))
        .env("OSL_0668_ORIGINAL", &original)
        .env("OSL_0668_PREPARED", &prepared)
        .env("OSL_0668_QUALITY_TEXT", "Quality check passed")
        .env("OSL_0668_QUALITY_STATE", "passed")
        .env("OSL_0668_OUT", &out_dir)
        .env("OSL_0668_SCREEN_SIZE", "1024x768x24")
        .output()
        .expect("comparison screen capture script can be launched");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "comparison screen capture must pass; stdout={stdout}; stderr={stderr}"
    );
    print!("{stdout}");
    eprint!("{stderr}");

    for marker in [
        "TASK0668_SCREEN_TREE_TITLE=Image comparison",
        "TASK0668_SCREEN_TREE_FIGURE=Private original",
        "TASK0668_SCREEN_TREE_FIGURE=Prepared post copy",
        "TASK0668_SCREEN_TREE_QUALITY=passed",
        "TASK0668_QUALITY_TEXT=Quality check passed",
        "TASK0668_FULL_MATCH_AE=0",
        "TASK0668_LEFT_MATCH_AE=0",
        "TASK0668_RIGHT_MATCH_AE=0",
        "TASK0668_BANNER_MATCH_AE=0",
    ] {
        assert!(
            stdout.contains(marker),
            "capture must print {marker}: {stdout}"
        );
    }
    assert!(
        !stdout.contains("TASK0668_SIDES_DIFFER_AE=0"),
        "the two pictures on screen must differ: {stdout}"
    );

    // The screenshot itself must contain the prepared post copy: decode the
    // hidden pointer straight out of the captured screen's right-hand region.
    let screenshot_prepared =
        decode_png_hidden_pointer(out_dir.join("screenshot-prepared-post-copy-crop.png"))
            .expect("screenshot prepared-copy region is decodable")
            .expect("screenshot prepared-copy region carries the hidden pointer");
    assert_eq!(
        screenshot_prepared.pointer, SENT_POINTER,
        "pointer decoded from the screenshot matches the sent pointer"
    );
    assert_eq!(
        screenshot_prepared.check_mark, SENT_CHECK_MARK,
        "check mark decoded from the screenshot matches"
    );
    println!(
        "TASK0668_SCREENSHOT_DECODED_POINTER_HEX={}",
        hex(&screenshot_prepared.pointer)
    );
    println!("TASK0668_SCREENSHOT_QUALITY=passed");

    // And the private-original region must NOT carry a pointer: the screen
    // really shows two different pictures, only one of them prepared.
    let screenshot_original =
        decode_png_hidden_pointer(out_dir.join("screenshot-private-original-crop.png"))
            .expect("screenshot original region is readable");
    assert!(
        screenshot_original.is_none(),
        "the private original on screen must not carry a hidden pointer"
    );
    println!("TASK0668_SCREENSHOT_ORIGINAL_HAS_NO_POINTER=true");

    let _ = fs::remove_dir_all(&work_dir);
}
