use ipc::commands::{cmd_osl_read_screen_words, cmd_osl_save_language_choice};
use ipc::AppState;
use keystore::{set_active_account_dir, set_base_dir_override};
use std::sync::Mutex;

static OSL_PROCESS_GLOBALS_LOCK: Mutex<()> = Mutex::new(());

struct ConfigDirGuard;

impl Drop for ConfigDirGuard {
    fn drop(&mut self) {
        set_active_account_dir(None);
        set_base_dir_override(None);
        ipc::main_password::set_file_storage_key(None);
    }
}

fn use_temp_config_dir(dir: &std::path::Path) -> ConfigDirGuard {
    set_active_account_dir(None);
    set_base_dir_override(Some(dir.to_path_buf()));
    ipc::main_password::set_file_storage_key(None);
    ConfigDirGuard
}

/// Reads the "settings" screen's "heading" word for whatever language is
/// currently saved in app_preferences.
fn read_settings_heading(state: &AppState) -> String {
    cmd_osl_read_screen_words(state, "settings".to_string())
        .expect("read settings screen words")
        .words
        .get("heading")
        .expect("settings screen must have a heading word")
        .clone()
}

#[test]
fn task3162_changing_language_changes_the_settings_heading_and_switching_back_restores_it() {
    let _osl_serial = OSL_PROCESS_GLOBALS_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().expect("tempdir");
    let _config_dir = use_temp_config_dir(dir.path());
    let state = AppState::new();

    cmd_osl_save_language_choice(&state, "en".to_string(), Some(dir.path().to_path_buf()))
        .expect("save English");
    let english_heading = read_settings_heading(&state);
    println!("TASK3162 english.heading={english_heading}");
    assert_eq!(
        english_heading, "Settings",
        "English settings heading must match crates/ipc/src/screen_words/en.json exactly"
    );

    cmd_osl_save_language_choice(&state, "es".to_string(), Some(dir.path().to_path_buf()))
        .expect("save Spanish");
    let spanish_heading = read_settings_heading(&state);
    println!("TASK3162 spanish.heading={spanish_heading}");
    assert_eq!(
        spanish_heading, "Configuracion",
        "Spanish settings heading must match crates/ipc/src/screen_words/es.json exactly"
    );

    assert_ne!(
        english_heading, spanish_heading,
        "the two headings must be different strings"
    );

    cmd_osl_save_language_choice(&state, "en".to_string(), Some(dir.path().to_path_buf()))
        .expect("switch back to English");
    let english_heading_again = read_settings_heading(&state);
    println!("TASK3162 english_again.heading={english_heading_again}");
    assert_eq!(
        english_heading_again, english_heading,
        "switching back to English must return the first heading string"
    );
}
