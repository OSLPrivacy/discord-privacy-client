// Shared bootstrap declaration. Keep the path at crate root via this include.
//
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
