use std::env;
use std::process::ExitCode;
use task_4082_row_probe::{load_fixture, render_instagram_4083_report};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::from(1)
        }
    }
}

fn run() -> Result<(), String> {
    let mut args = env::args().skip(1);
    let mut surface = None;
    let mut fixture_path = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--surface" => surface = args.next(),
            "--fixture" => fixture_path = args.next(),
            other => return Err(format!("unknown argument {other}")),
        }
    }

    if surface.as_deref() != Some("instagram-4083") {
        return Err("TASK4083_FAIL=surface must be instagram-4083".to_owned());
    }
    let fixture_path = fixture_path.ok_or_else(|| "TASK4083_FAIL=missing --fixture".to_owned())?;
    let fixture = load_fixture(fixture_path)?;
    let report = render_instagram_4083_report(&fixture);
    print!("{}", report.rendered);

    if report.unchecked_places.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "TASK4083_FAIL=never_checked_count={} never_checked={}",
            report.unchecked_places.len(),
            report.unchecked_places.join(",")
        ))
    }
}
