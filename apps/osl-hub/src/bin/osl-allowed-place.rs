use std::ffi::OsString;

fn main() {
    let mut args = std::env::args_os().collect::<Vec<_>>();
    if !args
        .iter()
        .any(|arg| arg == osl_privacy_hub::allowed_place_commands::ALLOWED_PLACE_CLI_FLAG)
    {
        args.insert(
            1,
            OsString::from(osl_privacy_hub::allowed_place_commands::ALLOWED_PLACE_CLI_FLAG),
        );
    }
    let Some(result) = osl_privacy_hub::allowed_place_commands::run_allowed_place_cli(args) else {
        println!(
            "{{\"ok\":false,\"command\":\"unknown\",\"error\":\"usage: osl-allowed-place <add|remove|list|allowed> --store <dir>\"}}"
        );
        std::process::exit(2);
    };
    print!("{}", result.stdout);
    std::process::exit(result.exit_code);
}
