//! TASK 0720 harness: prints the real per-level privacy rule sets through the
//! real `cmd_osl_read_privacy_level_rule_set` command, so the privacy level
//! setting screen's mirrored effect table can be proven equal to what the
//! backend actually does (gate task 0701 connected these rules to the four
//! named protection choices).
//!
//! Usage:
//!   task_0720_privacy_level_cli levels
//!
//! Prints one JSON object with every level's rule set and exits 0.

use ipc::commands::cmd_osl_read_privacy_level_rule_set;
use ipc::state::AppState;

fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    match run(&args) {
        Ok(json) => println!("{json}"),
        Err(error) => {
            println!("{}", serde_json::json!({ "ok": false, "error": error }));
            std::process::exit(1);
        }
    }
}

fn run(args: &[String]) -> Result<String, String> {
    if args.first().map(String::as_str) != Some("levels") {
        return Err("usage: task_0720_privacy_level_cli levels".to_owned());
    }
    let state = AppState::new();
    let mut levels = Vec::new();
    for level in ["basic", "balanced", "maximum"] {
        let dto = cmd_osl_read_privacy_level_rule_set(&state, level.to_string())?;
        levels.push(serde_json::json!({
            "level": dto.level,
            "label": dto.label,
            "beforeSendWarnings": dto.before_send_warnings,
            "attachmentCleaning": dto.attachment_cleaning,
            "cleanupReviewDays": dto.cleanup_review_days,
            "publicPostChecks": dto.public_post_checks,
            "vpnRequiredActions": dto.vpn_required_actions,
            "protectedContactsRequired": dto.protected_contacts_required,
        }));
    }
    Ok(serde_json::json!({ "ok": true, "command": "levels", "levels": levels }).to_string())
}
