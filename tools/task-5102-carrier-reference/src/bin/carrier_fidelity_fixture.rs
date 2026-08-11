use png::{BitDepth, ColorType, Encoder};
use serde_json::json;
use std::fs::{self, File};
use std::io::BufWriter;
use std::path::{Path, PathBuf};

const WIDTH: u32 = 72;
const HEIGHT: u32 = 56;

fn main() {
    let Some(output_dir) = std::env::args_os().nth(1).map(PathBuf::from) else {
        eprintln!("usage: carrier-fidelity-fixture <output-dir>");
        std::process::exit(1);
    };
    if let Err(error) = write_fixture(&output_dir) {
        eprintln!("fixture failed: {error}");
        std::process::exit(1);
    }
    println!(
        "fixture cases=6 channels=3 states=2 dimensions={}x{} seam=4px glyph_expansion=2px",
        WIDTH, HEIGHT
    );
}

fn write_fixture(output_dir: &Path) -> Result<(), String> {
    fs::create_dir_all(output_dir).map_err(|error| error.to_string())?;
    let channels = ["stable", "ptb", "canary"];
    let states = ["composer-exact-probe", "eye-protected-text"];
    let mut comparisons = Vec::new();
    for channel in channels {
        for state in states {
            let protected = state == "eye-protected-text";
            let reference_name = format!("{channel}-{state}-reference.png");
            let candidate_name = format!("{channel}-{state}-candidate.png");
            write_png(&output_dir.join(&reference_name), render(false, protected))?;
            write_png(
                &output_dir.join(&candidate_name),
                render(protected, protected),
            )?;
            comparisons.push(json!({
                "channel": channel,
                "state": state,
                "reference_png": reference_name,
                "candidate_png": candidate_name,
                "physical_width": WIDTH,
                "physical_height": HEIGHT,
                "text_relation": if protected { "protected_text_differs" } else { "exact_benign_probe" },
                "known_good_capture_count": 5,
                "known_good_distinct_min": 3,
                "known_good_distinct_max": 16,
                "owned_rois": [{
                    "name": if protected { "decrypted-message-content" } else { "composer" },
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
            }));
        }
    }
    let manifest = json!({
        "schema": "osl-carrier-fidelity-v1",
        "seam_ring_physical_px": 4,
        "glyph_expansion_physical_px": 2,
        "required_channels": channels,
        "required_states": states,
        "comparisons": comparisons
    });
    let encoded = serde_json::to_vec_pretty(&manifest).map_err(|error| error.to_string())?;
    fs::write(output_dir.join("manifest.json"), encoded).map_err(|error| error.to_string())
}

fn render(alternate_text: bool, protected: bool) -> Vec<u8> {
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
            set(&mut pixels, x, y, rgb);
        }
    }
    if protected {
        let starts: &[u32] = if alternate_text {
            &[31, 38, 49, 55]
        } else {
            &[29, 35, 44, 52]
        };
        for start in starts {
            for y in 25..34 {
                set(&mut pixels, *start, y, [219, 222, 225]);
                if (y + start) % 3 == 0 {
                    set(&mut pixels, start + 1, y, [185, 188, 193]);
                }
            }
        }
    } else {
        for x in 28..48 {
            if x % 3 != 0 {
                for y in 26..34 {
                    if (x + y) % 4 != 0 {
                        set(&mut pixels, x, y, [219, 222, 225]);
                    }
                }
            }
        }
    }
    pixels
}

fn set(pixels: &mut [u8], x: u32, y: u32, rgb: [u8; 3]) {
    let index = ((y * WIDTH + x) * 3) as usize;
    pixels[index..index + 3].copy_from_slice(&rgb);
}

fn write_png(path: &Path, pixels: Vec<u8>) -> Result<(), String> {
    let writer = BufWriter::new(File::create(path).map_err(|error| error.to_string())?);
    let mut encoder = Encoder::new(writer, WIDTH, HEIGHT);
    encoder.set_color(ColorType::Rgb);
    encoder.set_depth(BitDepth::Eight);
    let mut png = encoder.write_header().map_err(|error| error.to_string())?;
    png.write_image_data(&pixels)
        .map_err(|error| error.to_string())
}
