//! TASK 0778 - prove saved window, movement, tray, and sound choices.
//!
//! This writes a machine-readable result file from the four app actions, then
//! compares that result to the preferences saved on disk.  The comparison is
//! deliberately run again with one changed value in a throwaway preferences
//! copy, so this proof cannot pass if it ignores a saved choice.

use ipc::app_preferences::{load_app_preferences, write_app_preferences, AppPreferences};
use ipc::commands::{
    cmd_osl_apply_movement_behaviour, cmd_osl_open_window_with_behaviour,
    cmd_osl_play_sound_with_behaviour, cmd_osl_save_behaviour_choice,
    cmd_osl_show_tray_with_behaviour, MovementBehaviourDto, SoundBehaviourDto, TrayBehaviourDto,
    WindowOpeningBehaviourDto,
};
use ipc::AppState;
use keystore::{set_active_account_dir, set_base_dir_override};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::sync::Mutex;
use tempfile::tempdir;

static KEY_LOCK: Mutex<()> = Mutex::new(());

struct ConfigDirGuard;

impl Drop for ConfigDirGuard {
    fn drop(&mut self) {
        set_active_account_dir(None);
        set_base_dir_override(None);
        ipc::main_password::set_file_storage_key(None);
    }
}

fn use_temp_config_dir(dir: &Path) -> ConfigDirGuard {
    set_active_account_dir(None);
    set_base_dir_override(Some(dir.to_path_buf()));
    ipc::main_password::set_file_storage_key(None);
    ConfigDirGuard
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
struct BehaviourActionResult {
    window: WindowOpeningBehaviourDto,
    movement: MovementBehaviourDto,
    tray: TrayBehaviourDto,
    sound: SoundBehaviourDto,
}

fn saved<'a>(prefs: &'a AppPreferences, name: &str) -> Result<&'a str, String> {
    prefs
        .behaviour_choices
        .get(name)
        .map(String::as_str)
        .ok_or_else(|| format!("saved behaviour choice is missing: {name}"))
}

fn assert_result_matches_saved(
    prefs: &AppPreferences,
    result: &BehaviourActionResult,
) -> Result<(), String> {
    let actual = [
        ("position", result.window.position.as_str()),
        ("remember place", result.window.remember_place.as_str()),
        ("movement", result.movement.movement.as_str()),
        ("tray picture", result.tray.tray_picture.as_str()),
        ("sound", result.sound.sound.as_str()),
        ("mute", result.sound.mute.as_str()),
        ("quiet hours", result.sound.quiet_hours.as_str()),
    ];
    for (name, actual) in actual {
        let expected = saved(prefs, name)?;
        if actual != expected {
            return Err(format!(
                "behaviour result mismatch for {name}: expected {expected:?}, got {actual:?}"
            ));
        }
    }
    Ok(())
}

#[test]
fn task_0778_saved_choices_drive_action_result_and_tampering_fails_the_check() {
    let _serial = KEY_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    let dir = tempdir().expect("temporary configuration directory");
    let _config_dir = use_temp_config_dir(dir.path());
    let prefs_path = dir.path().join("app_preferences.json");
    let result_path = dir.path().join("behaviour-action-result.json");
    let state = AppState::new();
    let choices = [
        ("position", "x=611,y=377,w=920,h=640"),
        ("remember place", "restore-last-window"),
        ("movement", "snap-to-active-display"),
        ("tray picture", "monogram-shield"),
        ("sound", "low-bell"),
        ("mute", "unmuted"),
        ("quiet hours", "23:30-06:15"),
    ];

    for (name, value) in choices {
        let saved = cmd_osl_save_behaviour_choice(
            &state,
            name.to_owned(),
            value.to_owned(),
            Some(dir.path().to_path_buf()),
        )
        .unwrap_or_else(|error| panic!("save {name}: {error}"));
        assert_eq!(saved.name, name);
        assert_eq!(saved.choice, value);
    }

    let result = BehaviourActionResult {
        window: cmd_osl_open_window_with_behaviour(&state).expect("open-window command"),
        movement: cmd_osl_apply_movement_behaviour(&state).expect("move-window command"),
        tray: cmd_osl_show_tray_with_behaviour(&state).expect("show-tray command"),
        sound: cmd_osl_play_sound_with_behaviour(&state).expect("play-sound command"),
    };
    fs::write(
        &result_path,
        serde_json::to_vec_pretty(&result).expect("serialize action result"),
    )
    .expect("write action result file");

    ipc::main_password::ensure_device_bound_fallback_file_storage_key(dir.path())
        .expect("configure the temporary preference file key");
    let saved_preferences = load_app_preferences(&prefs_path);
    let result_from_file: BehaviourActionResult = serde_json::from_slice(
        &fs::read(&result_path).expect("read action result file"),
    )
    .expect("parse action result file");
    assert_result_matches_saved(&saved_preferences, &result_from_file)
        .expect("action result must match every saved choice");

    let altered_preferences_path = dir.path().join("app_preferences-altered-copy.json");
    fs::copy(&prefs_path, &altered_preferences_path).expect("copy saved preferences for break check");
    let mut altered_preferences = load_app_preferences(&altered_preferences_path);
    altered_preferences
        .behaviour_choices
        .insert("sound".to_owned(), "different-sound".to_owned());
    write_app_preferences(&altered_preferences_path, &altered_preferences)
        .expect("write one changed saved value in throwaway copy");

    let altered_result = assert_result_matches_saved(
        &load_app_preferences(&altered_preferences_path),
        &result_from_file,
    )
    .expect_err("a changed saved value in the throwaway copy must fail the check");
    assert!(altered_result.contains("behaviour result mismatch for sound"));

    println!(
        "TASK0778 result_file={} open={} position={} remember_place={} movement={} tray_picture={} sound={} mute={} quiet_hours={} tampered_copy_rejected=true error={}",
        result_path.display(),
        result_from_file.window.action,
        result_from_file.window.position,
        result_from_file.window.remember_place,
        result_from_file.movement.movement,
        result_from_file.tray.tray_picture,
        result_from_file.sound.sound,
        result_from_file.sound.mute,
        result_from_file.sound.quiet_hours,
        altered_result,
    );
}
