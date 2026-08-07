use ipc::app_preferences::load_app_preferences;
use ipc::commands::{
    cmd_osl_get_language_choice, cmd_osl_read_screen_words, cmd_osl_save_language_choice,
};
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
fn task3160_direct_commands_save_language_read_words_and_refuse_missing_words_file() {
    let _osl_serial = OSL_PROCESS_GLOBALS_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().expect("tempdir");
    let _config_dir = use_temp_config_dir(dir.path());
    let prefs_path = dir.path().join("app_preferences.json");
    let state = AppState::new();

    let saved_english =
        cmd_osl_save_language_choice(&state, "en".to_string(), Some(dir.path().to_path_buf()))
            .expect("save English");
    let read_english_choice = cmd_osl_get_language_choice(&state).expect("read English choice");
    let english_words =
        cmd_osl_read_screen_words(&state, "welcome".to_string()).expect("read English words");
    println!("TASK3160 direct_command=cmd_osl_save_language_choice");
    println!("TASK3160 save_english.return={saved_english}");
    println!("TASK3160 read_english_choice={read_english_choice}");
    println!("TASK3160 english.language={}", english_words.language);
    println!("TASK3160 english.screen={}", english_words.screen);
    println!(
        "TASK3160 english.title={}",
        english_words.words.get("title").expect("English title")
    );
    println!(
        "TASK3160 english.body={}",
        english_words.words.get("body").expect("English body")
    );
    println!(
        "TASK3160 english.primary_button={}",
        english_words
            .words
            .get("primary_button")
            .expect("English primary button")
    );
    assert_eq!(saved_english, "en");
    assert_eq!(read_english_choice, "en");
    assert_eq!(
        load_app_preferences(&prefs_path).language,
        "en",
        "English choice must persist to app_preferences.json"
    );
    assert_eq!(
        english_words.words.get("title").map(String::as_str),
        Some("Welcome to OSL")
    );
    assert_eq!(
        english_words.words.get("body").map(String::as_str),
        Some("Private chat starts here.")
    );
    assert_eq!(
        english_words
            .words
            .get("primary_button")
            .map(String::as_str),
        Some("Create account")
    );

    let saved_spanish =
        cmd_osl_save_language_choice(&state, "es".to_string(), Some(dir.path().to_path_buf()))
            .expect("save Spanish");
    let spanish_words =
        cmd_osl_read_screen_words(&state, "welcome".to_string()).expect("read Spanish words");
    println!("TASK3160 save_second_language.return={saved_spanish}");
    println!(
        "TASK3160 second_language.language={}",
        spanish_words.language
    );
    println!("TASK3160 second_language.screen={}", spanish_words.screen);
    println!(
        "TASK3160 second_language.title={}",
        spanish_words.words.get("title").expect("Spanish title")
    );
    println!(
        "TASK3160 second_language.body={}",
        spanish_words.words.get("body").expect("Spanish body")
    );
    println!(
        "TASK3160 second_language.primary_button={}",
        spanish_words
            .words
            .get("primary_button")
            .expect("Spanish primary button")
    );
    assert_eq!(saved_spanish, "es");
    assert_eq!(
        load_app_preferences(&prefs_path).language,
        "es",
        "second-language choice must persist to app_preferences.json"
    );
    assert_eq!(
        spanish_words.words.get("title").map(String::as_str),
        Some("Bienvenido a OSL")
    );
    assert_eq!(
        spanish_words.words.get("body").map(String::as_str),
        Some("El chat privado empieza aqui.")
    );
    assert_eq!(
        spanish_words
            .words
            .get("primary_button")
            .map(String::as_str),
        Some("Crear cuenta")
    );

    let missing_language_error =
        cmd_osl_save_language_choice(&state, "fr".to_string(), Some(dir.path().to_path_buf()))
            .expect_err("missing language words file refused");
    let after_missing = cmd_osl_get_language_choice(&state).expect("read after missing language");
    println!("TASK3160 missing_words_file_refused=true");
    println!("TASK3160 missing_words_file.error={missing_language_error}");
    println!("TASK3160 after_missing_language.read={after_missing}");
    assert!(
        missing_language_error.contains("no screen words file for language"),
        "missing words file error must say why: {missing_language_error}"
    );
    assert_eq!(after_missing, "es");
}
