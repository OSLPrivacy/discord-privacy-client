//! TASK 6818 scenario runner.
//!
//! Two stages over the same shipped fixture:
//!
//! * `model` — build the world, run the sequence, and emit the leaver's live
//!   sidebar model at each point the interface has to render a menu.
//! * `depart` — build a fresh world and run the same sequence, acting only on
//!   the activations parsed out of the rendered menu.
//!
//! Both write a full report. The check compares the models the interface was
//! rendered from against the models the engine actually held at each attempt,
//! so a menu that says one thing while the engine does another is caught.

use std::path::PathBuf;
use std::process::ExitCode;

use place_departure::run::{run, MenuBundle};

fn main() -> ExitCode {
    match real_main() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("TASK6818 runner failed: {error}");
            ExitCode::FAILURE
        }
    }
}

fn real_main() -> Result<(), String> {
    let mut stage = String::from("model");
    let mut out: Option<PathBuf> = None;
    let mut state: Option<PathBuf> = None;
    let mut menu: Option<PathBuf> = None;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--stage" => stage = args.next().ok_or("--stage needs a value")?,
            "--out" => out = Some(PathBuf::from(args.next().ok_or("--out needs a path")?)),
            "--state" => state = Some(PathBuf::from(args.next().ok_or("--state needs a path")?)),
            "--menu" => menu = Some(PathBuf::from(args.next().ok_or("--menu needs a path")?)),
            other => return Err(format!("unknown argument {other}")),
        }
    }

    let out = out.ok_or("--out is required")?;
    let state = state.ok_or("--state is required")?;
    let fixture = place_departure::shipped_fixture().map_err(|error| error.to_string())?;

    let bundle = match &menu {
        Some(path) => {
            let text = std::fs::read_to_string(path)
                .map_err(|error| format!("reading {}: {error}", path.display()))?;
            let bundle: MenuBundle = serde_json::from_str(&text)
                .map_err(|error| format!("parsing {}: {error}", path.display()))?;
            Some(bundle)
        }
        None => None,
    };

    if state.exists() {
        std::fs::remove_dir_all(&state).map_err(|error| error.to_string())?;
    }

    let report = run(fixture, &state, bundle.as_ref(), &stage).map_err(|error| error.to_string())?;
    let json = serde_json::to_string_pretty(&report).map_err(|error| error.to_string())?;
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    std::fs::write(&out, json).map_err(|error| error.to_string())?;
    println!(
        "TASK6818 stage={stage} departures={} missing_activations={} wrote {}",
        report.departures.len(),
        report.missing_activations.len(),
        out.display()
    );
    Ok(())
}
