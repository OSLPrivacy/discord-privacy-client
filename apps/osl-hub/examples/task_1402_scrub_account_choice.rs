//! TASK 1402 -- the setup screen's real transport to the Scrub account
//! permission commands.
//!
//! The account-choice screen in `apps/osl-hub-ui/src/scrub-account-choice.ts`
//! sends one `save_scrub_account_permissions` call when Continue is pressed.
//! This example is that call: it takes the payload the screen built, hands it
//! to the same command the desktop build registers, and prints what the store
//! now holds. `read` is a separate process on purpose, so the answer comes back
//! off disk rather than out of the writer's own memory.
//!
//! Usage:
//!   task_1402_scrub_account_choice save <store-file> <owner> <payload-json>
//!   task_1402_scrub_account_choice read <store-file> <owner>

use std::path::PathBuf;

use osl_privacy_hub::hub_command_surface::{
    get_scrub_account_permissions_command, save_scrub_account_permissions_command,
};
use osl_privacy_hub::preferences::{PreviewState, ScrubAccountPermissionInput};

fn usage() -> ! {
    eprintln!(
        "usage: task_1402_scrub_account_choice save <store-file> <owner> <payload-json>\n       task_1402_scrub_account_choice read <store-file> <owner>"
    );
    std::process::exit(2);
}

fn fail(error: String) -> ! {
    println!("TASK1402_ERROR={error}");
    eprintln!("TASK1402_ERROR={error}");
    std::process::exit(1);
}

fn report(command: &str, account_ids: &[String]) {
    println!("TASK1402_COMMAND={command}");
    println!("TASK1402_ACCOUNT_IDS={}", account_ids.join(","));
    println!("TASK1402_ACCOUNT_COUNT={}", account_ids.len());
    // The line the UI test parses: exactly the `ScrubAccountPermissionRead`
    // shape the Tauri command returns to the renderer.
    println!(
        "TASK1402_JSON={}",
        serde_json::json!({ "accountIds": account_ids })
    );
}

fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    match args.first().map(String::as_str) {
        Some("save") => {
            if args.len() != 4 {
                usage();
            }
            let state = PreviewState::load(PathBuf::from(&args[1]));
            let input: ScrubAccountPermissionInput = match serde_json::from_str(&args[3]) {
                Ok(input) => input,
                Err(error) => fail(format!("payload is not a permission write: {error}")),
            };
            println!(
                "TASK1402_AVAILABLE_IDS={}",
                input.available_account_ids.join(",")
            );
            println!(
                "TASK1402_SELECTED_IDS={}",
                input.selected_account_ids.join(",")
            );
            match save_scrub_account_permissions_command(&state, &args[2], input) {
                Ok(read) => report("save_scrub_account_permissions", &read.account_ids),
                Err(error) => fail(error),
            }
        }
        Some("read") => {
            if args.len() != 3 {
                usage();
            }
            let state = PreviewState::load(PathBuf::from(&args[1]));
            match get_scrub_account_permissions_command(&state, &args[2]) {
                Ok(read) => report("get_scrub_account_permissions", &read.account_ids),
                Err(error) => fail(error),
            }
        }
        _ => usage(),
    }
}
