//! D-142 — a fresh process opening an existing profile refuses to unlock.
//!
//! # Why this test exists and why it is `#[ignore]`d
//!
//! The refusal happens in `cmd_osl_verify_gate_password`'s post-gate branch
//! (`crates/ipc/src/commands.rs:14761-14785`): the password verifies, the file
//! storage key is installed, `session_lock::unlock_session` runs, and a
//! non-empty `ReloadReport::errors` makes the gate call `lock_session` and
//! return one sentence with no cause. The name of the loader that refused only
//! ever existed in a `tracing::error!` field, and the desktop binary registered
//! no subscriber, so nobody had ever read it.
//!
//! Driving that path through the GUI needs a full Tauri build plus WebKitWeb-
//! Driver. This is the cheap half: point it at a **real, already-onboarded
//! profile directory** and it runs the identical gate command against it, then
//! prints the `errors` vector verbatim.
//!
//! It is `#[ignore]`d because it needs a profile that only exists on a machine
//! where the app has actually been run, and it mutates that profile (unlock is
//! a write path — quarantine renames, re-encryption sweeps, marker refreshes).
//! **It copies the profile to a temp dir first and never touches the original.**
//!
//! ```bash
//! # <base> is the dir holding password_marker.json (…/org.oslprivacy.hub/osl-core)
//! OSL_D142_BASE_DIR=/path/to/osl-core \
//! OSL_D142_ACCOUNT_DIR=/path/to/osl-core/hub-identities/id-XXXX \
//! OSL_D142_PASSWORD='osl-e2e-Passw0rd!' \
//!   cargo test --test d142_post_gate_reload_real_profile -- --ignored --nocapture --test-threads=1
//! ```
//!
//! `--test-threads=1` is mandatory: the file storage key, the base-dir override
//! and the burn-state latch are all process globals.

use std::path::{Path, PathBuf};

fn copy_tree(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_tree(&from, &to)?;
        } else {
            std::fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

struct Fixture {
    _temp: tempfile::TempDir,
    base: PathBuf,
    account: PathBuf,
    password: String,
}

/// Returns `None` when the env vars are unset, so the test skips loudly rather
/// than passing by accident on a machine with no profile.
fn fixture() -> Option<Fixture> {
    let base_src = PathBuf::from(std::env::var("OSL_D142_BASE_DIR").ok()?);
    let account_src = PathBuf::from(std::env::var("OSL_D142_ACCOUNT_DIR").ok()?);
    let password = std::env::var("OSL_D142_PASSWORD").ok()?;

    let account_rel = account_src
        .strip_prefix(&base_src)
        .expect("OSL_D142_ACCOUNT_DIR must live under OSL_D142_BASE_DIR")
        .to_path_buf();

    let temp = tempfile::tempdir().expect("tempdir");
    let base = temp.path().join("osl-core");
    copy_tree(&base_src, &base).expect("copy profile");

    Some(Fixture {
        base: base.clone(),
        account: base.join(account_rel),
        password,
        _temp: temp,
    })
}

/// Run the real gate command against a copy of a real profile and print the
/// post-gate reload report. This is the instrument, not an assertion: its whole
/// job is to make the string that names the failing loader observable.
#[test]
#[ignore = "needs OSL_D142_BASE_DIR / OSL_D142_ACCOUNT_DIR / OSL_D142_PASSWORD"]
fn post_gate_reload_against_a_real_profile() {
    let Some(fx) = fixture() else {
        panic!(
            "set OSL_D142_BASE_DIR, OSL_D142_ACCOUNT_DIR and OSL_D142_PASSWORD — \
             see this file's module docs"
        );
    };

    ipc::main_password::set_file_storage_key(None);
    keystore::set_base_dir_override(Some(fx.base.clone()));
    keystore::set_active_account_dir(Some(fx.account.clone()));
    ipc::burned_scopes_file::reset_burn_state_unreadable_for_tests();

    let state = ipc::AppState::new();

    // Exactly what the desktop shell does on a returning launch: nothing is
    // pre-loaded into AppState, the gate is the first thing that touches the
    // profile.
    let verdict = ipc::commands::cmd_osl_verify_gate_password(&state, fx.password.clone());

    match &verdict {
        Ok(dto) => println!("[D-142] gate verify OK: result={:?}", dto.result),
        Err(e) => println!("[D-142] gate verify REFUSED: {e}"),
    }

    // Re-run the reload directly so the per-loader errors are visible even when
    // the gate collapsed them into its one sentence.
    ipc::main_password::set_file_storage_key(None);
    let marker = ipc::main_password::read_marker_pub(&fx.base).expect("read password marker");
    let outcome = ipc::main_password::verify_gate_password_with_marker(&marker, &fx.password)
        .expect("verify password");
    let ipc::main_password::GateMatch::Main(file_key) = outcome else {
        panic!("the supplied password is not the MAIN password for this profile");
    };
    ipc::main_password::set_file_storage_key_after_main_password_unlock(file_key);

    let fresh = ipc::AppState::new();
    match ipc::session_lock::unlock_session(&fresh, &fx.account) {
        Ok(report) => {
            println!("[D-142] identity_reloaded      = {}", report.identity_reloaded);
            println!(
                "[D-142] message_store_reopened = {}",
                report.message_store_reopened
            );
            println!("[D-142] peer_map_entries       = {}", report.reload.peer_map_entries);
            println!("[D-142] prekeys_loaded         = {}", report.reload.prekeys_loaded);
            println!("[D-142] u.reload.errors        = {:?}", report.reload.errors);
            assert!(
                report.reload.errors.is_empty(),
                "post-gate reload reported errors: {:?}",
                report.reload.errors
            );
        }
        Err(e) => panic!("[D-142] unlock_session returned Err: {e}"),
    }

    ipc::main_password::set_file_storage_key(None);
    keystore::set_active_account_dir(None);
    keystore::set_base_dir_override(None);
}
