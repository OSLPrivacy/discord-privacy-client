use std::path::PathBuf;

fn main() {
    let Some(path) = std::env::args_os().nth(1).map(PathBuf::from) else {
        eprintln!("usage: carrier-fidelity <manifest.json>");
        std::process::exit(1);
    };
    match task_5102_carrier_reference::fidelity::compare_manifest(&path) {
        Ok(report) => print!("{report}"),
        Err(error) => {
            eprintln!("carrier fidelity refused: {error}");
            std::process::exit(1);
        }
    }
}
