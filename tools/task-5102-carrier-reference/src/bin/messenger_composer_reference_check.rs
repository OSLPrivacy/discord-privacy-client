use std::path::Path;
use std::process::ExitCode;
use task_5102_carrier_reference::messenger::{
    verify_composer_references, ValidationMode, CAPTURES_PER_KEY,
};

fn main() -> ExitCode {
    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    if arguments.len() != 3 {
        eprintln!(
            "usage: messenger-composer-reference-check <dated-live-census.json> <shipped-contract.json> <reviewed-manifest-directory>"
        );
        return ExitCode::from(1);
    }
    match verify_composer_references(
        Path::new(&arguments[0]),
        Path::new(&arguments[1]),
        Path::new(&arguments[2]),
        ValidationMode::Release,
    ) {
        Ok(verified) => {
            println!("pre_diff_preflight=passed");
            println!("origin=https://www.messenger.com");
            println!("composer_state=ordinary-unprotected-probe");
            println!("composer_keys={}", verified.keys.len());
            println!("captures_per_key={CAPTURES_PER_KEY}");
            println!("captures={}", verified.captures);
            println!(
                "minimum_capture_distinct_rgb_colours={}",
                verified.minimum_capture_colours
            );
            println!(
                "minimum_roi_distinct_rgb_colours={}",
                verified.minimum_roi_colours
            );
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("TASK 5130 BLOCKED: {error}");
            ExitCode::from(1)
        }
    }
}
