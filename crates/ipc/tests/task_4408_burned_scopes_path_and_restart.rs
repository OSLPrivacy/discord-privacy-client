use ipc::burned_scopes_file::{self, BURNED_SCOPES_FILE_NAME};
use ipc::commands::{cmd_osl_list_burned_scopes, cmd_osl_mark_scope_burned};
use ipc::main_password::{has_enc_magic, set_file_storage_key};
use ipc::state::AppState;
use ipc::state_reload::reload_encrypted_state_after_unlock;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

static KEY_LOCK: Mutex<()> = Mutex::new(());

struct ConfigDirGuard;

impl Drop for ConfigDirGuard {
    fn drop(&mut self) {
        burned_scopes_file::reset_burn_state_unreadable_for_tests();
        set_file_storage_key(None);
        keystore::set_active_account_dir(None);
        keystore::set_base_dir_override(None);
    }
}

fn use_saved_things_dir(dir: &Path) -> ConfigDirGuard {
    burned_scopes_file::reset_burn_state_unreadable_for_tests();
    keystore::set_active_account_dir(Some(dir.to_path_buf()));
    keystore::set_base_dir_override(Some(
        dir.parent().expect("account dir has parent").to_path_buf(),
    ));
    set_file_storage_key(Some([0x44; 32]));
    ConfigDirGuard
}

fn find_burn_ledgers(root: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(root).expect("scan saved-things root") {
        let entry = entry.expect("read dir entry");
        let path = entry.path();
        if path.is_dir() {
            find_burn_ledgers(&path, out);
        } else if path.file_name().and_then(|name| name.to_str()) == Some(BURNED_SCOPES_FILE_NAME) {
            out.push(path);
        }
    }
}

fn byte_occurrences(haystack: &[u8], needle: &str) -> usize {
    let needle = needle.as_bytes();
    if needle.is_empty() || haystack.len() < needle.len() {
        return 0;
    }
    haystack
        .windows(needle.len())
        .filter(|window| *window == needle)
        .count()
}

#[test]
fn task_4408_sealed_burn_list_lives_in_saved_things_and_survives_restart() {
    let _guard = KEY_LOCK.lock().unwrap_or_else(|error| error.into_inner());

    let root = tempfile::tempdir().expect("temp saved-things root");
    let account_dir = root.path().join("accounts").join("task-4408-account");
    fs::create_dir_all(&account_dir).expect("create account saved-things dir");
    let _config = use_saved_things_dir(&account_dir);

    let chosen_config_dir = keystore::osl_config_dir().expect("resolve app saved-things dir");
    let chosen_path = burned_scopes_file::path_in_config_dir(&chosen_config_dir);
    assert_eq!(chosen_config_dir, account_dir);

    let state = AppState::new();
    let burns = [
        (
            "dm",
            "Readable-Alice-4408",
            None,
            None,
            vec!["Readable-Message-A-4408".to_string()],
        ),
        (
            "gc",
            "Readable-Group-4408",
            None,
            Some("Readable-Channel-B-4408".to_string()),
            vec!["Readable-Message-B-4408".to_string()],
        ),
        (
            "server_channel",
            "Readable-Server-4408:Readable-Channel-C-4408",
            Some("Readable-Server-4408".to_string()),
            Some("Readable-Channel-C-4408".to_string()),
            vec!["Readable-Message-C-4408".to_string()],
        ),
    ];

    for (kind, scope_id, server_id, channel_id, message_ids) in burns {
        cmd_osl_mark_scope_burned(
            &state,
            kind.to_string(),
            scope_id.to_string(),
            server_id,
            channel_id,
            message_ids,
        )
        .expect("queue burn in durable ledger");
    }

    let queued_before_restart = cmd_osl_list_burned_scopes(&state)
        .expect("list burns before restart")
        .len();
    assert_eq!(queued_before_restart, 3);

    let mut ledger_paths = Vec::new();
    find_burn_ledgers(root.path(), &mut ledger_paths);
    ledger_paths.sort();
    let only_written_path = ledger_paths
        .first()
        .expect("burn ledger was written")
        .to_path_buf();
    assert_eq!(ledger_paths, vec![chosen_path.clone()]);

    let sealed = fs::read(&chosen_path).expect("read sealed burn ledger");
    assert!(has_enc_magic(&sealed));
    let readable_names = [
        "Readable-Alice-4408",
        "Readable-Group-4408",
        "Readable-Server-4408",
        "Readable-Channel-B-4408",
        "Readable-Channel-C-4408",
        "Readable-Message-A-4408",
        "Readable-Message-B-4408",
        "Readable-Message-C-4408",
    ];
    let readable_name_hits: usize = readable_names
        .iter()
        .map(|name| byte_occurrences(&sealed, name))
        .sum();
    assert_eq!(readable_name_hits, 0);

    let restarted = AppState::new();
    let report = reload_encrypted_state_after_unlock(&restarted, &chosen_config_dir)
        .expect("reload encrypted state after restart");
    assert!(report.burned_scopes_loaded);
    assert_eq!(report.burned_scopes_count, 3);
    assert!(report.errors.is_empty());

    let after_restart = cmd_osl_list_burned_scopes(&restarted).expect("list burns after restart");
    assert_eq!(after_restart.len(), 3);

    println!("TASK4408_SAVED_THINGS_DIR={}", chosen_config_dir.display());
    println!("TASK4408_CHOSEN_BURN_LIST_PATH={}", chosen_path.display());
    println!("TASK4408_ONLY_WRITTEN_PATH={}", only_written_path.display());
    println!("TASK4408_BURN_LIST_FILES_WRITTEN={}", ledger_paths.len());
    println!("TASK4408_SEALED_MAGIC={}", has_enc_magic(&sealed));
    println!("TASK4408_READABLE_NAME_HITS={readable_name_hits}");
    println!("TASK4408_QUEUED_BEFORE_RESTART={queued_before_restart}");
    println!("TASK4408_AFTER_RESTART_COUNT={}", after_restart.len());
}
