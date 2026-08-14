// Lifecycle, command surfaces, security, and incoming-mail declarations.
// Append related work here.
//
// Timed deletion and view-once expiry. Needs `ipc` (sealed-at-rest storage) and
// `store` (local plaintext cache), so it lives behind `core` like they do.
pub mod expiry_clock;
#[cfg(feature = "core")]
pub mod message_expiry;
// View-once payloads are opened only after the native viewer proves capture
// protection; the module is dependency-free so its ordering tests run on all
// supported build hosts.
#[cfg(feature = "core")]
pub mod password_lifecycle;
#[cfg(feature = "core")]
pub mod view_once_eligibility;
pub mod view_once_open;
pub mod view_once_watch;
pub mod visible_osl_mark;
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
pub mod imap_verify;
#[cfg(feature = "core")]
pub mod scrub_imap;
#[cfg(feature = "core")]
pub mod scrub_index;
pub mod scrub_receipt;
#[cfg(feature = "core")]
pub mod security;
#[cfg(feature = "core")]
pub mod security_credentials;
pub mod sensitive_warning;
#[cfg(feature = "core")]
pub mod service_burn_selection;
#[cfg(feature = "core")]
pub mod service_scope_index;
pub mod shared_mail_deleter;
pub mod shared_mail_reader_types;
pub mod shared_mailbox_reader;
pub mod shared_marked_message_deleter;
pub mod signal_destination_binding;
#[cfg(feature = "core")]
pub mod signal_extra_device_sender;
#[cfg(feature = "core")]
pub mod spaces;
#[cfg(feature = "core")]
pub mod startup_gate;
#[cfg(feature = "core")]
pub mod story_privacy_surface;
pub mod timed_delete_closed_recovery;
pub mod timed_delete_sweep_job;
