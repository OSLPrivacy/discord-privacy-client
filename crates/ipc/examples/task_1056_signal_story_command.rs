use ipc::signal_story::run_signal_story_command;

fn main() {
    let result = run_signal_story_command(std::env::args_os().skip(1));
    print!("{}", result.stdout);
    std::process::exit(result.exit_code);
}
