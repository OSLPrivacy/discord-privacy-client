use std::{env, fs, process};

fn refuse(error: impl std::fmt::Display) -> ! {
    eprintln!("5205b: {error}");
    process::exit(1)
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    match args.as_slice() {
        [command, caller, path] if command == "check-external" => {
            let source = fs::read_to_string(path)
                .unwrap_or_else(|error| refuse(format!("caller={caller} key=<catalogue> fallback=disabled: {path}: {error}")));
            let result = match caller.as_str() {
                "windows" => task_5205_catalogue::load_external_windows_catalogue(&source),
                "service" => task_5205_catalogue::load_external_service_catalogue(&source),
                _ => refuse(format!("caller={caller} key=<catalogue> fallback=parallel-permissive-loader: bypassing caller is not a production entry point")),
            };
            match result {
                Ok(catalogue) => println!(
                    "5205b accepted caller={} version={} keys={} fallback=disabled",
                    catalogue.caller(),
                    catalogue.version(),
                    catalogue.keys().count()
                ),
                Err(error) => refuse(error),
            }
        }
        [command, caller] if command == "attempt-bypass" => refuse(format!(
            "caller={caller} key=<catalogue> fallback=parallel-permissive-loader: shipping entry point routed around strict loader"
        )),
        _ => refuse("caller=<cli> key=<catalogue> fallback=disabled: usage: task-5205b check-external <windows|service> <path> | attempt-bypass <caller>"),
    }
}
