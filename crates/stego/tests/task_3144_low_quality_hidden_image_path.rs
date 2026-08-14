//! TASK 3144: the selected low-quality refusal gates the real image carrier.
//!
//! The quality API receives decoded RGB samples, so the good JPEG's caller-visible
//! name survives normalization to the PNG-only hidden-pointer carrier.

use std::io::Cursor;

use stego::{
    check_decoded_rgb_hidden_image_quality, decode_png_hidden_pointer_bytes,
    encode_png_hidden_pointer_bytes, ImageHiddenQualityDecision, ImageHiddenQualityResult,
    IMAGE_HIDDEN_LOW_QUALITY_REFUSAL, IMAGE_HIDDEN_POINTER_BYTES,
};

const HIDDEN_TEXT: &str = "IMAGE-QUALITY-3144";
const GOOD_FILE: &str = "IMAGE-GOOD-1200.jpg";
const FLAT_FILE: &str = "IMAGE-FLAT-64.png";
const CHECK_MARK: [u8; 4] = [0x31, 0x44, 0xca, 0xfe];

enum HideAttempt {
    Accepted {
        quality: ImageHiddenQualityResult,
        read_back: String,
    },
    Refused(ImageHiddenQualityResult),
}

#[test]
fn task_3144_good_photo_reads_back_exact_text_and_flat_image_is_refused() {
    let good = try_hide(
        GOOD_FILE,
        1_200,
        800,
        &detailed_rgb(1_200, 800),
        HIDDEN_TEXT,
    );
    let flat = try_hide(FLAT_FILE, 64, 64, &flat_rgb(64, 64), HIDDEN_TEXT);

    let (good_quality, read_back) = match good {
        HideAttempt::Accepted { quality, read_back } => (quality, read_back),
        HideAttempt::Refused(quality) => panic!("unexpected refusal: {}", quality.message()),
    };
    let flat_quality = match flat {
        HideAttempt::Refused(quality) => quality,
        HideAttempt::Accepted { quality, .. } => {
            panic!("unexpected acceptance: {}", quality.message())
        }
    };

    let good_reason = good_quality.message();
    let flat_reason = flat_quality.message();
    println!(
        "TASK3144_GOOD file={} decision=accepted read_back={} reason={}",
        good_quality.file_name, read_back, good_reason
    );
    println!(
        "TASK3144_FLAT file={} decision=refused attempted_text={} reason={}",
        flat_quality.file_name, HIDDEN_TEXT, flat_reason
    );

    assert_eq!(good_quality.file_name, GOOD_FILE);
    assert_eq!(good_quality.marked_bytes, HIDDEN_TEXT.len());
    assert_eq!(good_quality.decision, ImageHiddenQualityDecision::Accepted);
    assert_eq!(read_back, HIDDEN_TEXT);
    assert_eq!(
        good_reason,
        format!(
            "{GOOD_FILE}: accepted {} marked bytes; measured capacity {} bytes.",
            HIDDEN_TEXT.len(),
            good_quality.measured_capacity_bytes
        )
    );

    assert_eq!(flat_quality.file_name, FLAT_FILE);
    assert_eq!(flat_quality.marked_bytes, HIDDEN_TEXT.len());
    assert_eq!(flat_quality.measured_capacity_bytes, 0);
    assert_eq!(flat_quality.decision, ImageHiddenQualityDecision::Refused);
    assert_eq!(
        flat_reason,
        format!(
            "{FLAT_FILE}: refused {} marked bytes; measured capacity 0 bytes. {IMAGE_HIDDEN_LOW_QUALITY_REFUSAL}",
            HIDDEN_TEXT.len()
        )
    );
}

fn try_hide(file_name: &str, width: u32, height: u32, rgb: &[u8], text: &str) -> HideAttempt {
    let quality = check_decoded_rgb_hidden_image_quality(file_name, width, height, rgb, text.len())
        .expect("decoded image has a quality decision");
    if !quality.is_accepted() {
        return HideAttempt::Refused(quality);
    }

    assert!(text.len() <= IMAGE_HIDDEN_POINTER_BYTES);
    let mut pointer = [0u8; IMAGE_HIDDEN_POINTER_BYTES];
    pointer[..text.len()].copy_from_slice(text.as_bytes());

    // The shipping carrier consumes PNG bytes after the source decoder has
    // normalized the caller's JPEG/PNG pixels to RGB.
    let normalized_png = write_png_rgb(width, height, rgb);
    let hidden_png = encode_png_hidden_pointer_bytes(&normalized_png, pointer, CHECK_MARK)
        .expect("accepted image carries the hidden text");
    let decoded = decode_png_hidden_pointer_bytes(&hidden_png)
        .expect("accepted image reads")
        .expect("accepted image contains the hidden frame");
    assert_eq!(decoded.check_mark, CHECK_MARK);
    let read_back = String::from_utf8(decoded.pointer[..text.len()].to_vec())
        .expect("hidden test text remains UTF-8");

    HideAttempt::Accepted { quality, read_back }
}

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

fn write_png_rgb(width: u32, height: u32, pixels: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(Cursor::new(&mut bytes), width, height);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .expect("normalized PNG header writes")
            .write_image_data(pixels)
            .expect("normalized PNG pixels write");
    }
    bytes
}
