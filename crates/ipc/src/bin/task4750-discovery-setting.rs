use ipc::commands::{
    cmd_osl_list_discovery_setting_choices, cmd_osl_read_discovery_replies_switch,
    cmd_osl_read_discovery_setting, cmd_osl_save_discovery_setting,
    cmd_osl_walk_discovery_publish_path,
};
use ipc::state::AppState;
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let Some(command) = args.next() else {
        eprintln!(
            "usage: task4750-discovery-setting <read-new-profile|reject|choices|publish-off|shipped-default>"
        );
        return ExitCode::from(2);
    };

    match command.as_str() {
        "read-new-profile" => {
            let state = AppState::new();
            println!(
                "{}",
                cmd_osl_read_discovery_setting(&state).expect("read setting")
            );
            println!(
                "{}",
                cmd_osl_read_discovery_replies_switch(&state).expect("read discovery replies")
            );
            ExitCode::SUCCESS
        }
        "reject" => {
            let value = args.next().unwrap_or_else(|| "fifth".to_owned());
            let state = AppState::new();
            match cmd_osl_save_discovery_setting(&state, value, None) {
                Ok(saved) => {
                    println!("{saved}");
                    ExitCode::from(1)
                }
                Err(error) => {
                    println!("{error}");
                    ExitCode::SUCCESS
                }
            }
        }
        "choices" => {
            for choice in cmd_osl_list_discovery_setting_choices().expect("list choices") {
                println!("{choice}");
            }
            ExitCode::SUCCESS
        }
        "publish-off" => {
            let state = AppState::new();
            let saved =
                cmd_osl_save_discovery_setting(&state, "anyone".to_owned(), None).expect("save");
            if saved != "anyone" {
                println!("{saved}");
                return ExitCode::from(1);
            }
            let report = cmd_osl_walk_discovery_publish_path(&state).expect("walk publish path");
            println!("{} cards", report.cards_written);
            println!("{}", report.status);
            ExitCode::SUCCESS
        }
        "shipped-default" => {
            let found = ipc::app_preferences::DiscoverySetting::default().as_str();
            println!("{found}");
            if found == "never" {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            }
        }
        other => {
            eprintln!("unknown command: {other}");
            ExitCode::from(2)
        }
    }
}
