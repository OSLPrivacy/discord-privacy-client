use std::path::PathBuf;

fn main() {
    let mut inventory = None;
    let mut traffic = None;
    let mut catalogue = None;
    let mut args = std::env::args_os().skip(1);
    while let Some(flag) = args.next() {
        let value = args.next().unwrap_or_else(|| {
            eprintln!("TASK5213b missing value");
            std::process::exit(2);
        });
        match flag.to_string_lossy().as_ref() {
            "--inventory" => inventory = Some(PathBuf::from(value)),
            "--traffic" => traffic = Some(PathBuf::from(value)),
            "--catalogue" => catalogue = Some(PathBuf::from(value)),
            other => {
                eprintln!("TASK5213b unknown argument {other}");
                std::process::exit(2);
            }
        }
    }
    match task_5213_service_results::check(
        inventory.as_deref(),
        traffic.as_deref(),
        catalogue.as_deref(),
    ) {
        Ok(report) => println!("{report}"),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}
