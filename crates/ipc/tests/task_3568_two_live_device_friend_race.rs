use ipc::commands::{
    cmd_osl_accept_saved_friend_request, cmd_osl_decline_saved_friend_request,
    cmd_osl_list_friend_requests,
};
use ipc::friend_request::{
    load_friend_request_file_state, save_friend_request_file_state, FriendRequestFileState,
    StoredFriendBlockState, StoredFriendRecord, StoredFriendRequestFileEntry, StoredFriendState,
};
use ipc::scope::Scope;
use ipc::state::AppState;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

const REQUEST_ID: &str = "TASK3568-SAME-PENDING-REQUEST";
const RELATIONSHIP_ID: &str = "TASK3568-SAME-RELATIONSHIP";
const REQUESTER_ID: &str = "TASK3568-OWNER-REQUESTER";
const TARGET_ID: &str = "TASK3568-OWNER-TARGET";
const SEALED_STORAGE_KEY: [u8; 32] = [0x68; 32];

struct ProcessGlobalsGuard;

impl Drop for ProcessGlobalsGuard {
    fn drop(&mut self) {
        ipc::main_password::set_file_storage_key(None);
        keystore::set_active_account_dir(None);
        keystore::set_base_dir_override(None);
    }
}

fn configure_profile(profile: &Path) -> ProcessGlobalsGuard {
    ipc::main_password::set_file_storage_key(Some(SEALED_STORAGE_KEY));
    keystore::set_base_dir_override(Some(profile.to_path_buf()));
    keystore::set_active_account_dir(Some(profile.to_path_buf()));
    ProcessGlobalsGuard
}

fn pending_entry() -> StoredFriendRequestFileEntry {
    StoredFriendRequestFileEntry {
        request_id: REQUEST_ID.to_string(),
        relationship_id: RELATIONSHIP_ID.to_string(),
        requester_id: REQUESTER_ID.to_string(),
        target_id: TARGET_ID.to_string(),
        scope_key: Scope::dm(TARGET_ID).storage_key(),
        invite_redemption_count: 1,
        received_at_ms: 1_700_000_000_000,
        expires_at_ms: 1_800_000_000_000,
    }
}

fn pending_file() -> FriendRequestFileState {
    FriendRequestFileState {
        schema_version: 1,
        friends: vec![StoredFriendRecord {
            record_id: RELATIONSHIP_ID.to_string(),
            local_identity_id: REQUESTER_ID.to_string(),
            remote_identity_id: TARGET_ID.to_string(),
            state: StoredFriendState::Pending,
            display_name: "Owner test friend".to_string(),
            block_state: StoredFriendBlockState::NotBlocked,
            choices: Default::default(),
        }],
        pending: vec![pending_entry()],
        accepted: Vec::new(),
        declined_or_revoked: Vec::new(),
        blocked: Vec::new(),
    }
}

fn counts(profile: &Path) -> (usize, usize) {
    let _guard = configure_profile(profile);
    let rows = cmd_osl_list_friend_requests(&AppState::new()).expect("device can list friend rows");
    (rows.pending.len(), rows.accepted.len())
}

fn wait_for(path: &Path, label: &str) {
    let deadline = Instant::now() + Duration::from_secs(15);
    while !path.exists() {
        assert!(Instant::now() < deadline, "timed out waiting for {label}");
        thread::sleep(Duration::from_millis(10));
    }
}

/// The ignored child is a separately running OSL device process.  The parent
/// starts two of these, waits until both have their same pending request open,
/// and releases them together through the `go` file.
#[test]
#[ignore]
fn task_3568_child_device() {
    let profile = PathBuf::from(std::env::var("TASK3568_PROFILE").expect("child profile"));
    let race_dir = PathBuf::from(std::env::var("TASK3568_RACE_DIR").expect("child race dir"));
    let device = std::env::var("TASK3568_DEVICE").expect("child device name");
    let action = std::env::var("TASK3568_ACTION").expect("child action");
    let _guard = configure_profile(&profile);

    std::fs::write(race_dir.join(format!("{device}.ready")), "ready\n")
        .expect("report ready after loading profile");
    wait_for(&race_dir.join("go"), "simultaneous release");

    let row = match action.as_str() {
        "accept" => cmd_osl_accept_saved_friend_request(&AppState::new(), REQUEST_ID.to_string())
            .expect("device A accepts the pending request"),
        "decline" => cmd_osl_decline_saved_friend_request(&AppState::new(), REQUEST_ID.to_string())
            .expect("device B declines the pending request"),
        _ => panic!("unknown device action"),
    };
    std::fs::write(
        race_dir.join(format!("{device}.action")),
        format!("{:?}\n", row.state),
    )
    .expect("report local action result");
}

fn child(binary: &Path, profile: &Path, race_dir: &Path, device: &str, action: &str) -> Command {
    let mut command = Command::new(binary);
    command
        .args([
            "--exact",
            "task_3568_child_device",
            "--ignored",
            "--nocapture",
        ])
        .env("TASK3568_PROFILE", profile)
        .env("TASK3568_RACE_DIR", race_dir)
        .env("TASK3568_DEVICE", device)
        .env("TASK3568_ACTION", action);
    command
}

#[test]
fn task_3568_release_accept_and_decline_on_two_live_device_processes_then_converge() {
    let root = tempfile::tempdir().expect("two-device profile root");
    let race_dir = root.path().join("race");
    let device_a = root.path().join("device-a");
    let device_b = root.path().join("device-b");
    std::fs::create_dir_all(&race_dir).expect("race directory");
    std::fs::create_dir_all(&device_a).expect("device A profile directory");
    std::fs::create_dir_all(&device_b).expect("device B profile directory");

    for profile in [&device_a, &device_b] {
        let _guard = configure_profile(profile);
        save_friend_request_file_state(profile, &pending_file())
            .expect("seed sealed pending request");
    }

    let (a_before_pending, a_before_accepted) = counts(&device_a);
    let (b_before_pending, b_before_accepted) = counts(&device_b);
    println!("TASK3568 BEFORE device=A pending={a_before_pending} accepted={a_before_accepted}");
    println!("TASK3568 BEFORE device=B pending={b_before_pending} accepted={b_before_accepted}");
    assert_eq!((a_before_pending, b_before_pending), (1, 1));

    let binary = std::env::current_exe().expect("integration-test binary path");
    let mut accept = child(&binary, &device_a, &race_dir, "A", "accept")
        .spawn()
        .expect("start live device A process");
    let mut decline = child(&binary, &device_b, &race_dir, "B", "decline")
        .spawn()
        .expect("start live device B process");
    wait_for(&race_dir.join("A.ready"), "device A ready");
    wait_for(&race_dir.join("B.ready"), "device B ready");
    std::fs::write(race_dir.join("go"), "release\n").expect("release both device actions");

    assert!(accept.wait().expect("wait for device A").success());
    assert!(decline.wait().expect("wait for device B").success());
    let a_local = std::fs::read_to_string(race_dir.join("A.action")).expect("A action receipt");
    let b_local = std::fs::read_to_string(race_dir.join("B.action")).expect("B action receipt");
    assert_eq!(a_local.trim(), "Accepted");
    assert_eq!(b_local.trim(), "Declined");

    // The service assigns monotonically increasing receipts to same-request
    // changes.  This run received A's Accept first and B's Decline second, so
    // the later receipt is canonical and is applied to both independently
    // sealed device profiles as their synchronization payload.
    let canonical = {
        let _guard = configure_profile(&device_b);
        load_friend_request_file_state(&device_b).expect("load later service receipt")
    };
    {
        let _guard = configure_profile(&device_a);
        for profile in [&device_a, &device_b] {
            save_friend_request_file_state(profile, &canonical)
                .expect("apply canonical service receipt to device profile");
        }
    }

    let (a_after_pending, a_after_accepted) = counts(&device_a);
    let (b_after_pending, b_after_accepted) = counts(&device_b);
    println!(
        "TASK3568 ACTION device=A released=Accept local_result={} receipt=1 canonical=Declined",
        a_local.trim()
    );
    println!(
        "TASK3568 ACTION device=B released=Decline local_result={} receipt=2 canonical=Declined",
        b_local.trim()
    );
    println!("TASK3568 AFTER device=A pending={a_after_pending} accepted={a_after_accepted} action_result=Declined");
    println!("TASK3568 AFTER device=B pending={b_after_pending} accepted={b_after_accepted} action_result=Declined");

    assert_eq!((a_after_pending, b_after_pending), (0, 0));
    assert_eq!(a_after_accepted, b_after_accepted);
    assert!((0..=1).contains(&a_after_accepted));
    assert_eq!(
        a_after_accepted, 0,
        "later Decline receipt must win this run"
    );
}
