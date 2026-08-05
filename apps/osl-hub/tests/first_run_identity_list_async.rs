#![cfg(feature = "core")]

//! NEW-3 — the first read of the identity list runs inside the Tauri async
//! runtime, and that is where it dies.
//!
//! `list_hub_identities` is declared `async fn`, so its body executes on
//! tokio's multi-thread runtime. Unlike every sibling in
//! `identity_registry` — `create_hub_identity_slot`, `recover_hub_identity_slot`,
//! `switch_hub_identity`, `burn_active_hub_identity`, all of which hop through
//! `tauri::async_runtime::spawn_blocking` — it calls straight into
//! `identity_registry::list_identity_slots`.
//!
//! That matters only because of what this one command does on a first run: no
//! slot marker exists yet, so it performs the whole flat-account migration,
//! including `original_bootstrap::run_autostart`, which re-registers the
//! identity against the key server over blocking HTTP. Blocking HTTP inside an
//! entered runtime is exactly the fault D5 was about ("Cannot drop a runtime in
//! a context where blocking is not allowed") — and the panic unwinds through
//! three held `std::sync::Mutex` guards, poisoning them, so every later read
//! answers "OSL identity registry is unavailable" for the rest of the session.
//!
//! This test drives the same sequence from inside a multi-thread runtime.

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use osl_privacy_hub::core_bridge::HubCoreState;
use osl_privacy_hub::identity_registry::{self, HubIdentityRegistryState};
use osl_privacy_hub::password_lifecycle;

const PASSWORD: &str = "aB3!z9-safe-passphrase";

fn isolated_base() -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "osl-hub-first-run-async-{}-{nonce}/osl-core",
        std::process::id()
    ))
}

#[test]
fn the_first_identity_list_read_survives_the_async_command_runtime() {
    let base = isolated_base();
    std::fs::create_dir_all(&base).expect("create isolated base dir");
    keystore::set_active_account_dir(None);
    keystore::set_base_dir_override(Some(base.clone()));

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("build a runtime that matches Tauri's");

    let slots = runtime.block_on(async {
        let state = HubCoreState::bootstrap_from_disk();
        std::fs::write(
            base.join("keyserver.json"),
            br#"{"base_url":"http://127.0.0.1:1","user_id":"isolated-first-run-probe"}"#,
        )
        .expect("write loopback-only keyserver override");

        password_lifecycle::create_native_identity(
            &state,
            Some(password_lifecycle::IdentityCreationOwnerAuthorization::ExplicitOwnerSignoff),
        )
        .expect("first-run account creation succeeds");
        password_lifecycle::setup_main_password(&state, PASSWORD.to_owned())
            .expect("first-run password setup succeeds");

        // The body of the `async fn list_hub_identities` command, on the
        // runtime the command actually runs on.
        let registry_state = HubIdentityRegistryState::default();
        identity_registry::list_identity_slots(&state, &registry_state)
    });

    let slots = slots.expect("Settings can read the identity list from the async command runtime");
    assert_eq!(slots.len(), 1);
    assert!(slots[0].active);

    ipc::main_password::set_file_storage_key(None);
    keystore::set_base_dir_override(None);
    if let Some(root) = base.parent() {
        let _ = std::fs::remove_dir_all(root);
    }
}
