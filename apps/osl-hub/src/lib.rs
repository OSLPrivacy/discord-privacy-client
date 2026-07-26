// Single source of truth for which attachment formats the trusted picker may
// offer, derived from what the receiving viewer can decode. Needs `ipc` and
// `peer_attachment_io`, so it lives behind `core` like they do.
#[cfg(feature = "core")]
pub mod attachment_formats;
pub mod browser_companion;
pub mod burn_contract;
pub mod control_contract;
pub mod discord_carrier_geometry;
pub mod external_overlay;
pub(crate) mod firefox_migration_coordinator;
pub mod models;
pub mod mullvad_window_host;
pub mod native_apps;
pub mod native_attachment_jobs;
pub mod native_discord_adapter;
pub mod native_window_host;
#[cfg(feature = "core")]
pub mod peer_attachment_io;
pub mod preferences;
pub mod privacy_scan;
#[cfg(feature = "core")]
pub mod pro_context_cover;
pub mod service_host;
#[cfg(feature = "core")]
pub mod services;
pub mod updates;

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
pub mod identity_registry;
#[cfg(feature = "core")]
pub mod mass_cleanup;
// Timed deletion and view-once expiry. Needs `ipc` (sealed-at-rest storage) and
// `store` (local plaintext cache), so it lives behind `core` like they do.
#[cfg(feature = "core")]
pub mod message_expiry;
#[cfg(feature = "core")]
pub mod password_lifecycle;
#[cfg(feature = "core")]
pub mod scrub_index;
#[cfg(feature = "core")]
pub mod security;
#[cfg(feature = "core")]
pub mod security_credentials;
#[cfg(feature = "core")]
pub mod service_scope_index;
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
