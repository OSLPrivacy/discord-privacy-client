// Single source of truth for which attachment formats the trusted picker may
// offer, derived from what the receiving viewer can decode. Needs `ipc` and
// `peer_attachment_io`, so it lives behind `core` like they do.
#[cfg(feature = "core")]
pub mod attachment_formats;
pub mod attachment_scan;
pub mod attended_imap;
#[cfg(feature = "core")]
pub mod autoscrub_run;
pub mod background_priority;
pub mod browser_companion;
pub mod browser_footprint;
#[cfg(feature = "core")]
pub mod browser_profile_scan;
pub mod burn_contract;
pub mod cloud_autoscrub_authority;
pub mod cloud_autoscrub_consent;
pub mod cloud_autoscrub_envelope;
pub mod cloud_autoscrub_execution;
#[cfg(feature = "core")]
pub mod cloud_autoscrub_run;
pub mod consent_ledger;
pub mod control_contract;
pub mod discord_carrier_geometry;
pub mod execution_consent;
pub mod external_overlay;
pub(crate) mod firefox_migration_coordinator;
pub mod hosted_audience;
pub mod hosted_port;
pub mod hosted_provider_recipe;
pub mod hosted_session_port;
pub mod invite_clipboard;
pub mod models;
pub mod mullvad_window_host;
pub mod native_apps;
pub mod native_attachment_jobs;
pub mod native_discord_adapter;
pub mod native_signal_adapter;
pub mod native_whatsapp_adapter;
pub mod native_window_host;
#[cfg(feature = "core")]
pub mod osl_profile;
pub mod owner_presence;
#[cfg(feature = "core")]
pub mod peer_attachment_io;
pub mod preferences;
pub mod privacy_scan;
#[cfg(feature = "core")]
pub mod pro_context_cover;
pub mod proprietary_module_boundary;
pub mod proprietary_module_lifecycle;
pub mod scrub_evidence_manifest;
pub mod service_host;
#[cfg(feature = "core")]
pub mod services;
pub mod updates;
pub mod whatsapp_accessibility;
pub mod whatsapp_qa_host;
#[cfg(feature = "core")]
pub mod whatsapp_qa_pairing;
pub mod whatsapp_qa_transport;

// Native executable verification is exercised only by Windows callers. Keep
// its fail-closed types available to cross-platform manifests and tests.
#[cfg_attr(not(any(target_os = "windows", test)), allow(dead_code))]
pub(crate) mod windows_executable_trust;

mod atomic_file;

#[cfg(feature = "desktop")]
pub mod placement;

#[cfg(feature = "core")]
pub mod broker;
#[cfg(feature = "core")]
pub mod cleanup;
#[cfg(feature = "core")]
pub mod core_bridge;
#[cfg(all(feature = "core", feature = "discord-qa-shell"))]
pub mod discord_qa_identity;
#[cfg(all(feature = "core", feature = "discord-qa-shell"))]
pub mod discord_qa_inbound_receipt;
#[cfg(feature = "core")]
pub mod identity_binding_verifier;
#[cfg(feature = "core")]
pub mod identity_registry;
pub mod isolated_worker;
#[cfg(feature = "core")]
pub mod mass_cleanup;
// Timed deletion and view-once expiry. Needs `ipc` (sealed-at-rest storage) and
// `store` (local plaintext cache), so it lives behind `core` like they do.
#[cfg(feature = "core")]
pub mod message_expiry;
#[cfg(feature = "core")]
pub mod password_lifecycle;
// The pure half of the headless QA self-test driver in `main.rs`. It lives here
// only so it can actually be tested: the `osl-privacy-hub` binary cannot be
// built on a Linux host, so every `#[cfg(test)]` inside `main.rs` is compiled
// and never run.
#[cfg(all(feature = "core", feature = "discord-qa-shell"))]
pub mod qa_selftest_request;
// Provider-neutral off-screen surface capture. Same reason as
// `qa_selftest_request` above: it was a private module of `main.rs`, so its
// bounded-geometry and exact-session-binding tests were compiled by nothing and
// run by nothing. The module has no Tauri surface at all, so the library is
// where it belongs; `main.rs` keeps only the command that calls it.
#[cfg(feature = "core")]
pub mod native_surface_capture;
// The pure half of the desktop binary's Tauri command surface: send authority,
// checked-host ordering, reviewed-run identity binding, the restart-proof drain
// order, and the registered-command/ACL proofs. Same reason as the two modules
// above -- nothing inside `main.rs` is ever compiled or run by `--features
// core`, which is the only configuration CI and the accept commands can build.
#[cfg(feature = "core")]
pub mod hub_command_surface;
pub mod scrub_imap;
#[cfg(feature = "core")]
pub mod scrub_index;
pub mod scrub_receipt;
#[cfg(feature = "core")]
pub mod security;
#[cfg(feature = "core")]
pub mod security_credentials;
#[cfg(feature = "core")]
pub mod service_scope_index;
pub mod signal_destination_binding;
#[cfg(feature = "core")]
pub mod startup_gate;

// Share the original Tauri-free bootstrap verbatim so the app loads the same
// sealed identity and local security state without forking that logic.
#[cfg(feature = "core")]
#[allow(
    clippy::needless_borrow,
    clippy::needless_borrows_for_generic_args,
    clippy::uninlined_format_args
)]
#[path = "../../../src-tauri/src/bootstrap.rs"]
pub mod original_bootstrap;

// The keystore/ipc crates deliberately keep the active-account dir, base-dir
// override, and unlocked main-password key in process-wide statics (see
// crates/keystore/src/recipients.rs and crates/ipc/src/main_password.rs) —
// correct for a single-instance desktop app, but `cargo test` runs many test
// functions concurrently on separate OS threads inside one process. Any test
// (in any module) that mutates those globals must hold this lock for its
// entire critical section, or it can race a sibling test running in another
// module and silently read back the wrong key or directory. Deliberately
// crate-wide: a lock private to one module is no protection at all when a
// sibling module's test mutates the same global concurrently.
#[cfg(test)]
pub(crate) static GLOBAL_KEYSTORE_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Take the crate-wide keystore/globals lock, recovering from poisoning.
///
/// Every caller used to `.lock().unwrap()`, which meant one genuinely failing
/// test poisoned the mutex and every later test that touched these globals
/// died with `PoisonError` instead of running. On Windows CI that turned a
/// single real assertion failure in `services` into 11 reported failures and
/// buried the one that mattered. Poisoning tells us nothing useful here: the
/// lock guards process globals that each test sets up for itself, not an
/// invariant that a panic could leave half-applied.
#[cfg(test)]
pub(crate) fn global_keystore_test_lock() -> std::sync::MutexGuard<'static, ()> {
    GLOBAL_KEYSTORE_TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}
