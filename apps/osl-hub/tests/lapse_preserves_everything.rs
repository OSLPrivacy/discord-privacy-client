//! A prepaid entitlement ending may remove only optional Pro access. It must
//! never reset, delete, or rewrite account data owned by the shipping app.

#![cfg(feature = "core")]

use osl_privacy_hub::core_bridge::{license_state, HubCoreState};
use osl_privacy_hub::security::{add_friend_code, export_friend_code, HubSecurityState};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

const PASSWORD: &str = "lapse-preserves-everything-fixture-password";
const CHANNEL: &str = "osl-chat-lapse-preserves-everything";

struct IsolatedStorage {
    root: tempfile::TempDir,
}

impl IsolatedStorage {
    fn new() -> Self {
        let root = tempfile::tempdir().expect("create isolated storage root");
        keystore::set_base_dir_override(Some(root.path().to_owned()));
        keystore::set_active_account_dir(Some(root.path().to_owned()));
        ipc::main_password::set_file_storage_key(None);
        ipc::main_password::set_main_password(root.path(), PASSWORD)
            .expect("unlock isolated storage");
        Self { root }
    }

    fn path(&self) -> &Path {
        self.root.path()
    }
}

impl Drop for IsolatedStorage {
    fn drop(&mut self) {
        keystore::set_active_account_dir(None);
        keystore::set_base_dir_override(None);
        ipc::main_password::set_file_storage_key(None);
    }
}

fn snapshot(path: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn visit(root: &Path, current: &Path, output: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for entry in fs::read_dir(current).expect("read snapshot directory") {
            let entry = entry.expect("read snapshot entry");
            let path = entry.path();
            if path == root.join("license.json") {
                continue;
            }
            if path.is_dir() {
                visit(root, &path, output);
            } else {
                output.insert(
                    path.strip_prefix(root)
                        .expect("snapshot entry remains below root")
                        .to_owned(),
                    fs::read(&path).expect("read snapshot bytes"),
                );
            }
        }
    }

    let mut files = BTreeMap::new();
    visit(path, path, &mut files);
    files
}

#[test]
fn expired_pro_cache_leaves_identity_peers_history_and_notes_byte_identical() {
    let storage = IsolatedStorage::new();
    let bob = keystore::generate_identity("osl-lapse-bob".to_owned());
    let alice = keystore::generate_identity("osl-lapse-alice".to_owned());
    let sealer = keystore::select_best_sealer();
    keystore::save_identity(&storage.path().join("identity.json"), &bob, sealer.as_ref())
        .expect("save the account identity");

    let before_lapse = HubCoreState::default();
    *before_lapse.osl.identity.lock().expect("identity lock") = Some(bob.clone());
    let alice_core = HubCoreState::default();
    *alice_core.osl.identity.lock().expect("identity lock") = Some(alice.clone());
    let security = HubSecurityState::default();
    let friend = add_friend_code(
        &before_lapse,
        &security,
        export_friend_code(&alice_core)
            .expect("export Alice friend code")
            .friend_code,
        Some("Alice".to_owned()),
    )
    .expect("persist peer map");
    assert!(
        !friend.person_id.is_empty(),
        "fixture contains a real peer record"
    );

    let message_store =
        store::MessageStore::open(&storage.path().join("store"), bob.x25519_secret.as_bytes())
            .expect("open encrypted message store");
    *before_lapse
        .osl
        .message_store
        .lock()
        .expect("message store lock") = Some(message_store);
    ipc::commands::cmd_osl_persist_inbound(
        &before_lapse.osl,
        CHANNEL.to_owned(),
        "pre-lapse-inbound-0000000000000001".to_owned(),
        alice.user_id.clone(),
        "received before lapse".to_owned(),
    )
    .expect("persist pre-lapse received text");
    drop(before_lapse);

    // Notes is deliberately not compiled into the v1 UI (D43), but a lapse
    // still has no authority to touch its encrypted local artifact.
    fs::write(
        storage.path().join("osl_notes.bin"),
        b"sealed notes fixture bytes",
    )
    .expect("write Notes artifact");
    let before = snapshot(storage.path());

    let now = ipc::main_password::now_unix_secs_pub();
    keystore::save_license_cache(
        &storage.path().join("license.json"),
        &keystore::LicenseCacheInner {
            license_plaintext: "OSL-2222-3333-4444-5555".to_owned(),
            last_validated_status: "ACTIVE".to_owned(),
            redeemed_at: Some(now - 31 * 24 * 60 * 60),
            expires_at: Some(now - 1),
            current_period_end: Some(now - 1),
            last_validated_at: now - 1,
            checksum_ok: true,
        },
        sealer.as_ref(),
    )
    .expect("save expired Pro cache");

    let after_lapse = HubCoreState::bootstrap_from_disk();
    let entitlement = license_state(&after_lapse).expect("read expired entitlement");
    assert_eq!(
        entitlement.access, "free",
        "lapsed Pro access becomes Free offline"
    );
    assert_eq!(
        before,
        snapshot(storage.path()),
        "expiry may change entitlement only; every pre-existing account byte survives"
    );

    ipc::commands::cmd_osl_persist_outbound(
        &after_lapse.osl,
        CHANNEL.to_owned(),
        "post-lapse-outbound-00000000000001".to_owned(),
        "sent after lapse".to_owned(),
    )
    .expect("text sending stays available after lapse");
    ipc::commands::cmd_osl_persist_inbound(
        &after_lapse.osl,
        CHANNEL.to_owned(),
        "post-lapse-inbound-0000000000000001".to_owned(),
        alice.user_id,
        "received after lapse".to_owned(),
    )
    .expect("text receiving stays available after lapse");
    let history =
        ipc::commands::cmd_osl_load_channel_history(&after_lapse.osl, CHANNEL.to_owned(), Some(10))
            .expect("load text history after lapse");
    assert!(history
        .iter()
        .any(|row| row.plaintext == "sent after lapse"));
    assert!(history
        .iter()
        .any(|row| row.plaintext == "received after lapse"));
}
