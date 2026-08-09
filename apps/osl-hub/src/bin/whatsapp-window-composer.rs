use std::process::ExitCode;

fn main() -> ExitCode {
    let fixture = std::env::args().nth(1).unwrap_or_default();
    match osl_privacy_hub::whatsapp_window_composer::render_prepared_whatsapp_fixture(&fixture) {
        Ok(output) => {
            print!("{output}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(2)
        }
    }
}
