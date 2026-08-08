//! TASK 3235 / attack 51: replay both an old account backup and the whole
//! app-data tree after a completed saved-password Burn.
//!
//! `TASK3235_FIXTURE` selects the positive-control fixture. The default names
//! three protected messages in a pre-Burn backup; the saved fixture without a
//! backup must make this exact check fail before Burn executes.

use ipc::state::AppState;
use osl_privacy_hub::cleanup::{task_3235_complete_verified_gate_burn, Task3234PowerCutKeyStore};
use osl_privacy_hub::core_bridge::HubCoreState;
use osl_privacy_hub::startup_gate::{verify_password_role, VerifiedGateRole};
use serde::Deserialize;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use store::{MessageStore, StoredMessage};

const DEFAULT_FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/task_3235/with_pre_burn_backup.json"
);
const CHANNEL: &str = "task-3235-protected-channel";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Fixture {
    pre_burn_backup: Option<PreBurnBackup>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PreBurnBackup {
    name: String,
    protected_messages: Vec<String>,
}

fn copy_tree(source: &Path, destination: &Path) {
    fs::create_dir_all(destination).expect("TASK3235 create snapshot destination");
    for entry in fs::read_dir(source).expect("TASK3235 read snapshot source") {
        let entry = entry.expect("TASK3235 read snapshot entry");
        let target = destination.join(entry.file_name());
        if entry
            .file_type()
            .expect("TASK3235 read entry type")
            .is_dir()
        {
            copy_tree(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).expect("TASK3235 copy snapshot file");
        }
    }
}

fn checkpoint(store_dir: &Path) {
    let connection = rusqlite::Connection::open(store_dir.join("messages.sqlite"))
        .expect("TASK3235 open store for checkpoint");
    connection
        .execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
        .expect("TASK3235 checkpoint pre-burn store");
}

fn open_count(
    store_dir: &Path,
    secret: &[u8; 32],
    anchor: Arc<keystore::KeystoreBackedAnchor>,
) -> Result<usize, String> {
    let store = MessageStore::open_anchored(store_dir, secret, anchor)
        .map_err(|error| error.to_string())?;
    store
        .list_by_channel(CHANNEL, 100)
        .map(|messages| messages.len())
        .map_err(|error| error.to_string())
}

fn reset_globals() {
    ipc::main_password::set_file_storage_key(None);
    keystore::set_active_account_dir(None);
    keystore::set_base_dir_override(None);
}

#[test]
fn task_3235_pre_burn_backups_cannot_restore_readable_messages_after_burn() {
    reset_globals();
    let fixture_path = env::var_os("TASK3235_FIXTURE")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_FIXTURE));
    let fixture: Fixture = serde_json::from_slice(
        &fs::read(&fixture_path)
            .unwrap_or_else(|error| panic!("HAZEL-3235 fixture read failed: {error}")),
    )
    .unwrap_or_else(|error| panic!("HAZEL-3235 fixture parse failed: {error}"));
    let pre_burn_backup = fixture.pre_burn_backup.unwrap_or_else(|| {
        panic!(
            "HAZEL-3235 positive-control fixture={} expected pre_burn_backup_count>0 actual=0",
            fixture_path.display()
        )
    });
    assert!(
        !pre_burn_backup.protected_messages.is_empty(),
        "HAZEL-3235 positive-control fixture={} expected pre_burn_backup_count>0 actual=0",
        fixture_path.display()
    );

    let live = tempfile::tempdir().expect("TASK3235 isolated live app root");
    let snapshots = tempfile::tempdir().expect("TASK3235 isolated snapshot root");
    let config = live.path().join("config");
    let local = live.path().join("local");
    let core = config.join("osl-core");
    let store_dir = core.join("store");
    fs::create_dir_all(&local).expect("TASK3235 local app-data root");
    keystore::set_base_dir_override(Some(core.clone()));

    ipc::main_password::set_main_password(&core, "main-pass-3235")
        .expect("TASK3235 save main password");
    ipc::main_password::set_burn_password(&core, "main-pass-3235", "burn-pass-3235")
        .expect("TASK3235 save burn password");

    let identity = keystore::identity_from_entropy([0x35; 16], "task-3235-account".to_string());
    let secret = *identity.x25519_secret.as_bytes();
    let app_state = AppState::new();
    app_state.install_identity(identity);
    let anchor = Arc::new(keystore::KeystoreBackedAnchor::task_3235_in_memory());
    {
        let store = MessageStore::open_anchored(&store_dir, &secret, anchor.clone())
            .expect("TASK3235 enroll anchored protected store");
        for (index, plaintext) in pre_burn_backup.protected_messages.iter().enumerate() {
            store
                .put(&StoredMessage {
                    discord_message_id: format!("task-3235-message-{index}"),
                    channel_id: CHANNEL.to_string(),
                    sender_discord_id: "task-3235-sender".to_string(),
                    sender_osl_user_id: "task-3235-osl-sender".to_string(),
                    plaintext: plaintext.clone(),
                    decrypted_at: 1_780_000_000 + index as i64,
                    reply_parent_id: None,
                    edit_revision: 1,
                    burned: false,
                })
                .expect("TASK3235 write protected message");
        }
    }
    checkpoint(&store_dir);

    // The account-backup snapshot is built from the shipping export manifest.
    // The outer package's phrase AEAD is irrelevant after successful import;
    // 0458 proves that import restores these exact files and raw identity keys.
    assert!(
        ipc::commands::osl_export_files().contains(&"store/messages.sqlite"),
        "HAZEL-3235 account backup contract omitted the protected message store"
    );
    let package_backup = snapshots.path().join("account-backup");
    for relative in ipc::commands::osl_export_files() {
        let source = core.join(relative);
        if source.is_file() {
            let destination = package_backup.join(relative);
            fs::create_dir_all(destination.parent().expect("TASK3235 package parent"))
                .expect("TASK3235 create package parent");
            fs::copy(source, destination).expect("TASK3235 copy account-backup artifact");
        }
    }
    let folder_backup = snapshots.path().join("whole-app-data");
    copy_tree(live.path(), &folder_backup);

    let expected = pre_burn_backup.protected_messages.len();
    let package_control_dir = snapshots.path().join("control-package/store");
    copy_tree(&package_backup.join("store"), &package_control_dir);
    let package_control = open_count(&package_control_dir, &secret, anchor.clone())
        .expect("same account backup opens normally without an intervening Burn");
    let folder_control = open_count(
        &folder_backup.join("config/osl-core/store"),
        &secret,
        anchor.clone(),
    )
    .expect("same whole-app folder opens normally without an intervening Burn");
    assert_eq!(package_control, expected);
    assert_eq!(folder_control, expected);
    println!(
        "TASK3235 before={} backup_name={} package_backup_present=true folder_backup_present=true",
        expected, pre_burn_backup.name
    );
    println!(
        "TASK3235 control=package no_burn=true open=normal readable_protected_messages={package_control}"
    );
    println!(
        "TASK3235 control=whole_app_folder no_burn=true open=normal readable_protected_messages={folder_control}"
    );

    let hub_state = HubCoreState::default();
    let verification = verify_password_role(&hub_state, "burn-pass-3235".to_string())
        .expect("TASK3235 verify exact saved burn password");
    assert_eq!(verification.role, VerifiedGateRole::Burn);
    let key_store = Task3234PowerCutKeyStore::seeded_with_restore_anchor(true, anchor.clone());
    let burn = task_3235_complete_verified_gate_burn(&hub_state, &config, &local, &key_store)
        .expect("TASK3235 complete saved-password Burn");
    assert!(burn.local_cleanup_complete);
    assert_eq!(key_store.usable_count(), 0);
    assert_eq!(key_store.invalidated_anchor_count(), 1);
    println!(
        "TASK3235 burn_role=burn local_cleanup_complete={} external_anchors_invalidated={} usable_keys_after={}",
        burn.local_cleanup_complete,
        key_store.invalidated_anchor_count(),
        key_store.usable_count()
    );

    let package_restore = core.join("store");
    copy_tree(&package_backup.join("store"), &package_restore);
    let package_warning = open_count(&package_restore, &secret, anchor.clone())
        .expect_err("old account backup must not open after Burn");
    assert!(package_warning.contains("behind external anchor"));
    let package_readable = 0usize;
    println!(
        "TASK3235 restore=package open=refused warning={package_warning:?} readable_protected_messages={package_readable}"
    );

    fs::remove_dir_all(live.path()).expect("TASK3235 clear package restore before folder replay");
    copy_tree(&folder_backup, live.path());
    let folder_warning = open_count(&store_dir, &secret, anchor.clone())
        .expect_err("old whole-app folder must not open after Burn");
    assert!(folder_warning.contains("behind external anchor"));
    let folder_readable = 0usize;
    println!(
        "TASK3235 restore=whole_app_folder open=refused warning={folder_warning:?} readable_protected_messages={folder_readable}"
    );

    let maximum_after = package_readable.max(folder_readable);
    assert_eq!(maximum_after, 0);
    println!(
        "TASK3235 SUMMARY fixture={} restore_modes=2 maximum_readable_after_burn_restore={} no_burn_package_readable={} no_burn_folder_readable={}",
        fixture_path.display(), maximum_after, package_control, folder_control
    );
    reset_globals();
}
