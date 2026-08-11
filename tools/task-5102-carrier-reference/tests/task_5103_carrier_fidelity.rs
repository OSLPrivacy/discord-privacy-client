use png::{BitDepth, ColorType, Encoder};
use serde_json::{json, Value};
use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use task_5102_carrier_reference::fidelity::compare_manifest;
use tempfile::TempDir;

const WIDTH: u32 = 72;
const HEIGHT: u32 = 56;

#[test]
fn every_state_channel_and_regional_metric_passes_without_averaging() {
    let temp = TempDir::new().unwrap();
    let manifest_path = write_six_case_fixture(temp.path());
    let report = compare_manifest(&manifest_path).expect("fixture should pass");
    assert_eq!(report.matches("SCORE channel=").count(), 6, "{report}");
    assert!(report.contains("boundary=0px baseline=0px flat_median=0.0000 flat_p99=0.0000 structure=0.0000% seam_structure=0.0000% tile16_max=0.0000% perceptual_raw=0.0000%"), "{report}");
    assert!(
        report.contains("PASS cases=6 channels=3 states=2 averaging=none"),
        "{report}"
    );
    println!("{report}");
}

#[test]
fn preflight_exits_one_before_scoring_for_dimensions_masks_and_starved_inputs() {
    let temp = TempDir::new().unwrap();
    let reference = rich_rgb(false, false);
    let candidate = reference.clone();
    write_rgb(
        &temp.path().join("reference.png"),
        WIDTH,
        HEIGHT,
        &reference,
    );
    write_rgb(
        &temp.path().join("candidate.png"),
        WIDTH,
        HEIGHT,
        &candidate,
    );

    let mut unequal = one_case_manifest("reference.png", "candidate-wide.png", false);
    let wide = rich_rgb_dimensions(WIDTH + 1, HEIGHT);
    write_rgb(
        &temp.path().join("candidate-wide.png"),
        WIDTH + 1,
        HEIGHT,
        &wide,
    );
    let message = run_refusal(temp.path(), "unequal.json", &mut unequal);
    assert!(message.contains("unequal physical dimensions"), "{message}");
    assert!(
        !message.contains("SCORE "),
        "preflight scored unequal inputs: {message}"
    );

    let mut free_form = one_case_manifest("reference.png", "candidate.png", false);
    free_form["comparisons"][0]["owned_rois"][0]["mask"] =
        json!({"left": 0, "top": 0, "right": 72, "bottom": 56});
    let message = run_refusal(temp.path(), "free-mask.json", &mut free_form);
    assert!(message.contains("unknown field `mask`"), "{message}");
    assert!(!message.contains("SCORE "));

    let mut averaged = one_case_manifest("reference.png", "candidate.png", false);
    averaged["average_across_states"] = json!(true);
    let message = run_refusal(temp.path(), "averaged.json", &mut averaged);
    assert!(
        message.contains("unknown field `average_across_states`"),
        "{message}"
    );
    assert!(!message.contains("SCORE "));

    let mut missing_case = one_case_manifest("reference.png", "candidate.png", false);
    missing_case["required_states"] = json!(["exact", "starved-state"]);
    let message = run_refusal(temp.path(), "missing-case.json", &mut missing_case);
    assert!(
        message.contains("state/channel inventory mismatch"),
        "{message}"
    );
    assert!(!message.contains("SCORE "));

    for role in ["reference", "candidate"] {
        for kind in ["one-colour", "one-bit", "all-transparent"] {
            let bad_name = format!("{role}-{kind}.png");
            let bad_path = temp.path().join(&bad_name);
            match kind {
                "one-colour" => write_rgb(
                    &bad_path,
                    WIDTH,
                    HEIGHT,
                    &vec![49; WIDTH as usize * HEIGHT as usize * 3],
                ),
                "one-bit" => write_one_bit(&bad_path),
                "all-transparent" => write_transparent(&bad_path),
                _ => unreachable!(),
            }
            let reference_name = if role == "reference" {
                &bad_name
            } else {
                "reference.png"
            };
            let candidate_name = if role == "candidate" {
                &bad_name
            } else {
                "candidate.png"
            };
            let mut manifest = one_case_manifest(reference_name, candidate_name, false);
            let message = run_refusal(temp.path(), &format!("{role}-{kind}.json"), &mut manifest);
            assert!(
                !message.contains("SCORE "),
                "starved input was scored: {message}"
            );
            match kind {
                "one-colour" => {
                    assert!(message.contains("degenerate with 1 distinct"), "{message}")
                }
                "one-bit" => assert!(
                    message.contains("1-bit and indexed inputs are forbidden"),
                    "{message}"
                ),
                "all-transparent" => assert!(message.contains("all-transparent"), "{message}"),
                _ => unreachable!(),
            }
        }
    }

    let tri = tri_colour_capture();
    write_rgb(&temp.path().join("tri-reference.png"), WIDTH, HEIGHT, &tri);
    write_rgb(&temp.path().join("tri-candidate.png"), WIDTH, HEIGHT, &tri);
    let mut fixed_floor = one_case_manifest("tri-reference.png", "tri-candidate.png", false);
    fixed_floor["comparisons"][0]["known_good_distinct_min"] = json!(5);
    let message = run_refusal(temp.path(), "fixed-floor.json", &mut fixed_floor);
    assert!(
        message.contains("outside reviewed range 5..=16"),
        "{message}"
    );
    assert!(!message.contains("SCORE "));
    println!("preflight refusals: unequal=1 free_form_mask=1 averaging=1 missing_case=1 starved_inputs=6 fixed_floor=1 all_exit=1 score_lines=0");
}

#[test]
fn protected_text_uses_only_uia_ink_expanded_two_pixels() {
    let temp = TempDir::new().unwrap();
    write_rgb(
        &temp.path().join("reference.png"),
        WIDTH,
        HEIGHT,
        &rich_rgb(false, true),
    );
    write_rgb(
        &temp.path().join("candidate.png"),
        WIDTH,
        HEIGHT,
        &rich_rgb(true, true),
    );
    let manifest = one_case_manifest("reference.png", "candidate.png", true);
    let path = write_manifest(temp.path(), "protected.json", &manifest);
    let report = compare_manifest(&path).expect("UIA-derived glyph mask should pass");
    let mask_pixels = score_value(&report, "mask");
    assert!(
        mask_pixels > 0.0 && mask_pixels < (64 * 48) as f64,
        "{report}"
    );

    let mut exact_with_mask = manifest;
    exact_with_mask["comparisons"][0]["text_relation"] = json!("exact_benign_probe");
    let path = write_manifest(temp.path(), "exact-with-range.json", &exact_with_mask);
    let error = compare_manifest(&path).unwrap_err();
    assert!(error.contains("may not mask any pixel"), "{error}");

    let mut wrong_expansion = one_case_manifest("reference.png", "candidate.png", true);
    wrong_expansion["glyph_expansion_physical_px"] = json!(3);
    let path = write_manifest(temp.path(), "wrong-expansion.json", &wrong_expansion);
    let error = compare_manifest(&path).unwrap_err();
    assert!(error.contains("exactly 2 physical pixels"), "{error}");
    println!("protected mask_pixels={mask_pixels:.0} exact_probe_mask=refused glyph_expansion=2px");
}

#[test]
fn every_numeric_gate_turns_red_independently() {
    let temp = TempDir::new().unwrap();
    let reference = rich_rgb(false, false);
    write_rgb(
        &temp.path().join("reference.png"),
        WIDTH,
        HEIGHT,
        &reference,
    );

    let cases: Vec<(&str, Value, Vec<u8>, &str)> = vec![
        (
            "boundary",
            {
                let mut manifest =
                    one_case_manifest("reference.png", "candidate-boundary.png", false);
                manifest["comparisons"][0]["owned_rois"][0]["candidate_boundary"]["left"] =
                    json!(121);
                manifest["comparisons"][0]["owned_rois"][0]["candidate_boundary"]["right"] =
                    json!(185);
                manifest
            },
            reference.clone(),
            "boundary displacement 1px",
        ),
        (
            "baseline",
            {
                let mut manifest =
                    one_case_manifest("reference.png", "candidate-baseline.png", false);
                manifest["comparisons"][0]["owned_rois"][0]["candidate_baseline_y"] = json!(251);
                manifest
            },
            reference.clone(),
            "baseline displacement 1px",
        ),
        (
            "flat-fill",
            one_case_manifest("reference.png", "candidate-flat-fill.png", false),
            mutate_rect(reference.clone(), 8, 8, 24, 12, [56, 58, 64]),
            "flat-fill median delta-E00",
        ),
        (
            "structure",
            one_case_manifest("reference.png", "candidate-structure.png", false),
            mutate_rect(reference.clone(), 24, 14, 48, 42, [245, 50, 60]),
            "ROI structure mismatch",
        ),
        (
            "tile",
            one_case_manifest("reference.png", "candidate-tile.png", false),
            mutate_rect(reference.clone(), 6, 6, 22, 22, [245, 245, 245]),
            "16x16 tile mismatch",
        ),
        (
            "raw",
            one_case_manifest("reference.png", "candidate-raw.png", false),
            sparse_raw_mutation(reference.clone()),
            "exact-probe perceptual raw mismatch",
        ),
        (
            "line-wrap",
            one_case_manifest("reference.png", "candidate-line-wrap.png", false),
            line_wrap_mutation(reference.clone()),
            "exact-probe perceptual raw mismatch",
        ),
    ];
    for (name, manifest, candidate, expected) in cases {
        let candidate_name = format!("candidate-{name}.png");
        write_rgb(&temp.path().join(candidate_name), WIDTH, HEIGHT, &candidate);
        let path = write_manifest(temp.path(), &format!("{name}.json"), &manifest);
        let error = compare_manifest(&path).expect_err(name);
        assert!(
            error.contains(expected),
            "{name}: expected {expected}: {error}"
        );
        println!("numeric_mutant={name} exit=1 named={expected}");
    }
}

fn write_six_case_fixture(dir: &Path) -> PathBuf {
    let channels = ["stable", "ptb", "canary"];
    let states = ["composer-exact-probe", "eye-protected-text"];
    let mut comparisons = Vec::new();
    for channel in channels {
        for state in states {
            let protected = state == "eye-protected-text";
            let reference = format!("{channel}-{state}-reference.png");
            let candidate = format!("{channel}-{state}-candidate.png");
            write_rgb(
                &dir.join(&reference),
                WIDTH,
                HEIGHT,
                &rich_rgb(false, protected),
            );
            write_rgb(
                &dir.join(&candidate),
                WIDTH,
                HEIGHT,
                &rich_rgb(protected, protected),
            );
            comparisons.push(comparison(
                channel, state, &reference, &candidate, protected,
            ));
        }
    }
    write_manifest(
        dir,
        "manifest.json",
        &json!({
            "schema": "osl-carrier-fidelity-v1",
            "seam_ring_physical_px": 4,
            "glyph_expansion_physical_px": 2,
            "required_channels": channels,
            "required_states": states,
            "comparisons": comparisons
        }),
    )
}

fn one_case_manifest(reference: &str, candidate: &str, protected: bool) -> Value {
    json!({
        "schema": "osl-carrier-fidelity-v1",
        "seam_ring_physical_px": 4,
        "glyph_expansion_physical_px": 2,
        "required_channels": ["stable"],
        "required_states": ["exact"],
        "comparisons": [comparison("stable", "exact", reference, candidate, protected)]
    })
}

fn comparison(
    channel: &str,
    state: &str,
    reference: &str,
    candidate: &str,
    protected: bool,
) -> Value {
    json!({
        "channel": channel,
        "state": state,
        "reference_png": reference,
        "candidate_png": candidate,
        "physical_width": WIDTH,
        "physical_height": HEIGHT,
        "text_relation": if protected { "protected_text_differs" } else { "exact_benign_probe" },
        "known_good_capture_count": 5,
        "known_good_distinct_min": 3,
        "known_good_distinct_max": 16,
        "owned_rois": [{
            "name": "composer",
            "roi": {"left": 4, "top": 4, "right": 68, "bottom": 52},
            "reference_boundary": {"left": 120, "top": 220, "right": 184, "bottom": 268},
            "candidate_boundary": {"left": 120, "top": 220, "right": 184, "bottom": 268},
            "reference_baseline_y": 250,
            "candidate_baseline_y": 250,
            "flat_fill_rects": [{"left": 8, "top": 8, "right": 24, "bottom": 12}],
            "uia_text_ranges": if protected {
                json!([{"left": 28, "top": 22, "right": 60, "bottom": 38}])
            } else {
                json!([])
            }
        }]
    })
}

fn rich_rgb(alternate_text: bool, protected: bool) -> Vec<u8> {
    let mut pixels = rich_rgb_dimensions(WIDTH, HEIGHT);
    if protected {
        let starts: &[u32] = if alternate_text {
            &[31, 38, 49, 55]
        } else {
            &[29, 35, 44, 52]
        };
        for x in starts {
            for y in 25..34 {
                set(&mut pixels, WIDTH, *x, y, [219, 222, 225]);
                if (y + x) % 3 == 0 {
                    set(&mut pixels, WIDTH, x + 1, y, [185, 188, 193]);
                }
            }
        }
    } else {
        for x in 28..48 {
            if x % 3 != 0 {
                for y in 26..34 {
                    if (x + y) % 4 != 0 {
                        set(&mut pixels, WIDTH, x, y, [219, 222, 225]);
                    }
                }
            }
        }
    }
    pixels
}

fn rich_rgb_dimensions(width: u32, height: u32) -> Vec<u8> {
    let mut pixels = vec![0u8; width as usize * height as usize * 3];
    for y in 0..height {
        for x in 0..width {
            let rgb = if x < 4 || y < 4 || x + 4 >= width || y + 4 >= height {
                [43, 45, 49]
            } else if x == 4 || y == 4 || x + 5 == width || y + 5 == height {
                [30, 31, 34]
            } else if (52..64).contains(&x) && (42..48).contains(&y) {
                [88, 101, 242]
            } else {
                [49, 51, 56]
            };
            set(&mut pixels, width, x, y, rgb);
        }
    }
    pixels
}

fn mutate_rect(
    mut pixels: Vec<u8>,
    left: u32,
    top: u32,
    right: u32,
    bottom: u32,
    rgb: [u8; 3],
) -> Vec<u8> {
    for y in top..bottom {
        for x in left..right {
            set(&mut pixels, WIDTH, x, y, rgb);
        }
    }
    pixels
}

fn sparse_raw_mutation(mut pixels: Vec<u8>) -> Vec<u8> {
    for y in 12..20 {
        for x in 28..36 {
            if (x + y) % 2 == 0 {
                set(&mut pixels, WIDTH, x, y, [255, 0, 0]);
            }
        }
    }
    pixels
}

fn line_wrap_mutation(mut pixels: Vec<u8>) -> Vec<u8> {
    for y in 26..34 {
        for x in 28..48 {
            set(&mut pixels, WIDTH, x, y, [49, 51, 56]);
        }
    }
    for y in 18..24 {
        for x in 28..42 {
            if (x + y) % 3 != 0 {
                set(&mut pixels, WIDTH, x, y, [219, 222, 225]);
            }
        }
    }
    for y in 35..41 {
        for x in 28..42 {
            if (x + y) % 3 != 0 {
                set(&mut pixels, WIDTH, x, y, [219, 222, 225]);
            }
        }
    }
    pixels
}

fn tri_colour_capture() -> Vec<u8> {
    let mut pixels = vec![0u8; WIDTH as usize * HEIGHT as usize * 3];
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let rgb = if x < 4 || y < 4 || x >= 68 || y >= 52 {
                [43, 45, 49]
            } else if x == 4 || y == 4 || x == 67 || y == 51 {
                [30, 31, 34]
            } else if (52..64).contains(&x) && (42..48).contains(&y) {
                [88, 101, 242]
            } else {
                [49, 51, 56]
            };
            set(&mut pixels, WIDTH, x, y, rgb);
        }
    }
    pixels
}

fn set(pixels: &mut [u8], width: u32, x: u32, y: u32, rgb: [u8; 3]) {
    let index = ((y * width + x) * 3) as usize;
    pixels[index..index + 3].copy_from_slice(&rgb);
}

fn write_rgb(path: &Path, width: u32, height: u32, pixels: &[u8]) {
    write_png(path, width, height, ColorType::Rgb, BitDepth::Eight, pixels);
}

fn write_one_bit(path: &Path) {
    let stride = WIDTH.div_ceil(8) as usize;
    let mut pixels = vec![0u8; stride * HEIGHT as usize];
    for (index, byte) in pixels.iter_mut().enumerate() {
        *byte = if index % 2 == 0 { 0xaa } else { 0x55 };
    }
    write_png(
        path,
        WIDTH,
        HEIGHT,
        ColorType::Grayscale,
        BitDepth::One,
        &pixels,
    );
}

fn write_transparent(path: &Path) {
    let mut pixels = vec![0u8; WIDTH as usize * HEIGHT as usize * 4];
    for chunk in pixels.chunks_exact_mut(4) {
        chunk[..3].copy_from_slice(&[49, 51, 56]);
    }
    write_png(
        path,
        WIDTH,
        HEIGHT,
        ColorType::Rgba,
        BitDepth::Eight,
        &pixels,
    );
}

fn write_png(
    path: &Path,
    width: u32,
    height: u32,
    color: ColorType,
    depth: BitDepth,
    pixels: &[u8],
) {
    let writer = BufWriter::new(File::create(path).unwrap());
    let mut encoder = Encoder::new(writer, width, height);
    encoder.set_color(color);
    encoder.set_depth(depth);
    let mut png = encoder.write_header().unwrap();
    png.write_image_data(pixels).unwrap();
}

fn write_manifest(dir: &Path, name: &str, manifest: &Value) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, serde_json::to_vec_pretty(manifest).unwrap()).unwrap();
    path
}

fn run_refusal(dir: &Path, name: &str, manifest: &mut Value) -> String {
    let path = write_manifest(dir, name, manifest);
    let Output {
        status,
        stdout,
        stderr,
    } = Command::new(env!("CARGO_BIN_EXE_carrier-fidelity"))
        .arg(path)
        .output()
        .unwrap();
    assert_eq!(
        status.code(),
        Some(1),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&stdout),
        String::from_utf8_lossy(&stderr)
    );
    format!(
        "{}{}",
        String::from_utf8_lossy(&stdout),
        String::from_utf8_lossy(&stderr)
    )
}

fn score_value(report: &str, key: &str) -> f64 {
    report
        .split_whitespace()
        .find_map(|field| field.strip_prefix(&format!("{key}=")))
        .map(|value| {
            value
                .trim_end_matches("px")
                .trim_end_matches('%')
                .parse()
                .unwrap()
        })
        .unwrap()
}
