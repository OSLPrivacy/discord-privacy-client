use ipc::commands::cmd_osl_save_idle_lock_time_choice;
use ipc::session_lock;
use ipc::state::AppState;
use std::sync::Mutex;
use std::time::{Duration, Instant};

static PROCESS_GLOBALS: Mutex<()> = Mutex::new(());

struct ResetGlobals;

impl Drop for ResetGlobals {
    fn drop(&mut self) {
        ipc::main_password::set_file_storage_key(None);
        session_lock::disarm_idle_lock();
    }
}

fn unlocked_state_with_choice(choice: &str) -> (AppState, String, Option<u64>) {
    ipc::main_password::set_file_storage_key(Some([0x52; 32]));
    let state = AppState::new();
    state.install_identity(keystore::generate_identity(format!("task-3152-{choice}")));
    let saved = cmd_osl_save_idle_lock_time_choice(&state, choice.to_string(), None)
        .expect("save idle lock time choice");
    (state, saved.label, saved.seconds)
}

fn osl_is_unlocked(state: &AppState) -> bool {
    session_lock::session_holds_live_secrets(state)
        && ipc::main_password::get_file_storage_key().is_some()
}

#[test]
fn task_3152_one_minute_locks_at_70_seconds_and_never_stays_unlocked_at_5_minutes() {
    let _guard = PROCESS_GLOBALS
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let _reset = ResetGlobals;

    let (one_minute_state, one_minute_label, one_minute_seconds) =
        unlocked_state_with_choice("one minute");
    let t0 = Instant::now();
    session_lock::arm_idle_lock_at(t0);

    let locked_at_50 =
        session_lock::run_idle_session_lock_at(&one_minute_state, t0 + Duration::from_secs(50));
    let unlocked_at_50 = !locked_at_50 && osl_is_unlocked(&one_minute_state);
    println!(
        "TASK3152 chosen_idle_lock_time={} chosen_seconds={}",
        one_minute_label,
        one_minute_seconds.expect("one minute seconds")
    );
    println!("TASK3152 one_minute.idle_seconds=50 OSL_unlocked={unlocked_at_50}");
    assert!(
        unlocked_at_50,
        "OSL must still be unlocked at 50 idle seconds"
    );

    let locked_at_70 =
        session_lock::run_idle_session_lock_at(&one_minute_state, t0 + Duration::from_secs(70));
    let locked_state_at_70 = locked_at_70 && !osl_is_unlocked(&one_minute_state);
    println!("TASK3152 one_minute.idle_seconds=70 OSL_locked={locked_state_at_70}");
    assert!(locked_state_at_70, "OSL must lock at 70 idle seconds");

    ipc::main_password::set_file_storage_key(None);
    session_lock::disarm_idle_lock();

    let (never_state, never_label, never_seconds) = unlocked_state_with_choice("never");
    let never_t0 = Instant::now();
    session_lock::arm_idle_lock_at(never_t0);
    let locked_at_5_minutes =
        session_lock::run_idle_session_lock_at(&never_state, never_t0 + Duration::from_secs(300));
    let unlocked_at_5_minutes = !locked_at_5_minutes && osl_is_unlocked(&never_state);
    println!(
        "TASK3152 chosen_idle_lock_time={} chosen_seconds={}",
        never_label,
        never_seconds
            .map(|seconds| seconds.to_string())
            .unwrap_or_else(|| "none".to_string())
    );
    println!("TASK3152 never.idle_seconds=300 OSL_unlocked={unlocked_at_5_minutes}");
    assert!(
        unlocked_at_5_minutes,
        "OSL must still be unlocked after 5 idle minutes when never is chosen"
    );
}
