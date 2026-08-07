use ipc::allowed_places::{count_allowed_place_records, get_allowed_place_record};
use ipc::app_preferences::{load_app_preferences, write_app_preferences, APP_PREFERENCES_VERSION};
use ipc::auto_whitelist_rules::{normalize_app_kind, AutoWhitelistRule};
use ipc::commands::{
    cmd_osl_direct_new_place, cmd_osl_get_auto_whitelist_rule, cmd_osl_save_auto_whitelist_rule,
};
use ipc::AppState;
use tempfile::tempdir;

struct FileStorageKeyGuard;

impl Drop for FileStorageKeyGuard {
    fn drop(&mut self) {
        ipc::main_password::set_file_storage_key(None);
    }
}

fn use_test_file_storage_key() -> FileStorageKeyGuard {
    ipc::main_password::set_file_storage_key(Some([0x13; 32]));
    FileStorageKeyGuard
}

#[derive(Clone)]
struct SaveRuleAttempt {
    app_kind: String,
    choice: String,
    config_dir: std::path::PathBuf,
}

impl SaveRuleAttempt {
    fn run(&self, state: &AppState) -> Result<ipc::commands::AutoWhitelistRuleDto, String> {
        cmd_osl_save_auto_whitelist_rule(
            state,
            self.app_kind.clone(),
            self.choice.clone(),
            Some(self.config_dir.clone()),
        )
    }
}

fn auto_rule_choice_count(config_dir: &std::path::Path) -> usize {
    load_app_preferences(&config_dir.join("app_preferences.json"))
        .auto_whitelist_rules
        .len()
}

fn prefs_bytes(config_dir: &std::path::Path) -> Vec<u8> {
    std::fs::read(config_dir.join("app_preferences.json")).expect("read app_preferences.json")
}

fn seed_auto_rule_choice(
    state: &AppState,
    config_dir: &std::path::Path,
    app_kind: &str,
    rule: AutoWhitelistRule,
) {
    let app_kind = normalize_app_kind(app_kind).expect("normalize seed app kind");
    let snapshot = {
        let mut prefs = state
            .app_preferences
            .lock()
            .expect("app_preferences mutex poisoned");
        prefs.version = APP_PREFERENCES_VERSION;
        prefs.auto_whitelist_rules.insert(
            app_kind,
            ipc::auto_whitelist_rules::parse_auto_whitelist_choice(rule.as_label())
                .expect("seed auto-whitelist choice"),
        );
        prefs.clone()
    };
    write_app_preferences(&config_dir.join("app_preferences.json"), &snapshot)
        .expect("write seeded auto-rule choice");
}

#[test]
fn task0134_unknown_auto_rule_choice_is_refused_without_rewriting_saved_choice() {
    let _file_storage_key = use_test_file_storage_key();
    let state = AppState::new();
    let dirs = tempdir().expect("temp dirs");
    let app_data_dir = dirs.path().join("app-data");
    let config_dir = dirs.path().join("prefs");
    std::fs::create_dir_all(&config_dir).expect("create prefs dir");
    let marker = "PEACH-0134";
    let app_kind = "discord";
    let place_kind = "direct_message";
    let place_id = "task-0134-friend-dm";

    seed_auto_rule_choice(&state, &config_dir, app_kind, AutoWhitelistRule::Always);

    let seed_decision = cmd_osl_direct_new_place(
        &state,
        app_data_dir.clone(),
        app_kind.to_string(),
        place_kind.to_string(),
        place_id.to_string(),
        Some(marker.to_string()),
    )
    .expect("seed marked allowed place");
    assert_eq!(seed_decision.result, "allowed");

    let seed_record = get_allowed_place_record(&app_data_dir, app_kind, place_kind, place_id)
        .expect("read seeded marked place")
        .expect("seeded marked place exists");
    let marker_readable = seed_record.display_name.as_deref().unwrap_or_default();
    let choice_count_before = auto_rule_choice_count(&config_dir);
    let allowed_count_before = count_allowed_place_records(&app_data_dir).expect("count before");

    println!("TASK0134 marker.readable={marker_readable}");
    println!("TASK0134 seed_choice.value=always");
    println!("TASK0134 choice_count.before={choice_count_before}");
    println!("TASK0134 allowed_place_count.before={allowed_count_before}");

    assert_eq!(marker_readable, marker);
    assert_eq!(choice_count_before, 1);
    assert_eq!(allowed_count_before, 1);

    let good = SaveRuleAttempt {
        app_kind: app_kind.to_string(),
        choice: "only if a friend".to_string(),
        config_dir: config_dir.clone(),
    };
    let good_saved = good.run(&state).expect("save only-if-friend rule");
    let good_read =
        cmd_osl_get_auto_whitelist_rule(&state, app_kind.to_string()).expect("read good rule");
    let choice_count_after_good = auto_rule_choice_count(&config_dir);
    let allowed_count_after_good =
        count_allowed_place_records(&app_data_dir).expect("count after good save");
    let prefs_after_good = prefs_bytes(&config_dir);
    let marker_after_good = get_allowed_place_record(&app_data_dir, app_kind, place_kind, place_id)
        .expect("read marked place after good save")
        .expect("marked place still exists")
        .display_name
        .expect("marked place keeps display name");
    let unchanged_pair_after_good = format!("{good_read}\n{marker_after_good}").into_bytes();

    println!("TASK0134 good_save.return={good_saved}");
    println!("TASK0134 good_save.read={good_read}");
    println!("TASK0134 choice_count.after_good={choice_count_after_good}");
    println!("TASK0134 allowed_place_count.after_good={allowed_count_after_good}");

    assert_eq!(good_saved, "only if a friend");
    assert_eq!(good_read, "only if a friend");
    assert_eq!(choice_count_after_good, 1);
    assert_eq!(allowed_count_after_good, 1);

    let bad = SaveRuleAttempt {
        choice: "dragonfruit-0134".to_string(),
        ..good.clone()
    };
    assert_eq!(bad.app_kind, good.app_kind);
    assert_eq!(bad.config_dir, good.config_dir);
    assert_ne!(bad.choice, good.choice);

    let bad_error = bad.run(&state).expect_err("unknown choice refused");
    let after_bad_read =
        cmd_osl_get_auto_whitelist_rule(&state, app_kind.to_string()).expect("read after bad rule");
    let choice_count_after_bad = auto_rule_choice_count(&config_dir);
    let allowed_count_after_bad =
        count_allowed_place_records(&app_data_dir).expect("count after bad save");
    let prefs_after_bad = prefs_bytes(&config_dir);
    let marker_after_bad = get_allowed_place_record(&app_data_dir, app_kind, place_kind, place_id)
        .expect("read marked place after bad save")
        .expect("marked place still exists")
        .display_name
        .expect("marked place keeps display name after bad save");
    let unchanged_pair_after_bad = format!("{after_bad_read}\n{marker_after_bad}").into_bytes();

    println!("TASK0134 bad_save.changed_field=choice");
    println!("TASK0134 bad_save.choice={}", bad.choice);
    println!("TASK0134 bad_save.error={bad_error}");
    println!("TASK0134 choice_count.after_bad={choice_count_after_bad}");
    println!("TASK0134 allowed_place_count.after_bad={allowed_count_after_bad}");
    println!("TASK0134 after_bad.read={after_bad_read}");
    println!("TASK0134 after_bad.marker={marker_after_bad}");
    println!(
        "TASK0134 prefs_bytes_unchanged_after_bad={}",
        prefs_after_bad == prefs_after_good
    );
    println!(
        "TASK0134 rule_plus_marker_unchanged_after_bad={}",
        unchanged_pair_after_bad == unchanged_pair_after_good
    );

    assert!(
        bad_error.contains("unknown auto-rule choice"),
        "unknown-choice error must name unknown auto-rule choice: {bad_error}"
    );
    assert_eq!(choice_count_after_bad, 1);
    assert_eq!(allowed_count_after_bad, 1);
    assert_eq!(after_bad_read, "only if a friend");
    assert_eq!(marker_after_bad, marker);
    assert_eq!(prefs_after_bad, prefs_after_good);
    assert_eq!(unchanged_pair_after_bad, unchanged_pair_after_good);
}
