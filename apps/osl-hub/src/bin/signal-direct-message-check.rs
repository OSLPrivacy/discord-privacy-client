use std::process::ExitCode;

fn main() -> ExitCode {
    let fixture = std::env::args().nth(1).unwrap_or_default();
    match osl_privacy_hub::signal_direct_message_check::render_signal_direct_message_check(&fixture)
    {
        Ok(output) => {
            print!("{output}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("TASK1052_REFUSAL={error}");
            ExitCode::from(2)
        }
    }
}
