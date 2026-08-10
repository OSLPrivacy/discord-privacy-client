use std::path::PathBuf;
use task_5102_carrier_reference::fixture::{fixture_request, FixtureBackend, FixtureCase};
use task_5102_carrier_reference::run_capture;

fn main() {
    let mut args = std::env::args_os().skip(1);
    let Some(output_dir) = args.next().map(PathBuf::from) else {
        eprintln!("usage: carrier-reference-fixture OUTPUT_DIR CASE");
        std::process::exit(2);
    };
    let Some(case) = args
        .next()
        .and_then(|value| value.into_string().ok())
        .as_deref()
        .and_then(FixtureCase::parse)
    else {
        eprintln!("unknown fixture case");
        std::process::exit(2);
    };
    if args.next().is_some() {
        eprintln!("too many arguments");
        std::process::exit(2);
    }

    let request = fixture_request(output_dir, case);
    let mut backend = FixtureBackend::new(case);
    match run_capture(&mut backend, &request) {
        Ok(result) => {
            let requested = backend.capture_bounds().first().copied().unwrap();
            println!(
                "pngs=1 manifests=1 width={} height={} mode=RGB seam_px=4 capture_calls={} zeroized_sources={} requested_source={}x{} sha256={}",
                result.capture_bounds.width().unwrap_or(0),
                result.capture_bounds.height().unwrap_or(0),
                backend.captures(),
                backend.zeroized_sources(),
                requested.width().unwrap_or(0),
                requested.height().unwrap_or(0),
                result.png_sha256
            );
        }
        Err(error) => {
            eprintln!("capture refused: {error}; pngs=0");
            std::process::exit(1);
        }
    }
}
