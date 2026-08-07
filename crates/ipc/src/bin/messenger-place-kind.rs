use ipc::allowed_places::AllowedPlaceRecord;
use ipc::auto_whitelist_rules::parse_messenger_whitelist_kind;
use ipc::commands::{cmd_osl_new_place, cmd_osl_save_auto_whitelist_rule};
use ipc::AppState;
use std::process::ExitCode;

fn main() -> ExitCode {
    let Some(kind) = std::env::args().nth(1) else {
        eprintln!("usage: messenger-place-kind <kind>");
        return ExitCode::from(2);
    };

    match run(&kind) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(1)
        }
    }
}

fn run(kind: &str) -> Result<(), String> {
    let state = AppState::new();
    let kind = parse_messenger_whitelist_kind(kind)?.id();
    let rule_key = format!("messenger:{kind}");
    cmd_osl_save_auto_whitelist_rule(&state, rule_key, "always".to_owned(), None)?;

    let place = AllowedPlaceRecord {
        app: "messenger".to_owned(),
        account: "messenger-account-0164".to_owned(),
        kind: kind.to_owned(),
        stable_id: format!("messenger:messenger-account-0164:{kind}:place-0164"),
        place_name: format!("Messenger {kind} fixture"),
        person_name: "Messenger Fixture".to_owned(),
    };
    let dir = unique_allowed_place_dir(kind);
    let decision = cmd_osl_new_place(&state, place, Some(dir.clone()))?;
    println!(
        "TASK0164_MESSENGER_PLACE kind={} status={} rule={} prompt={}",
        kind, decision.status, decision.rule, decision.prompt
    );
    let _ = std::fs::remove_dir_all(dir);
    Ok(())
}

fn unique_allowed_place_dir(kind: &str) -> std::path::PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!(
        "osl-task0164-messenger-place-{}-{kind}-{nanos}",
        std::process::id()
    ))
}
