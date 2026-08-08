use ipc::burned_scopes_file::{
    self, burn_state_unreadable, load_burned_scopes, BurnedScopeEntry, BurnedScopesFile,
};
use ipc::commands::{
    cmd_osl_list_burned_scopes, cmd_osl_take_last_persist_error, cmd_osl_unburn_scope,
};
use ipc::main_password::set_file_storage_key;
use ipc::state::AppState;
use std::fs;
use std::sync::Mutex;

static PROCESS_GLOBALS: Mutex<()> = Mutex::new(());

const LOST_BURN: &str = "TASK4409-LOST-BURN-SCOPE";
const KEPT_BURN: &str = "TASK4409-KEPT-BURN-SCOPE";

struct ConfigDirGuard;

impl Drop for ConfigDirGuard {
    fn drop(&mut self) {
        burned_scopes_file::reset_burn_state_unreadable_for_tests();
        set_file_storage_key(None);
        keystore::set_active_account_dir(None);
        keystore::set_base_dir_override(None);
    }
}

fn use_saved_things_dir(dir: &std::path::Path) -> ConfigDirGuard {
    burned_scopes_file::reset_burn_state_unreadable_for_tests();
    keystore::set_active_account_dir(Some(dir.to_path_buf()));
    keystore::set_base_dir_override(Some(
        dir.parent().expect("account dir has parent").to_path_buf(),
    ));
    set_file_storage_key(None);
    ConfigDirGuard
}

fn burn(scope_id: &str, message_id: &str) -> BurnedScopeEntry {
    BurnedScopeEntry {
        scope_kind: "dm".to_string(),
        scope_id: scope_id.to_string(),
        server_id: None,
        channel_id: None,
        burned_at: 1_753_000_000,
        burned_message_ids: vec![message_id.to_string()],
    }
}

#[test]
fn task_4409_refused_emptying_keeps_the_named_burn_in_memory() {
    let _guard = PROCESS_GLOBALS
        .lock()
        .unwrap_or_else(|error| error.into_inner());

    let root = tempfile::tempdir().expect("temp saved-things root");
    let account_dir = root.path().join("accounts").join("task-4409-account");
    fs::create_dir_all(&account_dir).expect("create account saved-things dir");
    let _config = use_saved_things_dir(&account_dir);

    let ledger_path = burned_scopes_file::path_in_config_dir(&account_dir);
    fs::write(&ledger_path, b"not a burned scopes file").expect("poison burn ledger");
    let unreadable = load_burned_scopes(&ledger_path);
    assert!(unreadable.scopes.is_empty());
    assert!(
        burn_state_unreadable(),
        "fixture must make the real store refuse burned_scopes writes"
    );

    let state = AppState::new();
    {
        let mut ledger = state
            .burned_scopes
            .lock()
            .expect("burned_scopes mutex poisoned");
        *ledger = BurnedScopesFile {
            version: 1,
            scopes: vec![
                burn(LOST_BURN, "TASK4409-LOST-MESSAGE"),
                burn(KEPT_BURN, "TASK4409-KEPT-MESSAGE"),
            ],
        };
    }

    let before = cmd_osl_list_burned_scopes(&state).expect("list before refused unburn");
    let refusal = cmd_osl_unburn_scope(&state, "dm".to_string(), LOST_BURN.to_string())
        .expect_err("unburn must fail when the burn store refuses the write");
    let after = cmd_osl_list_burned_scopes(&state).expect("list after refused unburn");

    let before_scopes: Vec<_> = before.iter().map(|entry| entry.scope_id.as_str()).collect();
    let after_scopes: Vec<_> = after.iter().map(|entry| entry.scope_id.as_str()).collect();
    let lost_still_present = after_scopes.contains(&LOST_BURN);
    let kept_still_present = after_scopes.contains(&KEPT_BURN);

    assert!(
        refusal.contains("refusing to write burned_scopes.json"),
        "TASK4409_STORE_REFUSAL_UNEXPECTED={refusal}"
    );
    assert!(
        lost_still_present,
        "TASK4409_LOST_BURN={LOST_BURN} missing after refused store write; before={before_scopes:?} after={after_scopes:?} refusal={refusal}"
    );
    assert!(
        kept_still_present,
        "TASK4409_KEPT_BURN={KEPT_BURN} missing after refused store write; before={before_scopes:?} after={after_scopes:?} refusal={refusal}"
    );
    assert_eq!(
        before_scopes, after_scopes,
        "TASK4409_BURN_LIST_CHANGED_AFTER_REFUSED_STORE_WRITE before={before_scopes:?} after={after_scopes:?}"
    );

    let persist_error = cmd_osl_take_last_persist_error(&state)
        .expect("refused write should be surfaced to the UI");
    assert!(
        persist_error.contains("burned_scopes.json"),
        "TASK4409_PERSIST_ERROR_UNEXPECTED={persist_error}"
    );

    println!("TASK4409_STORE_REFUSAL={refusal}");
    println!("TASK4409_PERSIST_ERROR={persist_error}");
    println!("TASK4409_LOST_BURN_PRESENT_AFTER_REFUSAL={lost_still_present} name={LOST_BURN}");
    println!("TASK4409_KEPT_BURN_PRESENT_AFTER_REFUSAL={kept_still_present} name={KEPT_BURN}");
    println!("TASK4409_BEFORE_REFUSAL_COUNT={}", before_scopes.len());
    println!("TASK4409_AFTER_REFUSAL_COUNT={}", after_scopes.len());
}
