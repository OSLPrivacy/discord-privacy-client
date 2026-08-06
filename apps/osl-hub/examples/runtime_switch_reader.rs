#![cfg(feature = "core")]

use osl_privacy_hub::runtime_switches::{
    read_startup_test_only_runtime_switches_from_assignments,
    test_only_runtime_switch_choice_report, PASSWORD_SCREEN_ACCESS_SWITCH, SAFE_SENDING_SWITCH,
    TEST_ONLY_RUNTIME_SWITCH_LIST_NAME,
};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match read_startup_test_only_runtime_switches_from_assignments(args.iter().map(String::as_str))
    {
        Ok(switches) => {
            let report = test_only_runtime_switch_choice_report(&switches);
            println!("RUN-TIME SWITCH LIST: {TEST_ONLY_RUNTIME_SWITCH_LIST_NAME}");
            println!(
                "{}={}",
                PASSWORD_SCREEN_ACCESS_SWITCH, switches.password_screen_access
            );
            println!("{}={}", SAFE_SENDING_SWITCH, switches.safe_sending);
            println!(
                "OLD CHOICE {}={} source={}",
                PASSWORD_SCREEN_ACCESS_SWITCH, report.password_screen_access, report.source
            );
            println!(
                "OLD CHOICE {}={} source={}",
                SAFE_SENDING_SWITCH, report.safe_sending, report.source
            );
        }
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}
