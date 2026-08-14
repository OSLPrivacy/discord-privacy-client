//! TASK 3140: measure detailed image capacity and enforce Liam's task-3134
//! refusal for a small, plain source image.

use stego::{
    check_decoded_rgb_hidden_image_quality, ImageHiddenQualityDecision,
    IMAGE_HIDDEN_LOW_QUALITY_REFUSAL,
};

const MARKED_BYTES: usize = 1_024;
const GOOD_FILE: &str = "IMAGE-GOOD-1200.jpg";
const FLAT_FILE: &str = "IMAGE-FLAT-64.png";

#[test]
fn task_3140_accepts_good_image_and_refuses_small_plain_image_for_same_bytes() {
    let good = check_decoded_rgb_hidden_image_quality(
        GOOD_FILE,
        1_200,
        800,
        &detailed_rgb(1_200, 800),
        MARKED_BYTES,
    )
    .expect("decoded good-photo pixels have a quality result");
    let flat =
        check_decoded_rgb_hidden_image_quality(FLAT_FILE, 64, 64, &flat_rgb(64, 64), MARKED_BYTES)
            .expect("decoded flat PNG pixels have a quality result");

    println!("TASK3140_GOOD_RESULT={}", good.message());
    println!("TASK3140_FLAT_RESULT={}", flat.message());
    println!(
        "TASK3140_GOOD file={} decision=accepted marked_bytes={} measured_capacity_bytes={}",
        good.file_name, good.marked_bytes, good.measured_capacity_bytes
    );
    println!(
        "TASK3140_FLAT file={} decision=refused marked_bytes={} measured_capacity_bytes={}",
        flat.file_name, flat.marked_bytes, flat.measured_capacity_bytes
    );

    assert_eq!(good.file_name, GOOD_FILE);
    assert_eq!(good.marked_bytes, MARKED_BYTES);
    assert_eq!(good.decision, ImageHiddenQualityDecision::Accepted);
    assert!(good.measured_capacity_bytes >= MARKED_BYTES);
    assert!(good.message().contains(GOOD_FILE));
    assert!(good.message().contains(&format!(
        "measured capacity {} bytes",
        good.measured_capacity_bytes
    )));

    assert_eq!(flat.file_name, FLAT_FILE);
    assert_eq!(flat.marked_bytes, MARKED_BYTES);
    assert_eq!(flat.measured_capacity_bytes, 0);
    assert_eq!(flat.decision, ImageHiddenQualityDecision::Refused);
    assert!(flat.message().contains(FLAT_FILE));
    assert!(flat.message().contains("measured capacity 0 bytes"));
    assert!(flat.message().contains(IMAGE_HIDDEN_LOW_QUALITY_REFUSAL));
}

/// Deterministic decoded RGB stand-in for a detailed 1200-pixel-wide photo.
/// Adjacent samples vary in all channels, unlike the flat negative fixture.
fn detailed_rgb(width: u32, height: u32) -> Vec<u8> {
    let mut pixels = Vec::with_capacity((width * height * 3) as usize);
    for y in 0..height {
        for x in 0..width {
            pixels.push(((x * 37 + y * 17 + (x ^ y)) & 0xff) as u8);
            pixels.push(((x * 13 + y * 41 + (x * y % 251)) & 0xff) as u8);
            pixels.push(((x * 29 + y * 11 + ((x + y) * 7)) & 0xff) as u8);
        }
    }
    pixels
}

fn flat_rgb(width: u32, height: u32) -> Vec<u8> {
    [86, 132, 174]
        .into_iter()
        .cycle()
        .take((width * height * 3) as usize)
        .collect()
}
