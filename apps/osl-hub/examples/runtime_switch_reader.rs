#![cfg(feature = "core")]

use osl_privacy_hub::runtime_switches::{
    read_test_only_runtime_switches, PASSWORD_SCREEN_ACCESS_SWITCH, SAFE_SENDING_SWITCH,
    TEST_ONLY_RUNTIME_SWITCH_LIST_NAME,
};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match read_test_only_runtime_switches(args.iter().map(String::as_str)) {
        Ok(switches) => {
            println!("RUN-TIME SWITCH LIST: {TEST_ONLY_RUNTIME_SWITCH_LIST_NAME}");
            println!(
                "{}={}",
                PASSWORD_SCREEN_ACCESS_SWITCH, switches.password_screen_access
            );
            println!("{}={}", SAFE_SENDING_SWITCH, switches.safe_sending);
        }
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}
