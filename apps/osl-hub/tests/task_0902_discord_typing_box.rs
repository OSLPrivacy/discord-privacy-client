#[path = "../src/discord_typing_box_check.rs"]
mod discord_typing_box_check;

use discord_typing_box_check::{
    direct_message_from_composer_name, run_discord_typing_box_check_cli,
};
use serde_json::Value;
use std::ffi::OsString;

#[test]
fn direct_command_fails_closed_when_the_live_discord_carrier_is_absent() {
    assert_eq!(
        direct_message_from_composer_name("Message @OSL live peer"),
        Some("OSL live peer")
    );
    assert_eq!(direct_message_from_composer_name("Message #group"), None);
    assert_eq!(
        direct_message_from_composer_name("Email or Phone Number"),
        None
    );

    let result = run_discord_typing_box_check_cli([
        OsString::from("osl-hub"),
        OsString::from("--check-discord-typing-box"),
    ])
    .expect("recognized command");
    let output: Value = serde_json::from_str(result.stdout.trim()).expect("JSON output");
    assert_eq!(result.exit_code, 1);
    assert_eq!(output["ok"], false);
    assert_eq!(output["carrier"], "Discord");
    assert_eq!(output["typingBoxes"], 0);
    println!("TASK0902 carrier_absent_typing_boxes=0 exit_code=1");
}
