use std::path::PathBuf;

fn main() {
    let mut args = std::env::args_os().skip(1).map(PathBuf::from);
    let Some(policy) = args.next() else {
        eprintln!("usage: carrier-release-policy <policy.json> [human-calibration.json]");
        std::process::exit(1);
    };
    let calibration = args.next();
    if args.next().is_some() {
        eprintln!("carrier release policy refused: unexpected extra argument");
        std::process::exit(1);
    }
    match task_5102_carrier_reference::release_policy::check_files(&policy, calibration.as_deref())
    {
        Ok(report) => print!("{report}"),
        Err(error) => {
            eprintln!("carrier release policy refused: {error}");
            std::process::exit(1);
        }
    }
}
