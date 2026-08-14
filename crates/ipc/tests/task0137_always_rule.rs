use ipc::allowed_places::{allowed_places_db_path, count_allowed_place_records};
use ipc::commands::{
    cmd_osl_direct_new_place, cmd_osl_get_auto_whitelist_rule, cmd_osl_save_auto_whitelist_rule,
};
use ipc::AppState;
use rusqlite::Connection;
use tempfile::tempdir;

fn direct_allowed_place_count(app_data_dir: &std::path::Path) -> i64 {
    let conn = Connection::open(allowed_places_db_path(app_data_dir)).expect("open sqlite store");
    conn.query_row("SELECT COUNT(*) FROM allowed_places", [], |row| row.get(0))
        .expect("count allowed places")
}

#[test]
fn direct_new_place_returns_allowed_and_adds_one_record_when_rule_is_always() {
    let state = AppState::new();
    let dirs = tempdir().expect("temp dirs");
    let app_data_dir = dirs.path().join("app-data");
    let prefs_dir = dirs.path().join("prefs");

    let saved = cmd_osl_save_auto_whitelist_rule(
        &state,
        "discord".to_string(),
        "always".to_string(),
        Some(prefs_dir),
    )
    .expect("save rule");
    assert_eq!(saved, "always");
    let rule = cmd_osl_get_auto_whitelist_rule(&state, "discord".to_string()).expect("read rule");
    assert_eq!(rule, "always");

    assert_eq!(
        count_allowed_place_records(&app_data_dir).expect("initialize and count"),
        0
    );
    let before = direct_allowed_place_count(&app_data_dir);

    let decision = cmd_osl_direct_new_place(
        &state,
        app_data_dir.clone(),
        "discord".to_string(),
        "direct_message".to_string(),
        "900000000000013701".to_string(),
        Some("task 0137 direct dm".to_string()),
    )
    .expect("direct new-place command");

    let after = direct_allowed_place_count(&app_data_dir);
    let rise = after - before;
    println!("TASK0137 direct_new_place.result={}", decision.result);
    println!("TASK0137 allowed_place_count.before={before}");
    println!("TASK0137 allowed_place_count.after={after}");
    println!("TASK0137 allowed_place_count.rise={rise}");

    assert_eq!(decision.result, "allowed");
    assert_eq!(rise, 1);
}
