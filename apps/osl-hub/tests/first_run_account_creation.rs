#![cfg(feature = "core")]

//! D2b: creating the first account on a fresh profile must actually succeed.
//!
//! With the disposable QA identity out of the default build (see
//! `first_run_is_not_a_qa_account.rs`), the shipping app finally reached its
//! own account-creation flow — and that flow failed. `set_hub_recovery_kit_unsaved`
//! resolved the status file through `keystore::active_account_dir()`, which is
//! `None` until a *second* identity forces the registry to select a slot. A
//! first account lives flat in the base directory, so the mark failed, the UI
//! reported "OSL could not save the recovery-kit reminder" as a failed account
//! creation — and `identity.json` and `password_marker.json` were on disk
//! anyway, so the next launch asked the owner to unlock an account they had
//! just been told did not exist.
//!
//! This walks the real sequence the renderer performs: create identity, set
//! the main password, record the unsaved recovery kit. All three must succeed
//! against a directory layout that has never held an identity before.
//!
//! Not run by CI (which runs `--lib` plus `windows_identity_lifecycle` only).
//! It needs persistent OS credential storage, because identity creation
//! refuses an ephemeral sealer by design; on a machine without a keyring or
//! TPM it fails at `create_native_identity` with that message rather than
//! passing vacuously.

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use osl_privacy_hub::account_recovery;
use osl_privacy_hub::core_bridge::HubCoreState;
use osl_privacy_hub::password_lifecycle;

const PASSWORD: &str = "aB3!z9-first-run-passphrase";

fn isolated_root() -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "osl-hub-first-run-{}-{nonce}",
        std::process::id()
    ))
}

/// One test, deliberately: base directory, active account directory and the
/// unlocked file key are process globals, so a second concurrent test in this
/// binary would read back another test's account.
#[test]
fn a_fresh_profile_creates_a_recoverable_account_and_records_its_recovery_kit() {
    let root = isolated_root();
    let account = root.join("osl-core");
    std::fs::create_dir_all(&account).expect("create isolated account directory");

    // Deliberately NO keyserver.json yet. The file is an override, absent on a
    // real first launch, and `load_or_generate_identity` treats a `user_id` in
    // it as the seed for first-boot identity generation. Planting one here
    // would generate an identity during bootstrap and make step 1 assert
    // against a profile that is no longer empty — which is the exact condition
    // this test exists to rule out. The dead route is written at step 2,
    // before the first code path that reads it.

    // Exactly what the Tauri setup leaves behind on an empty profile:
    // `select_active_identity_before_bootstrap` finds no slot marker, so the
    // active account directory is cleared and the base is the flat namespace.
    keystore::set_active_account_dir(None);
    keystore::set_base_dir_override(Some(account.clone()));

    let state = HubCoreState::bootstrap_from_disk();

    // 1. The welcome screen's precondition: no identity, no password, and
    //    nothing already written into the profile.
    let before = password_lifecycle::readiness(&state);
    assert_eq!(
        before.access_state, "identitySetupRequired",
        "a fresh profile must ask for account creation, not resume one"
    );
    assert!(!before.identity_loaded, "no identity may exist yet");
    assert!(!before.main_password_set, "no password may exist yet");
    assert!(before.can_create_identity);
    assert!(
        !account.join("identity.json").exists(),
        "bootstrap must not have written an identity"
    );
    assert!(
        !account.join("password_marker.json").exists(),
        "bootstrap must not have written a password marker"
    );

    // Nothing listens here. Account creation must not depend on the keyserver:
    // `setup_main_password` re-runs the local bootstrap, which installs a
    // keyserver client from this route. A dead port proves the flow completes
    // without one, and keeps the test off the production keyserver.
    std::fs::write(
        account.join("keyserver.json"),
        br#"{"base_url":"http://127.0.0.1:1","user_id":"first-run-probe"}"#,
    )
    .expect("write refused keyserver route");

    // 2. Account creation hands back a real, usable 12-word phrase.
    let created = password_lifecycle::create_native_identity(
        &state,
        Some(password_lifecycle::IdentityCreationOwnerAuthorization::ExplicitOwnerSignoff),
    )
    .expect("a first account can be created on an empty profile");
    let identity_phrase = created
        .identity_recovery_phrase
        .clone()
        .expect("the owner is given the identity recovery phrase, not a zeroized None");
    assert_eq!(
        identity_phrase.split_whitespace().count(),
        12,
        "the recovery phrase must be the real 12-word mnemonic: {identity_phrase:?}"
    );
    bip39::Mnemonic::parse_in_normalized(bip39::Language::English, identity_phrase.trim())
        .expect("the recovery phrase must be a valid BIP39 mnemonic, not a placeholder");

    // 3. The password step completes and yields its own recovery phrase.
    let setup = password_lifecycle::setup_main_password(&state, PASSWORD.to_owned())
        .expect("the first main password can be set");
    assert_eq!(
        setup.password_recovery_phrase.split_whitespace().count(),
        12,
        "the password recovery phrase must be the real mnemonic too"
    );
    assert!(setup.readiness.identity_loaded);
    assert!(setup.readiness.main_password_set);
    assert!(setup.readiness.unlocked);
    assert!(account.join("identity.json").is_file());
    assert!(account.join("password_marker.json").is_file());

    // 4. The step that used to fail here, and be reported as a failed account
    //    creation: recording that a recovery kit exists and is not yet saved.
    account_recovery::mark_recovery_kit_unsaved()
        .expect("a first account can record its unsaved recovery kit");
    assert_eq!(
        account_recovery::recovery_kit_unsaved(),
        Ok(true),
        "the reminder must read back on the same fresh profile"
    );
    assert!(
        account.join("recovery_kit_status.json").is_file(),
        "the reminder belongs beside the account it describes"
    );
    account_recovery::clear_recovery_kit_unsaved()
        .expect("the owner can confirm the kit is saved");
    assert_eq!(account_recovery::recovery_kit_unsaved(), Ok(false));

    // 5. The phrase from step 2 really is this account's recovery phrase: it
    //    reconstructs the same OSL user id in a profile that has never seen it.
    let second = root.join("second-device");
    std::fs::create_dir_all(&second).expect("create second isolated account directory");
    keystore::set_base_dir_override(Some(second.clone()));
    let recovered_state = HubCoreState::bootstrap_from_disk();
    let recovered =
        password_lifecycle::import_native_identity_phrase(&recovered_state, identity_phrase)
            .expect("the recovery phrase restores the account on another device");
    assert_eq!(
        recovered.user_id, created.user_id,
        "the phrase must rebuild the same identity"
    );

    ipc::main_password::set_file_storage_key(None);
    keystore::set_base_dir_override(None);
    keystore::set_active_account_dir(None);
    let _ = std::fs::remove_dir_all(&root);
}
