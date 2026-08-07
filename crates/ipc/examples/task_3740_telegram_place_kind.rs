use ipc::commands::{cmd_osl_get_telegram_whitelist_kinds, cmd_osl_read_auto_whitelist_rule};
use ipc::state::AppState;

fn print_allowed(label: &str, kind: &str) -> Result<(), String> {
    let state = AppState::new();
    let rule = cmd_osl_read_auto_whitelist_rule(&state, format!("telegram:{kind}"))?;
    let place = rule
        .allowed_place
        .ok_or_else(|| format!("refusal kind={kind}"))?;
    println!(
        "{label}=allowed kind={} allowed_place={}:{}",
        place.kind, place.app, place.kind
    );
    Ok(())
}

fn print_list() -> Result<(), String> {
    let state = AppState::new();
    let kinds = cmd_osl_get_telegram_whitelist_kinds()?;
    let ids: Vec<String> = kinds.iter().map(|kind| kind.id.clone()).collect();
    let allowed = ids
        .iter()
        .map(|id| cmd_osl_read_auto_whitelist_rule(&state, format!("telegram:{id}")))
        .collect::<Result<Vec<_>, _>>()?;

    println!("telegram_kind_count={}", ids.len());
    println!("telegram_kinds={}", ids.join(","));
    println!(
        "telegram_allowed_place_count={}",
        allowed
            .iter()
            .filter(|rule| rule.allowed_place.is_some())
            .count()
    );
    println!(
        "telegram_allowed_place_answers={}",
        allowed
            .iter()
            .map(|rule| {
                let place = rule.allowed_place.as_ref().expect("allowed place");
                format!("{}:{}", place.app, place.kind)
            })
            .collect::<Vec<_>>()
            .join(",")
    );
    Ok(())
}

fn run(arg: &str) -> Result<(), String> {
    match arg {
        "list" => print_list(),
        "1027" => print_allowed("task1027", "supergroup"),
        "1028" => print_allowed("task1028", "saved_messages"),
        "story" => print_allowed("story", "story"),
        invented => print_allowed("invented", invented),
    }
}

fn main() {
    let arg = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "list".to_string());
    if let Err(error) = run(&arg) {
        eprintln!("refusal kind={arg} error={error}");
        std::process::exit(1);
    }
}
