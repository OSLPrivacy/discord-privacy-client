use ipc::commands::{cmd_osl_note_scope_membership, cmd_osl_restore_scope_membership_local_copies};
use ipc::membership::{
    load_previous_scope_membership_from_path, load_scope_membership_from_path,
    previous_scope_membership_path, write_scope_membership,
};
use ipc::scope::{ScopeInput, ScopeKind};
use ipc::state::AppState;
use std::sync::Mutex;

static CONFIG_LOCK: Mutex<()> = Mutex::new(());

struct GlobalStateGuard;

impl Drop for GlobalStateGuard {
    fn drop(&mut self) {
        keystore::set_active_account_dir(None);
        keystore::set_base_dir_override(None);
        ipc::main_password::set_file_storage_key(None);
    }
}

fn marked_count(membership: &ipc::membership::ScopeMembership, gc: &str) -> usize {
    membership
        .members_for_key(&format!("gc:{gc}"))
        .into_iter()
        .filter(|member| member.starts_with("task3649-marked-"))
        .count()
}

#[test]
fn task3649_corrupt_local_copies_and_recover_safely() {
    let _lock = CONFIG_LOCK.lock().unwrap_or_else(|err| err.into_inner());
    let _guard = GlobalStateGuard;
    let dir = tempfile::tempdir().expect("task3649 config dir");
    keystore::set_active_account_dir(None);
    keystore::set_base_dir_override(Some(dir.path().to_path_buf()));
    ipc::main_password::set_file_storage_key(Some([0x49; 32]));

    let state = AppState::new();
    let gc = "task3649-gc";
    let marked = vec![
        "task3649-marked-1".to_string(),
        "task3649-marked-2".to_string(),
        "task3649-marked-3".to_string(),
    ];
    cmd_osl_note_scope_membership(
        &state,
        ScopeInput {
            kind: ScopeKind::Gc,
            id: gc.to_string(),
            server_id: None,
            channel_id: Some(gc.to_string()),
        },
        marked.clone(),
    )
    .expect("initial command writes live and previous membership copies");
    let mut write_attempt_count = 1usize;

    let live_path = dir.path().join("membership.json");
    let previous_path = previous_scope_membership_path(&live_path);
    let live_before = load_scope_membership_from_path(&live_path).expect("live copy opens");
    let previous_before =
        load_previous_scope_membership_from_path(&live_path).expect("previous copy opens");
    let live_before_count = marked_count(&live_before, gc);
    let previous_before_count = marked_count(&previous_before, gc);
    println!(
        "TASK3649 before_corruption live_marked_count={} previous_marked_count={} marked_items={}",
        live_before_count,
        previous_before_count,
        marked.join(",")
    );
    assert_eq!(live_before_count, 3);
    assert_eq!(previous_before_count, 3);

    std::fs::write(&live_path, b"TASK3649 corrupt live copy").expect("corrupt live only");
    let after_live_corruption =
        load_scope_membership_from_path(&live_path).expect("previous copy restores live");
    let restored_from_previous_count = marked_count(&after_live_corruption, gc);
    let live_after_heal =
        load_scope_membership_from_path(&live_path).expect("healed live copy opens");
    let live_after_heal_count = marked_count(&live_after_heal, gc);
    println!(
        "TASK3649 live_only_corruption restored_from_previous_count={} healed_live_marked_count={}",
        restored_from_previous_count, live_after_heal_count
    );
    assert_eq!(restored_from_previous_count, 3);
    assert_eq!(live_after_heal_count, 3);

    std::fs::write(&live_path, b"TASK3649 corrupt live copy again").expect("corrupt live");
    std::fs::write(&previous_path, b"TASK3649 corrupt previous copy").expect("corrupt previous");
    let both_corrupt_error = load_scope_membership_from_path(&live_path)
        .expect_err("both corrupt local copies refuse plainly")
        .to_string();
    println!("TASK3649 both_corrupt_refusal=\"{}\"", both_corrupt_error);
    assert!(
        both_corrupt_error.contains("membership.json local copies are unreadable"),
        "{both_corrupt_error}"
    );

    let snapshot = state
        .scope_membership
        .lock()
        .expect("scope_membership mutex poisoned")
        .clone();
    let refused_write = write_scope_membership(&live_path, &snapshot)
        .expect_err("ordinary write refuses while both local copies are corrupt");
    println!(
        "TASK3649 refused_write=\"{}\" write_attempt_count={}",
        refused_write, write_attempt_count
    );
    assert_eq!(write_attempt_count, 1);
    assert_eq!(refused_write.kind(), std::io::ErrorKind::InvalidData);

    let restore = cmd_osl_restore_scope_membership_local_copies(&state)
        .expect("named restore command rebuilds both local copies");
    write_attempt_count = 1;
    let live_after_restore =
        load_scope_membership_from_path(&live_path).expect("live opens after named restore");
    let previous_after_restore =
        load_previous_scope_membership_from_path(&live_path).expect("previous opens after restore");
    let live_restore_count = marked_count(&live_after_restore, gc);
    let previous_restore_count = marked_count(&previous_after_restore, gc);
    println!(
        "TASK3649 restore_command=cmd_osl_restore_scope_membership_local_copies restored_items={} live_after_restore_count={} previous_after_restore_count={} write_attempt_count={}",
        restore.restored_items, live_restore_count, previous_restore_count, write_attempt_count
    );
    assert_eq!(restore.restored_items, 3);
    assert_eq!(live_restore_count, 3);
    assert_eq!(previous_restore_count, 3);
}
