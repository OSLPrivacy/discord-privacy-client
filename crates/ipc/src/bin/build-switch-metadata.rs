use ipc::build_switch_metadata::{
    cmd_osl_build_switch_test_metadata, format_build_switch_metadata, validate_runtime_switch_list,
};
use ipc::AppState;
use std::process::ExitCode;

fn parse_switch_list(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .collect()
}

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let mut required_switch_list = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--require-switch-list" => {
                let Some(value) = args.next() else {
                    eprintln!("--require-switch-list requires a comma-separated switch list");
                    return ExitCode::from(2);
                };
                required_switch_list = Some(parse_switch_list(&value));
            }
            "--help" | "-h" => {
                println!(
                    "usage: build-switch-metadata [--require-switch-list sender_keys_enabled,rn_wire_in_enabled]"
                );
                return ExitCode::SUCCESS;
            }
            other => {
                eprintln!("unknown argument: {other}");
                return ExitCode::from(2);
            }
        }
    }

    let state = AppState::new();
    let metadata = cmd_osl_build_switch_test_metadata(&state);
    println!("{}", format_build_switch_metadata(&metadata));

    if let Some(switches) = required_switch_list {
        if let Err(error) = validate_runtime_switch_list(switches) {
            eprintln!("build switch metadata incomplete: {error}");
            return ExitCode::from(1);
        }
    }

    ExitCode::SUCCESS
}
