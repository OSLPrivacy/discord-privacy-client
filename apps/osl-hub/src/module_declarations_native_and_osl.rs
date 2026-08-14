// Native adapters and OSL workspace declarations. Append related work here.
pub mod native_a11y;
pub mod native_apps;
pub mod native_attachment_jobs;
pub mod native_attachment_jobs_bridge;
pub mod native_discord_adapter;
#[cfg(feature = "core")]
pub mod native_outlook_adapter;
pub mod native_signal_adapter;
pub mod native_telegram_adapter;
pub mod native_whatsapp_adapter;
pub mod native_window_host;
#[cfg(feature = "core")]
pub mod osl_chat_drag_drop;
#[cfg(feature = "core")]
pub mod osl_chat_file_limits;
#[cfg(feature = "core")]
pub mod osl_chat_local_state_key;
#[cfg(feature = "core")]
pub mod osl_mail;
#[cfg(feature = "core")]
pub mod osl_profile;
pub mod owner_presence;
#[cfg(feature = "core")]
pub mod peer_attachment_io;
#[cfg(feature = "core")]
pub mod personal_archive;
pub mod preferences;
pub mod privacy_scan;
#[cfg(feature = "core")]
pub mod pro_context_cover;
/// Bounded lifecycle commands for protected Signal rows.  These commands are
/// intentionally separate from the placement-only native Signal adapter.
pub mod signal_lifecycle_commands;
pub mod signal_surface_finder;
// Pure decision boundary: no store handle, no tauri, so it stays ungated and
// is checkable without the desktop build.
pub mod pro_marked_deletion;
// TASK 1451: records the per-message outcome of an accepted deletion (1449).
// Also a pure decision boundary: given attempt results, it never opens a
// locator or a store handle itself.
pub mod pro_marked_deletion_outcomes;
pub mod proprietary_module_boundary;
pub mod proprietary_module_lifecycle;
/// Durable exact-target retry for a due Discord deletion when the app is closed.
pub mod task_3319_discord_delete_retry;
