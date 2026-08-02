#![cfg(all(windows, feature = "core"))]

//! T15-T22: a refused key-server route must not prevent a first local account
//! from being created, persisted, or unlocked after a fresh process starts.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use osl_privacy_hub::core_bridge::{self, HubCoreState};
use osl_privacy_hub::password_lifecycle;

const CHILD_ENV: &str = "OSL_HUB_OFFLINE_LIFECYCLE_CHILD";
const ROOT_ENV: &str = "OSL_HUB_OFFLINE_LIFECYCLE_ROOT";
const PASSWORD: &str = "aB3!z9-offline-passphrase";

#[test]
fn account_creation_and_unlock_succeed_when_keyserver_refuses_connections() {
    let root = isolated_root();
    std::fs::create_dir_all(&root).expect("create isolated lifecycle root");

    run_child(&root, "create");
    assert!(root.join("created.ok").is_file());
    run_child(&root, "unlock-after-relaunch");
    assert!(root.join("unlocked.ok").is_file());

    std::fs::remove_dir_all(&root).expect("remove isolated lifecycle root");
}

#[test]
fn offline_lifecycle_child() {
    let Ok(phase) = std::env::var(CHILD_ENV) else {
        return;
    };
    let root = PathBuf::from(std::env::var_os(ROOT_ENV).expect("isolated root supplied"));
    assert_isolated_root(&root);

    keystore::set_active_account_dir(None);
    keystore::set_base_dir_override(Some(root.join("osl-core")));
    // Nothing listens on this local port. If either lifecycle operation makes
    // an outbound key-server request, it is refused immediately rather than
    // being hidden by a reachable server or an internet connection.
    std::fs::create_dir_all(root.join("osl-core")).expect("create isolated core directory");
    std::fs::write(
        root.join("osl-core/keyserver.json"),
        br#"{"base_url":"http://127.0.0.1:1","user_id":"offline-lifecycle-probe"}"#,
    )
    .expect("write refused keyserver route");

    match phase.as_str() {
        "create" => create_account_offline(&root),
        "unlock-after-relaunch" => unlock_after_relaunch_offline(&root),
        _ => panic!("unknown offline lifecycle phase"),
    }
}

fn run_child(root: &Path, phase: &str) {
    let status = Command::new(std::env::current_exe().expect("current test executable"))
        .args(["--exact", "offline_lifecycle_child", "--nocapture"])
        .env(CHILD_ENV, phase)
        .env(ROOT_ENV, root)
        .status()
        .expect("launch isolated lifecycle child");
    assert!(status.success(), "offline lifecycle phase failed: {phase}");
}

fn create_account_offline(root: &Path) {
    let state = HubCoreState::bootstrap_from_disk();
    let created = password_lifecycle::create_native_identity(
        &state,
        Some(password_lifecycle::IdentityCreationOwnerAuthorization::ExplicitOwnerSignoff),
    )
    .expect("identity creation succeeds with refused keyserver route");
    assert_eq!(
        state.osl.cloud_registration_state(),
        ipc::state::CloudRegistrationState::NotAttempted,
        "identity creation must not register over the network"
    );

    let setup = password_lifecycle::setup_main_password(&state, PASSWORD.to_owned())
        .expect("password setup succeeds with refused keyserver route");
    assert!(setup.encrypted_state_reload_complete);
    assert_eq!(
        state.osl.cloud_registration_state(),
        ipc::state::CloudRegistrationState::NotAttempted,
        "password setup must complete before any deferred registration worker"
    );
    std::fs::write(root.join("expected-user-id"), created.user_id).expect("write public id marker");
    ipc::main_password::set_file_storage_key(None);
    std::fs::write(root.join("created.ok"), b"ok").expect("write creation marker");
}

fn unlock_after_relaunch_offline(root: &Path) {
    let state = HubCoreState::bootstrap_from_disk();
    let before = password_lifecycle::readiness(&state);
    assert_eq!(before.access_state, "passwordRequired");
    assert!(!before.unlocked);

    let readiness = core_bridge::unlock_main_password(&state, PASSWORD.to_owned())
        .expect("unlock succeeds with refused keyserver route");
    assert!(readiness.unlocked);
    assert_eq!(
        readiness.active_osl_user_id.as_deref(),
        Some(
            std::fs::read_to_string(root.join("expected-user-id"))
                .expect("public id marker exists")
                .as_str()
        )
    );
    assert_eq!(
        state.osl.cloud_registration_state(),
        ipc::state::CloudRegistrationState::NotAttempted,
        "unlock must not register over the network"
    );
    ipc::main_password::set_file_storage_key(None);
    std::fs::write(root.join("unlocked.ok"), b"ok").expect("write unlock marker");
}

fn isolated_root() -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "osl-offline-lifecycle-{}-{nonce}",
        std::process::id()
    ))
}

fn assert_isolated_root(root: &Path) {
    assert!(root.is_absolute(), "test root must be absolute");
    assert!(
        root.file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("osl-offline-lifecycle-")),
        "refusing to use a non-isolated root: {}",
        root.display()
    );
}
