//! D-207 — reopening a password-protected profile must ask for the password.
//!
//! The defect this reproduces is not subtle once written down: on **every**
//! relaunch the shipping build minted a fresh random device-bound storage key,
//! installed it process-wide, marked the session `unlocked`, never showed the
//! unlock screen, and then failed every encrypted read behind a Home that said
//! *"Protected — Device protection confirmed"* and *"Trusted people 0
//! verified"*.
//!
//! The cause was a directory disagreement. `password_marker.json` lives in the
//! **base** directory (`crates/ipc/src/commands.rs` `password_dir()`, which
//! resolves `keystore::osl_base_dir()` on purpose because the gate is
//! device-level). Every product caller of
//! `ensure_device_bound_fallback_file_storage_key` passed the **per-identity**
//! directory (`<base>/hub-identities/<slot>/`). The refusal guard asked
//! `marker_exists(dir)` — the caller's directory — so it never fired.
//!
//! These tests drive `HubCoreState::bootstrap_from_disk()`, which is the real
//! launch path (it runs `run_autostart_local`), against real profiles on disk.
//! Nothing here asserts on a hand-built struct: the whole point of D-207 is
//! that the internal state said `unlocked` while the user saw an empty app, so
//! the assertions are made on `core_bridge::readiness`, which is the exact
//! value the renderer routes on.
//!
//! # Mutants each test must go RED for
//!
//! * `reopening_a_password_protected_profile_refuses_the_mint_and_asks_for_the_password`
//!   — restore the per-identity directory at the guard (`marker_exists(dir)`
//!   in place of `marker_dir_in_lineage(dir)`). It must fail by REPRODUCING
//!   the mint: a key file appears and the session reports itself unlocked, not
//!   merely because a path string differs.
//! * the same test — force `readiness.unlocked` true while a marker is
//!   present (`|| get_file_storage_key().is_some()` in `core_bridge`).
//! * `a_genuinely_new_install_can_still_create_its_key` — delete the marker
//!   entirely. Onboarding must still work; the fix must not turn a first run
//!   into a locked door.
//! * `a_lost_device_key_is_not_the_unlock_screen` — a profile whose marker
//!   exists but whose identity blob will not open must reach the D-150
//!   message, not the unlock screen and not a silent empty Home.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use osl_privacy_hub::core_bridge::{readiness, HubCoreState};

/// Long enough for `validate_password`.
const PASSWORD: &str = "osl-D207-Passw0rd!";

/// Every test in this binary drives the same process-global slots (the file
/// storage key, the base-dir override, the active-account override), so they
/// run one at a time regardless of the harness's thread count.
static SERIAL: Mutex<()> = Mutex::new(());

fn serialize() -> MutexGuard<'static, ()> {
    SERIAL.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

struct Profile {
    _temp: tempfile::TempDir,
    base: PathBuf,
    account: PathBuf,
}

impl Drop for Profile {
    fn drop(&mut self) {
        ipc::main_password::set_file_storage_key(None);
        keystore::set_active_account_dir(None);
        keystore::set_base_dir_override(None);
    }
}

/// The exact on-disk shape D-207 was reproduced on: an `osl-core` base holding
/// the device-level `password_marker.json`, and the active identity one level
/// down in `hub-identities/<slot>/`.
fn profile(slot: &str, with_password: bool) -> Profile {
    let temp = tempfile::tempdir().expect("tempdir");
    let base = temp.path().join("osl-core");
    let account = base.join("hub-identities").join(slot);
    std::fs::create_dir_all(&account).expect("create account dir");

    ipc::main_password::set_file_storage_key(None);
    ipc::main_password::reset_device_bound_mint_refusals();
    keystore::set_base_dir_override(Some(base.clone()));
    keystore::set_active_account_dir(Some(account.clone()));

    if with_password {
        ipc::main_password::set_main_password(&base, PASSWORD).expect("set main password");
    }
    // A relaunch is a fresh process: whatever setting the password installed in
    // the slot is gone. This is the state the friend's machine is in.
    ipc::main_password::set_file_storage_key(None);

    Profile {
        _temp: temp,
        base,
        account,
    }
}

/// A real identity, sealed by the live device sealer, so bootstrap loads it the
/// way it loads a returning user's.
fn write_loadable_identity(account: &Path) {
    let sealer = keystore::select_best_sealer();
    let identity = keystore::generate_identity("osl_d207_returning_user".to_owned());
    keystore::save_identity(&account.join("identity.json"), &identity, sealer.as_ref())
        .expect("save identity");
}

/// An `identity.json` whose sealed body no key on this machine can open — the
/// "the key that opens this account is gone" shape (D-150). The method label is
/// the live sealer's, so the failure is an unseal failure rather than a method
/// rejection.
fn write_unopenable_identity(account: &Path) {
    let sealer = keystore::select_best_sealer();
    let blob = keystore::IdentityOnDisk {
        version: keystore::IDENTITY_BLOB_VERSION,
        method: sealer.method_label().to_string(),
        sealed_b64: "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".to_string(),
        insecure_banner: None,
    };
    std::fs::write(
        account.join("identity.json"),
        serde_json::to_vec_pretty(&blob).expect("encode identity blob"),
    )
    .expect("write identity.json");
}

fn fallback_key_files(profile: &Profile) -> Vec<PathBuf> {
    [profile.base.as_path(), profile.account.as_path()]
        .into_iter()
        .map(|dir| dir.join("file_storage_key_fallback.json"))
        .filter(|path| path.exists())
        .collect()
}

// =====================================================================
// 1. The defect itself.
// =====================================================================

#[test]
fn reopening_a_password_protected_profile_refuses_the_mint_and_asks_for_the_password() {
    let _serial = serialize();
    let profile = profile("id-d207-returning", true);
    write_loadable_identity(&profile.account);

    // The relaunch, through the app's own boot.
    let state = HubCoreState::bootstrap_from_disk();

    // The renderer's first move, and the call site that minted the key:
    // `get_osl_chat_local_state_key` passes `keystore::osl_config_dir()`, the
    // per-identity directory, exactly as `apps/osl-hub/src/main.rs` does.
    let directory = keystore::osl_config_dir().expect("config dir");
    assert_eq!(
        directory, profile.account,
        "the call site under test must be asking about the per-identity directory; \
         if this is the base dir the test is no longer exercising the mismatch"
    );
    let refused =
        osl_privacy_hub::osl_chat_local_state_key::osl_chat_local_state_key(&directory).err();
    let refused = refused.expect(
        "D-207: a password-gated profile must NOT hand out a storage key nobody authorised",
    );
    assert!(
        refused.contains(ipc::main_password::DEVICE_BOUND_FALLBACK_REFUSED),
        "the refusal must name its cause, got: {refused}"
    );

    // No key was minted, in EITHER directory. This is the assertion that goes
    // red by reproducing the defect rather than by reporting a path
    // difference: with the guard restored to `marker_exists(dir)` a
    // `file_storage_key_fallback.json` appears under the identity directory.
    assert_eq!(
        fallback_key_files(&profile),
        Vec::<PathBuf>::new(),
        "a device-bound fallback key was minted over a password-protected identity"
    );
    assert!(
        ipc::main_password::get_file_storage_key().is_none(),
        "a key nobody authorised was installed process-wide"
    );

    // It failed LOUDLY. A refusal that is only an `Err` in a caller that
    // swallows it is how this stayed invisible for a day.
    let refusals = ipc::main_password::device_bound_mint_refusals();
    assert!(
        refusals.count >= 1 && refusals.last_message.is_some(),
        "the refusal must be recorded, not merely returned: {refusals:?}"
    );

    // And the user is asked for the password. This is the whole product claim.
    let status = readiness(&state);
    assert!(
        status.password_gate_required,
        "the profile is password-protected"
    );
    assert!(
        status.identity_loaded,
        "the identity is sealed by the DEVICE, not the password, so it loads \
         before the gate; if it did not, this test would be proving the wrong thing"
    );
    assert!(
        !status.unlocked,
        "D-207: the session reported itself unlocked without the password"
    );
    assert_eq!(
        status.bootstrap_status, "passwordRequired",
        "the returning-user branch must be reachable; \
         `main.ts` routes to the unlock screen on exactly this value"
    );

    // The disproof, in the same direction the finding lane took it: supply the
    // missing user action and everything opens.
    ipc::main_password::verify_main_password(&profile.base, PASSWORD).expect("unlock");
    let unlocked = readiness(&state);
    assert!(unlocked.unlocked, "the real password must unlock the session");
    assert!(
        osl_privacy_hub::osl_chat_local_state_key::osl_chat_local_state_key(&directory).is_ok(),
        "after a real unlock the local-state key must derive from the password key"
    );
}

// =====================================================================
// 2. The fix must not turn a first run into a locked door.
// =====================================================================

#[test]
fn a_genuinely_new_install_can_still_create_its_key() {
    let _serial = serialize();
    let profile = profile("id-d207-fresh", false);

    let state = HubCoreState::bootstrap_from_disk();
    let directory = keystore::osl_config_dir().expect("config dir");

    let key = osl_privacy_hub::osl_chat_local_state_key::osl_chat_local_state_key(&directory)
        .expect("a first run has no marker anywhere and must still get a key");
    assert!(
        ipc::main_password::get_file_storage_key().is_some(),
        "the device-bound path must still install a key for an install with no password"
    );

    // It was minted at the AUTHORITY directory — the base — not under the
    // identity. That is the half of the fix that stops the next caller making
    // the same mistake: there is now one place to look.
    assert!(
        profile.base.join("file_storage_key_fallback.json").exists(),
        "the fallback key belongs in the directory that owns the gate"
    );
    assert!(
        !profile
            .account
            .join("file_storage_key_fallback.json")
            .exists(),
        "a per-identity key cannot be correct for a PROCESS-GLOBAL slot"
    );

    // And onboarding proceeds: no password is configured, so nothing is gated.
    let status = readiness(&state);
    assert!(!status.password_gate_required);
    assert!(
        status.unlocked,
        "an install with no password must not present a locked door"
    );

    // Stable across calls, and identical whichever directory in the lineage is
    // named — the disagreement that caused D-207 cannot recur.
    let again = osl_privacy_hub::osl_chat_local_state_key::osl_chat_local_state_key(&directory)
        .expect("reopen");
    assert_eq!(*key, *again);
    ipc::main_password::set_file_storage_key(None);
    let from_base =
        osl_privacy_hub::osl_chat_local_state_key::osl_chat_local_state_key(&profile.base)
            .expect("the base resolves to the same authority");
    assert_eq!(*key, *from_base);
}

// =====================================================================
// 3. A pre-D-207 install keeps its data.
// =====================================================================

#[test]
fn a_legacy_per_identity_key_is_adopted_rather_than_replaced() {
    let _serial = serialize();
    let profile = profile("id-d207-legacy", false);

    // The pre-fix layout: an install that legitimately had no password minted
    // its key under the identity directory, and its state files are sealed
    // under it. Re-minting at the base would orphan them — the very failure
    // D-207 is about, arriving from the fix.
    let legacy = ipc::main_password::ensure_device_bound_fallback_file_storage_key(&profile.account)
        .expect("legacy mint");
    let legacy_path = profile.account.join("file_storage_key_fallback.json");
    std::fs::rename(
        profile.base.join("file_storage_key_fallback.json"),
        &legacy_path,
    )
    .expect("stage the pre-fix layout");
    ipc::main_password::set_file_storage_key(None);

    let adopted = ipc::main_password::ensure_device_bound_fallback_file_storage_key(
        &keystore::osl_config_dir().expect("config dir"),
    )
    .expect("a legacy key must be opened, not replaced");

    assert_eq!(
        legacy, adopted,
        "the pre-fix key must survive; a different key here means every file \
         this profile ever sealed is unreadable"
    );
    assert!(
        !profile.base.join("file_storage_key_fallback.json").exists(),
        "adopting a legacy key must not also mint a second one"
    );
}

// =====================================================================
// 4. Key gone is not the same failure as password not yet typed.
// =====================================================================

#[test]
fn a_lost_device_key_is_not_the_unlock_screen() {
    let _serial = serialize();
    let profile = profile("id-d207-keygone", true);
    write_unopenable_identity(&profile.account);

    let state = HubCoreState::bootstrap_from_disk();
    let status = readiness(&state);

    assert!(
        !status.identity_loaded,
        "the identity blob must genuinely refuse to open, or this test proves nothing"
    );
    assert_eq!(
        status.bootstrap_status, "identityKeyLost",
        "a profile whose device key is gone must say so. `passwordRequired` \
         invites the user to type a password that cannot help, and \
         `setupRequired` shows a new-install screen over an account that still \
         exists. These two failures look identical to a user and must not be \
         conflated (D-150)."
    );
    assert_ne!(status.bootstrap_status, "passwordRequired");
    assert_ne!(status.bootstrap_status, "ready");
    assert!(
        !status.unlocked,
        "nothing about a lost key makes a session unlocked"
    );

    // And it still did not mint anything over the marker.
    assert_eq!(fallback_key_files(&profile), Vec::<PathBuf>::new());
}
