use runtime::{
    compare_fixed_screen_captures, FixedScreenCapture, PixelMismatch, ScreenshotComparison,
};

fn fixed_capture() -> FixedScreenCapture {
    let mut rgba = Vec::new();
    for y in 0..3 {
        for x in 0..4 {
            rgba.extend_from_slice(&[16 + (x * 8) as u8, 48 + (y * 16) as u8, 96, 255]);
        }
    }
    FixedScreenCapture::from_rgba(4, 3, rgba).expect("fixed capture")
}

#[test]
fn task_0026_identical_fixed_captures_pass_and_one_pixel_mutation_fails() {
    let baseline = fixed_capture();
    let identical = baseline.clone();

    let first = compare_fixed_screen_captures(&baseline, &identical)
        .expect("identical fixed captures compare");
    let ScreenshotComparison::Identical {
        width,
        height,
        pixels_compared,
    } = first
    else {
        panic!("identical captures did not compare as identical: {first:?}");
    };
    println!(
        "TASK0026 identical_comparison status=pass width={width} height={height} pixels_compared={pixels_compared}"
    );
    assert_eq!((width, height, pixels_compared), (4, 3, 12));

    let mut altered_rgba = baseline.rgba().to_vec();
    let changed_pixel = (1 * width as usize + 2) * 4;
    altered_rgba[changed_pixel] = altered_rgba[changed_pixel].wrapping_add(1);
    let altered =
        FixedScreenCapture::from_rgba(width, height, altered_rgba).expect("altered capture");

    let second =
        compare_fixed_screen_captures(&baseline, &altered).expect("altered fixed captures compare");
    let ScreenshotComparison::Different {
        width,
        height,
        mismatched_pixels,
        first_mismatch:
            PixelMismatch {
                x,
                y,
                left_rgba,
                right_rgba,
            },
    } = second
    else {
        panic!("altered capture did not compare as different: {second:?}");
    };
    println!(
        "TASK0026 altered_comparison status=fail width={width} height={height} mismatched_pixels={mismatched_pixels} first_mismatch_pixel=({x},{y}) left_rgba={left_rgba:?} right_rgba={right_rgba:?}"
    );
    assert_eq!((width, height, mismatched_pixels), (4, 3, 1));
    assert_eq!((x, y), (2, 1));
    assert_eq!(left_rgba, [32, 64, 96, 255]);
    assert_eq!(right_rgba, [33, 64, 96, 255]);
}
