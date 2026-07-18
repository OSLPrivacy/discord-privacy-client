#![cfg(all(windows, feature = "core"))]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use osl_privacy_hub::core_bridge::{self, HubCoreState};
use osl_privacy_hub::identity_registry::{self, HubIdentityRegistryState};
use osl_privacy_hub::password_lifecycle;
use osl_privacy_hub::{broker, broker::HubBrokerState};

const CHILD_ENV: &str = "OSL_HUB_LIFECYCLE_CHILD";
const ROOT_ENV: &str = "OSL_HUB_LIFECYCLE_ROOT";
const PASSWORD: &str = "aB3!z9-safe-passphrase";
const WRONG_PASSWORD: &str = "xY7@q2-wrong-passphrase";
const LOCAL_PLAINTEXT: &str = "loopback-secret-never-on-disk";

#[test]
fn windows_identity_password_survives_process_restart() {
    let root = isolated_root();
    std::fs::create_dir_all(&root).expect("create isolated lifecycle root");

    run_child(&root, "create-and-lock");
    assert!(root.join("created.ok").is_file());
    run_child(&root, "reject-wrong-password");
    assert!(root.join("wrong-rejected.ok").is_file());
    run_child(&root, "restart-and-unlock");
    assert!(root.join("restart-unlocked.ok").is_file());
    run_child(&root, "restart-multi-identity");
    assert!(root.join("multi-identity-restart.ok").is_file());

    std::fs::remove_dir_all(&root).expect("remove isolated lifecycle root");
}

#[test]
fn windows_lifecycle_child() {
    let Ok(phase) = std::env::var(CHILD_ENV) else {
        return;
    };
    let root = PathBuf::from(std::env::var_os(ROOT_ENV).expect("isolated root supplied"));
    assert_isolated_root(&root);

    keystore::set_active_account_dir(None);
    keystore::set_base_dir_override(Some(root.join("osl-core")));

    match phase.as_str() {
        "create-and-lock" => create_and_lock(&root),
        "reject-wrong-password" => reject_wrong_password(&root),
        "restart-and-unlock" => restart_and_unlock(&root),
        "restart-multi-identity" => restart_multi_identity(&root),
        _ => panic!("unknown lifecycle probe phase"),
    }
}

fn run_child(root: &Path, phase: &str) {
    let status = Command::new(std::env::current_exe().expect("current test executable"))
        .args(["--exact", "windows_lifecycle_child", "--nocapture"])
        .env(CHILD_ENV, phase)
        .env(ROOT_ENV, root)
        .status()
        .expect("launch isolated lifecycle child");
    assert!(status.success(), "lifecycle child phase failed: {phase}");
}

fn create_and_lock(root: &Path) {
    let state = HubCoreState::bootstrap_from_disk();
    let initial = password_lifecycle::readiness(&state);
    assert_eq!(initial.access_state, "identitySetupRequired");
    assert!(!initial.identity_loaded);

    // Never contact production during this lifecycle probe. This file is
    // written only after first bootstrap so the legacy bootstrap cannot use
    // its `user_id` field to auto-generate an identity ahead of the lifecycle
    // API under test.
    std::fs::write(
        root.join("osl-core/keyserver.json"),
        br#"{"base_url":"http://127.0.0.1:1","user_id":"isolated-lifecycle-probe"}"#,
    )
    .expect("write loopback-only keyserver override");

    let created = password_lifecycle::create_native_identity(&state)
        .expect("create isolated native identity");
    assert_eq!(created.storage_method, keystore::METHOD_KEYRING);
    let identity_phrase = created
        .identity_recovery_phrase
        .expect("new identity has recovery phrase");
    assert_eq!(identity_phrase.split_whitespace().count(), 12);
    drop(identity_phrase);
    std::fs::write(root.join("expected-user-id"), created.user_id.as_bytes())
        .expect("persist public identity id for restart comparison");

    let setup = password_lifecycle::setup_main_password(&state, PASSWORD.to_owned())
        .expect("set isolated strong passphrase");
    assert_eq!(
        setup.password_recovery_phrase.split_whitespace().count(),
        12
    );
    assert!(setup.encrypted_state_reload_complete);
    drop(setup);

    let broker_state = HubBrokerState::default();
    let lease = broker_state
        .activate(
            hub_context(&created.user_id, "instagram", "account-a", "dm-a"),
            1,
        )
        .expect("activate exact local protected context");
    let protected = broker::prepare_local_protected_text(
        &state,
        &broker_state,
        &lease.context_token,
        LOCAL_PLAINTEXT.to_owned(),
    )
    .expect("prepare context-bound local protected capsule");
    assert_eq!(protected.protection, "local_protected_loopback");
    assert!(!protected.person_to_person_e2ee);
    assert!(protected.state_persisted);
    assert!(!protected.capsule.contains(LOCAL_PLAINTEXT));
    std::fs::write(root.join("protected-capsule"), protected.capsule)
        .expect("persist only ciphertext for restart phase");
    let protected_ledger =
        std::fs::read(root.join("osl-core/hub_local_protected.json")).expect("ledger persisted");
    assert!(ipc::main_password::has_enc_magic(&protected_ledger));
    assert!(!protected_ledger
        .windows(LOCAL_PLAINTEXT.len())
        .any(|window| window == LOCAL_PLAINTEXT.as_bytes()));

    let wrapper: serde_json::Value = serde_json::from_slice(
        &std::fs::read(root.join("osl-core/identity.json")).expect("sealed identity exists"),
    )
    .expect("identity wrapper is JSON");
    assert_eq!(
        wrapper.get("method").and_then(|v| v.as_str()),
        Some("keyring")
    );
    assert!(wrapper.get("sealed_b64").and_then(|v| v.as_str()).is_some());

    // Simulate the locked state before process exit and leave only a public
    // completion marker. Recovery phrases and keys are never written by the
    // test outside their normal sealed production files.
    ipc::main_password::set_file_storage_key(None);
    std::fs::write(root.join("created.ok"), b"ok").expect("write create marker");
}

fn reject_wrong_password(root: &Path) {
    let state = HubCoreState::bootstrap_from_disk();
    let before = password_lifecycle::readiness(&state);
    assert_eq!(before.access_state, "passwordRequired");
    assert!(before.identity_loaded);
    assert!(!before.unlocked);

    let self_id =
        std::fs::read_to_string(root.join("expected-user-id")).expect("public id marker exists");
    let broker_state = HubBrokerState::default();
    let lease = broker_state
        .activate(hub_context(&self_id, "instagram", "account-a", "dm-a"), 1)
        .expect("activate locked probe context");
    let capsule =
        std::fs::read_to_string(root.join("protected-capsule")).expect("capsule persisted");
    assert!(broker::decrypt_local_protected_capsule(
        &state,
        &broker_state,
        &lease.context_token,
        capsule
    )
    .is_err());

    let error = core_bridge::unlock_main_password(&state, WRONG_PASSWORD.to_owned())
        .expect_err("wrong password must be rejected");
    assert!(!error.contains(WRONG_PASSWORD));
    assert!(ipc::main_password::get_file_storage_key().is_none());
    std::fs::write(root.join("wrong-rejected.ok"), b"ok").expect("write rejection marker");
}

fn restart_and_unlock(root: &Path) {
    let sealer = keystore::select_best_sealer();
    assert_eq!(
        sealer.method_label(),
        keystore::METHOD_KEYRING,
        "restart must retain a persistent Windows sealer"
    );
    let directly_loaded =
        keystore::load_identity(&root.join("osl-core/identity.json"), sealer.as_ref())
            .unwrap_or_else(|error| panic!("sealed identity must load before bootstrap: {error}"));
    drop(directly_loaded);
    drop(sealer);

    let state = HubCoreState::bootstrap_from_disk();
    let before = password_lifecycle::readiness(&state);
    assert_eq!(before.access_state, "passwordRequired");
    assert!(!before.unlocked);

    let readiness = core_bridge::unlock_main_password(&state, PASSWORD.to_owned())
        .expect("correct password unlocks after restart");
    let expected_user_id =
        std::fs::read_to_string(root.join("expected-user-id")).expect("public id marker exists");
    assert_eq!(
        readiness.active_osl_user_id.as_deref(),
        Some(expected_user_id.as_str())
    );
    assert!(readiness.identity_loaded);
    assert!(readiness.unlocked);

    let capsule =
        std::fs::read_to_string(root.join("protected-capsule")).expect("capsule persisted");
    let broker_state = HubBrokerState::default();
    let exact = broker_state
        .activate(
            hub_context(&expected_user_id, "instagram", "account-a", "dm-a"),
            1,
        )
        .expect("reactivate exact context after restart");
    let decrypted = broker::decrypt_local_protected_capsule(
        &state,
        &broker_state,
        &exact.context_token,
        capsule.clone(),
    )
    .expect("decrypt local protected capsule after restart");
    assert_eq!(decrypted.plaintext, LOCAL_PLAINTEXT);
    assert_eq!(decrypted.protection, "local_protected_loopback");
    assert!(!decrypted.person_to_person_e2ee);
    assert!(decrypted.context_verified);
    drop(decrypted);

    for wrong in [
        hub_context(&expected_user_id, "instagram", "account-a", "dm-b"),
        hub_context(&expected_user_id, "instagram", "account-b", "dm-a"),
        hub_context(&expected_user_id, "telegram", "account-a", "dm-a"),
    ] {
        let lease = broker_state
            .activate(wrong, 2)
            .expect("activate intentionally wrong context");
        assert!(broker::decrypt_local_protected_capsule(
            &state,
            &broker_state,
            &lease.context_token,
            capsule.clone(),
        )
        .is_err());
    }

    assert_no_plaintext_files(root, LOCAL_PLAINTEXT.as_bytes());

    let registry_state = HubIdentityRegistryState::default();
    let initial_slots = identity_registry::list_identity_slots(&state, &registry_state)
        .expect("migrate primary identity into durable slot");
    assert_eq!(initial_slots.len(), 1);
    assert!(initial_slots[0].active);
    let primary_slot = initial_slots[0].slot_id.clone();
    std::fs::write(root.join("expected-primary-slot"), primary_slot.as_bytes())
        .expect("persist opaque public slot id");

    let created = identity_registry::create_identity_slot(
        &state,
        &registry_state,
        "Testing identity".to_owned(),
    )
    .expect("create second sealed identity slot");
    let second_phrase = created
        .identity_recovery_phrase
        .expect("second identity has one-time recovery phrase");
    assert!(identity_registry::recover_identity_slot(
        &state,
        &registry_state,
        "Duplicate recovery".to_owned(),
        second_phrase.clone(),
    )
    .is_err());
    drop(second_phrase);
    let second_slot = created.identity.slot_id.clone();
    std::fs::write(root.join("expected-second-slot"), second_slot.as_bytes())
        .expect("persist opaque second slot id");

    let switched = identity_registry::switch_identity_slot(&state, &registry_state, second_slot)
        .expect("switch to second identity");
    assert!(switched.context_invalidation_required);
    assert_eq!(switched.previous_slot_id, primary_slot);
    assert!(switched.active_identity.active);
    let switched_back =
        identity_registry::switch_identity_slot(&state, &registry_state, switched.previous_slot_id)
            .expect("switch back to primary identity");
    assert!(switched_back.context_invalidation_required);
    ipc::main_password::set_file_storage_key(None);
    assert!(identity_registry::list_identity_slots(&state, &registry_state).is_err());
    std::fs::write(root.join("restart-unlocked.ok"), b"ok").expect("write unlock marker");
}

fn restart_multi_identity(root: &Path) {
    let selected = identity_registry::select_active_identity_before_bootstrap()
        .expect("select opaque active slot before password unlock")
        .expect("active slot persisted");
    let expected_primary = std::fs::read_to_string(root.join("expected-primary-slot"))
        .expect("primary slot marker exists");
    assert_eq!(selected, expected_primary);
    let state = HubCoreState::bootstrap_from_disk();
    assert_eq!(
        password_lifecycle::readiness(&state).access_state,
        "passwordRequired"
    );
    core_bridge::unlock_main_password(&state, PASSWORD.to_owned())
        .expect("unlock selected identity after process restart");
    let registry_state = HubIdentityRegistryState::default();
    let slots = identity_registry::list_identity_slots(&state, &registry_state)
        .expect("list persisted identity slots after restart");
    assert_eq!(slots.len(), 2);
    assert_eq!(slots.iter().filter(|slot| slot.active).count(), 1);
    assert!(slots
        .iter()
        .any(|slot| slot.slot_id == expected_primary && slot.active));
    let expected_second = std::fs::read_to_string(root.join("expected-second-slot"))
        .expect("second slot marker exists");
    assert!(slots.iter().any(|slot| slot.slot_id == expected_second));
    std::fs::write(root.join("multi-identity-restart.ok"), b"ok")
        .expect("write multi-identity marker");
}

fn hub_context(
    self_id: &str,
    service_id: &str,
    account_id: &str,
    conversation_id: &str,
) -> broker::HubConversationContext {
    broker::HubConversationContext {
        service_id: service_id.to_owned(),
        account_id: account_id.to_owned(),
        conversation_kind: broker::HubConversationKind::Dm,
        conversation_id: conversation_id.to_owned(),
        space_id: None,
        participant_osl_ids: vec![self_id.to_owned()],
        self_osl_id: self_id.to_owned(),
    }
}

fn assert_no_plaintext_files(root: &Path, needle: &[u8]) {
    let mut pending = vec![root.to_owned()];
    while let Some(path) = pending.pop() {
        for entry in std::fs::read_dir(path).expect("scan isolated state") {
            let entry = entry.expect("state entry");
            let file_type = entry.file_type().expect("state entry type");
            if file_type.is_dir() {
                pending.push(entry.path());
            } else if file_type.is_file() {
                let bytes = std::fs::read(entry.path()).expect("read isolated state file");
                assert!(
                    !bytes.windows(needle.len()).any(|window| window == needle),
                    "plaintext leaked into isolated state file"
                );
            }
        }
    }
}

fn isolated_root() -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "osl-hub-lifecycle-probe-{}-{nonce}",
        std::process::id()
    ))
}

fn assert_isolated_root(root: &Path) {
    let temp = std::env::temp_dir();
    assert!(root.starts_with(&temp));
    assert!(root
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with("osl-hub-lifecycle-probe-")));
    let lowered = root.to_string_lossy().to_ascii_lowercase();
    assert!(!lowered.ends_with("\\appdata\\roaming\\org.oslprivacy.hub"));
    assert!(!lowered.ends_with("\\appdata\\roaming\\osl"));
}
