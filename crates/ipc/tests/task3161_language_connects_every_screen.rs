//! TASK 3161 - connect the language choice to every screen.
//!
//! Proves, from direct commands (no UI, no process restart between steps):
//! - every screen this build ships (`screen_words::known_screens()`) has a
//!   complete word set in English AND in the second language, with no key
//!   missing from either side;
//! - reading a screen's words after `cmd_osl_save_language_choice` changes
//!   the language returns the new language's words for the SAME screen, in
//!   the SAME running process — the mechanism `initialLanguage` /
//!   `setLanguage` in the frontend language store rides to make a language
//!   change apply without restarting the app;
//! - a language with no words file is refused, not silently defaulted.

use ipc::commands::{
    cmd_osl_get_language_choice, cmd_osl_read_screen_words, cmd_osl_save_language_choice,
};
use ipc::screen_words::known_screens;
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

#[test]
fn task3161_every_known_screen_has_complete_words_in_both_languages() {
    let _osl_serial = OSL_PROCESS_GLOBALS_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().expect("tempdir");
    let _config_dir = use_temp_config_dir(dir.path());
    let state = AppState::new();

    let screens = known_screens();
    println!("TASK3161 known_screens={screens:?}");
    assert!(
        screens.contains(&"verification_warning".to_string()),
        "verification_warning screen must be connected"
    );
    assert!(
        screens.contains(&"view_once_player".to_string()),
        "view_once_player screen must be connected"
    );
    assert!(
        screens.len() >= 2,
        "at least the two converted screens must be present"
    );

    for screen in &screens {
        let english = cmd_osl_read_screen_words(&state, screen.clone())
            .unwrap_or_else(|e| panic!("English words for {screen}: {e}"));
        let spanish_language =
            cmd_osl_save_language_choice(&state, "es".to_string(), Some(dir.path().to_path_buf()))
                .expect("switch to Spanish");
        assert_eq!(spanish_language, "es");
        let spanish = cmd_osl_read_screen_words(&state, screen.clone())
            .unwrap_or_else(|e| panic!("Spanish words for {screen}: {e}"));
        cmd_osl_save_language_choice(&state, "en".to_string(), Some(dir.path().to_path_buf()))
            .expect("switch back to English");

        let mut english_keys: Vec<&String> = english.words.keys().collect();
        let mut spanish_keys: Vec<&String> = spanish.words.keys().collect();
        english_keys.sort();
        spanish_keys.sort();
        println!("TASK3161 screen={screen} en_keys={english_keys:?}");
        println!("TASK3161 screen={screen} es_keys={spanish_keys:?}");
        assert_eq!(
            english_keys, spanish_keys,
            "screen {screen} must have the SAME word keys in every shipped language"
        );
        assert!(
            !english.words.is_empty(),
            "screen {screen} must actually hold words, not an empty set"
        );
        for (key, en_value) in &english.words {
            let es_value = spanish.words.get(key).expect("checked keys match above");
            assert_ne!(
                en_value, es_value,
                "screen {screen} word {key} must actually be translated, not copied verbatim"
            );
        }
    }
}

#[test]
fn task3161_switching_language_changes_screen_words_without_restart() {
    let _osl_serial = OSL_PROCESS_GLOBALS_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().expect("tempdir");
    let _config_dir = use_temp_config_dir(dir.path());
    let state = AppState::new();

    // One AppState, one process, no restart between these two reads.
    let default_language = cmd_osl_get_language_choice(&state).expect("default language");
    println!("TASK3161 default_language={default_language}");
    assert_eq!(
        default_language, "en",
        "unset language must default to English"
    );

    let before = cmd_osl_read_screen_words(&state, "verification_warning".to_string())
        .expect("English verification_warning words");
    println!(
        "TASK3161 before.heading={}",
        before.words.get("heading").expect("English heading")
    );
    assert_eq!(
        before.words.get("heading").map(String::as_str),
        Some("Verification warning")
    );

    let switched =
        cmd_osl_save_language_choice(&state, "es".to_string(), Some(dir.path().to_path_buf()))
            .expect("switch language mid-session");
    assert_eq!(switched, "es");

    // SAME screen, SAME AppState, SAME process — only the saved language
    // preference changed in between.
    let after = cmd_osl_read_screen_words(&state, "verification_warning".to_string())
        .expect("Spanish verification_warning words");
    println!(
        "TASK3161 after.heading={}",
        after.words.get("heading").expect("Spanish heading")
    );
    assert_eq!(
        after.words.get("heading").map(String::as_str),
        Some("Advertencia de verificacion")
    );
    assert_ne!(
        before.words.get("heading"),
        after.words.get("heading"),
        "the same screen must show different words after the language changes, with no restart"
    );

    let refused =
        cmd_osl_save_language_choice(&state, "fr".to_string(), Some(dir.path().to_path_buf()))
            .expect_err("language with no words file must be refused");
    println!("TASK3161 refused_language.error={refused}");
    assert!(refused.contains("no screen words file for language"));
    let still_spanish = cmd_osl_get_language_choice(&state).expect("language unchanged");
    println!("TASK3161 after_refused_language={still_spanish}");
    assert_eq!(
        still_spanish, "es",
        "a refused language choice must not overwrite the last valid one"
    );
}
