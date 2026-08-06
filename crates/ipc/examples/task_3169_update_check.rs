use ipc::commands::{
    cmd_osl_check_site_for_update, SiteUpdateCheckResult, DEFAULT_OSL_SITE_UPDATE_MANIFEST_URL,
};

fn usage() -> &'static str {
    "usage: task_3169_update_check [--current VERSION] [--manifest-url URL] [--target TARGET] [--arch ARCH]"
}

fn next_arg(args: &[String], index: &mut usize, name: &str) -> Result<String, String> {
    *index += 1;
    args.get(*index)
        .cloned()
        .ok_or_else(|| format!("{name} needs a value"))
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut current = env!("CARGO_PKG_VERSION").to_owned();
    let mut manifest_url = DEFAULT_OSL_SITE_UPDATE_MANIFEST_URL.to_owned();
    let mut target = "windows".to_owned();
    let mut arch = "x86_64".to_owned();

    let mut index = 1;
    while index < args.len() {
        let parsed = match args[index].as_str() {
            "--current" => next_arg(&args, &mut index, "--current").map(|v| current = v),
            "--manifest-url" => {
                next_arg(&args, &mut index, "--manifest-url").map(|v| manifest_url = v)
            }
            "--target" => next_arg(&args, &mut index, "--target").map(|v| target = v),
            "--arch" => next_arg(&args, &mut index, "--arch").map(|v| arch = v),
            "--help" | "-h" => {
                println!("{}", usage());
                return;
            }
            other => Err(format!("unknown argument: {other}")),
        };
        if let Err(message) = parsed {
            println!("error: {message}\n{}", usage());
            std::process::exit(2);
        }
        index += 1;
    }

    let result = cmd_osl_check_site_for_update(current, manifest_url, target, arch);
    println!("{}", result.direct_command_output());
    if matches!(result, SiteUpdateCheckResult::Error { .. }) {
        std::process::exit(1);
    }
}
