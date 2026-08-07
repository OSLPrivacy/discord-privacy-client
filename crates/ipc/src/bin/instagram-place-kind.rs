use ipc::allowed_places::AllowedPlaceRecord;
use ipc::auto_whitelist_rules::{
    instagram_auto_whitelist_rule_key, parse_instagram_whitelist_kind,
};
use ipc::commands::{cmd_osl_new_place, cmd_osl_save_auto_whitelist_rule};
use ipc::AppState;
use std::process::ExitCode;

fn main() -> ExitCode {
    let Some(kind) = std::env::args().nth(1) else {
        eprintln!("usage: instagram-place-kind <kind>");
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
    let kind = parse_instagram_whitelist_kind(kind)?;
    let state = AppState::new();
    cmd_osl_save_auto_whitelist_rule(
        &state,
        instagram_auto_whitelist_rule_key(kind),
        "always".to_owned(),
        None,
    )?;

    let kind_id = kind.id();
    let dir = unique_allowed_place_dir(kind_id);
    let decision = cmd_osl_new_place(
        &state,
        AllowedPlaceRecord {
            app: "instagram".to_owned(),
            account: "instagram-account-3744".to_owned(),
            kind: kind_id.to_owned(),
            stable_id: format!("instagram:instagram-account-3744:{kind_id}:place-3744"),
            place_name: format!("Instagram {kind_id} fixture"),
            person_name: "Instagram Fixture".to_owned(),
        },
        Some(dir.clone()),
    )?;

    println!(
        "TASK3744_INSTAGRAM_PLACE kind={} status={} rule={} prompt={}",
        kind_id, decision.status, decision.rule, decision.prompt
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
        "osl-task3744-instagram-place-{}-{kind}-{nanos}",
        std::process::id()
    ))
}
