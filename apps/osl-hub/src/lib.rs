pub mod burn_contract;
pub mod control_contract;
pub mod external_overlay;
pub mod models;
pub mod native_apps;
pub mod native_window_host;
pub mod preferences;
pub mod privacy_scan;
pub mod service_host;
#[cfg(feature = "core")]
pub mod services;
pub mod updates;

mod atomic_file;

#[cfg(feature = "desktop")]
pub mod placement;

#[cfg(feature = "core")]
pub mod broker;
#[cfg(feature = "core")]
pub mod cleanup;
#[cfg(feature = "core")]
pub mod core_bridge;
#[cfg(feature = "core")]
pub mod identity_registry;
#[cfg(feature = "core")]
pub mod mass_cleanup;
#[cfg(feature = "core")]
pub mod password_lifecycle;
#[cfg(feature = "core")]
pub mod scrub_imap;
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
