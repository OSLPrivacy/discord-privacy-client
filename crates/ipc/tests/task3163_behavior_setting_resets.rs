use ipc::commands::{
    cmd_osl_get_ask_before_irreversible_actions_choice, cmd_osl_get_language_choice,
    cmd_osl_get_start_with_windows_choice, cmd_osl_read_alert_mode_choice,
    cmd_osl_read_idle_lock_time_choice, cmd_osl_reset_alert_mode_choice,
    cmd_osl_reset_ask_before_irreversible_actions_choice, cmd_osl_reset_follow_active_app_choice,
    cmd_osl_reset_idle_lock_time_choice, cmd_osl_reset_language_choice,
    cmd_osl_reset_start_with_windows_choice, cmd_osl_save_alert_mode_choice,
    cmd_osl_save_idle_lock_time_choice, cmd_osl_save_language_choice,
    cmd_osl_save_start_with_windows_choice, cmd_osl_set_ask_before_irreversible_actions_choice,
    cmd_osl_set_follow_active_app_choice,
};
use ipc::state::AppState;
use std::sync::Mutex;

static KEY_LOCK: Mutex<()> = Mutex::new(());

struct FileStorageKeyGuard;

impl Drop for FileStorageKeyGuard {
    fn drop(&mut self) {
        ipc::main_password::set_file_storage_key(None);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct BehaviorSnapshot {
    start_with_windows: String,
    idle_lock_time_choice: String,
    ask_before_irreversible_actions: String,
    alert_mode_choice: String,
    language: String,
    follow_active_app_choice: String,
}

impl BehaviorSnapshot {
    fn field(&self, name: &str) -> &str {
        match name {
            "start_with_windows" => &self.start_with_windows,
            "idle_lock_time_choice" => &self.idle_lock_time_choice,
            "ask_before_irreversible_actions" => &self.ask_before_irreversible_actions,
            "alert_mode_choice" => &self.alert_mode_choice,
            "language" => &self.language,
            "follow_active_app_choice" => &self.follow_active_app_choice,
            other => panic!("unknown behavior field {other}"),
        }
    }

    fn unchanged_count_except(&self, after: &Self, reset_field: &str) -> usize {
        BEHAVIOR_FIELDS
            .iter()
            .filter(|field| **field != reset_field && self.field(field) == after.field(field))
            .count()
    }

    fn unchanged_fields_except(&self, after: &Self, reset_field: &str) -> String {
        BEHAVIOR_FIELDS
            .iter()
            .copied()
            .filter(|field| *field != reset_field && self.field(field) == after.field(field))
            .collect::<Vec<_>>()
            .join(",")
    }
}

const BEHAVIOR_FIELDS: [&str; 6] = [
    "start_with_windows",
    "idle_lock_time_choice",
    "ask_before_irreversible_actions",
    "alert_mode_choice",
    "language",
    "follow_active_app_choice",
];

fn idle_value(dto: ipc::commands::IdleLockTimeChoiceDto) -> String {
    format!(
        "{}:{}:{}",
        dto.choice,
        dto.seconds
            .map(|seconds| seconds.to_string())
            .unwrap_or_else(|| "none".to_string()),
        dto.label
    )
}

fn read_behavior_snapshot(state: &AppState) -> BehaviorSnapshot {
    BehaviorSnapshot {
        start_with_windows: cmd_osl_get_start_with_windows_choice(state).unwrap(),
        idle_lock_time_choice: idle_value(cmd_osl_read_idle_lock_time_choice(state).unwrap()),
        ask_before_irreversible_actions: cmd_osl_get_ask_before_irreversible_actions_choice(state)
            .unwrap()
            .as_str()
            .to_string(),
        alert_mode_choice: cmd_osl_read_alert_mode_choice(state).unwrap().mode,
        language: cmd_osl_get_language_choice(state).unwrap(),
        follow_active_app_choice: ipc::commands::cmd_osl_get_follow_active_app_choice(state)
            .unwrap(),
    }
}

fn seed_non_default_preferences(state: &AppState, dir: &std::path::Path) {
    let dir = Some(dir.to_path_buf());
    cmd_osl_save_start_with_windows_choice(state, "on".to_string(), dir.clone()).unwrap();
    cmd_osl_save_idle_lock_time_choice(state, "never".to_string(), dir.clone()).unwrap();
    cmd_osl_set_ask_before_irreversible_actions_choice(state, "off".to_string(), dir.clone())
        .unwrap();
    cmd_osl_save_alert_mode_choice(state, "silent".to_string(), dir.clone()).unwrap();
    cmd_osl_save_language_choice(state, "es".to_string(), dir.clone()).unwrap();
    cmd_osl_set_follow_active_app_choice(state, "on", dir).unwrap();
}

#[test]
fn task3163_each_behavior_setting_reset_only_restores_its_starting_value() {
    let _lock = KEY_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    ipc::main_password::set_file_storage_key(Some([0x63u8; 32]));
    let _key_guard = FileStorageKeyGuard;

    let cases: [(&str, &str, &str, fn(&AppState, &std::path::Path) -> String); 6] = [
        (
            "start_with_windows",
            "cmd_osl_reset_start_with_windows_choice",
            "off",
            |state, dir| {
                cmd_osl_reset_start_with_windows_choice(state, Some(dir.to_path_buf())).unwrap()
            },
        ),
        (
            "idle_lock_time_choice",
            "cmd_osl_reset_idle_lock_time_choice",
            "seconds:900:900 seconds",
            |state, dir| {
                idle_value(
                    cmd_osl_reset_idle_lock_time_choice(state, Some(dir.to_path_buf())).unwrap(),
                )
            },
        ),
        (
            "ask_before_irreversible_actions",
            "cmd_osl_reset_ask_before_irreversible_actions_choice",
            "on",
            |state, dir| {
                cmd_osl_reset_ask_before_irreversible_actions_choice(state, Some(dir.to_path_buf()))
                    .unwrap()
                    .as_str()
                    .to_string()
            },
        ),
        (
            "alert_mode_choice",
            "cmd_osl_reset_alert_mode_choice",
            "normal",
            |state, dir| {
                cmd_osl_reset_alert_mode_choice(state, Some(dir.to_path_buf()))
                    .unwrap()
                    .mode
            },
        ),
        (
            "language",
            "cmd_osl_reset_language_choice",
            "en",
            |state, dir| cmd_osl_reset_language_choice(state, Some(dir.to_path_buf())).unwrap(),
        ),
        (
            "follow_active_app_choice",
            "cmd_osl_reset_follow_active_app_choice",
            "off",
            |state, dir| {
                cmd_osl_reset_follow_active_app_choice(state, Some(dir.to_path_buf())).unwrap()
            },
        ),
    ];

    for (field, command, starting_value, reset) in cases {
        let dir = tempfile::tempdir().unwrap();
        let state = AppState::new();
        seed_non_default_preferences(&state, dir.path());
        let before = read_behavior_snapshot(&state);

        let reset_return = reset(&state, dir.path());
        let after = read_behavior_snapshot(&state);
        let unchanged_count = before.unchanged_count_except(&after, field);
        let unchanged_fields = before.unchanged_fields_except(&after, field);
        let read_value = after.field(field);

        println!(
            "TASK3163 reset={field} direct_command={command} reset_return={reset_return} read_{field}={read_value} starting_value={starting_value} unchanged_count={unchanged_count} unchanged_settings={unchanged_fields}"
        );

        assert_eq!(reset_return, starting_value);
        assert_eq!(read_value, starting_value);
        assert_eq!(unchanged_count, 5);
    }
}
