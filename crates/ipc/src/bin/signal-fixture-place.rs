use ipc::commands::{
    cmd_osl_read_signal_auto_whitelist_rule, cmd_osl_save_signal_auto_whitelist_rule,
};
use ipc::state::AppState;

fn main() {
    let Some(kind) = std::env::args().nth(1) else {
        eprintln!("TASK 0152 signal fixture place error: missing kind");
        std::process::exit(1);
    };

    let state = AppState::new();
    let account = "signal-account-0152".to_owned();
    let place = format!("signal-place-0152-{kind}");
    let choice = if kind == "direct_message" {
        "always"
    } else {
        "only if a friend"
    };

    if let Err(error) = cmd_osl_save_signal_auto_whitelist_rule(
        &state,
        kind.clone(),
        account.clone(),
        place.clone(),
        choice.to_owned(),
    ) {
        eprintln!("TASK 0152 signal fixture place error: {error}");
        std::process::exit(1);
    }

    match cmd_osl_read_signal_auto_whitelist_rule(&state, kind, account, place) {
        Ok(resolved) => {
            println!(
                "TASK 0152 fixture place resolved: kind={} auto_rule={} allowed_place_kind={} stable_id={} choice={}",
                resolved.signal_kind,
                resolved.auto_rule_app_kind,
                resolved.allowed_place.kind,
                resolved.allowed_place.stable_id,
                resolved.choice
            );
        }
        Err(error) => {
            eprintln!("TASK 0152 signal fixture place error: {error}");
            std::process::exit(1);
        }
    }
}
