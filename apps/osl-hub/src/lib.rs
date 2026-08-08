// Single source of truth for which attachment formats the trusted picker may
// offer, derived from what the receiving viewer can decode. Needs `ipc` and
// `peer_attachment_io`, so it lives behind `core` like they do.
pub mod account_burn_selection;
#[cfg(feature = "core")]
pub mod account_identity_authority;
#[cfg(all(feature = "core", feature = "desktop"))]
pub mod account_recovery;
#[cfg(feature = "core")]
pub mod allowed_place_commands;
pub mod adapter_profile_boot;
pub mod adapters;
#[cfg(all(feature = "core", feature = "desktop", not(feature = "desktop")))]
pub mod allowed_place_commands;
#[cfg(feature = "core")]
pub mod attachment_formats;
#[cfg(feature = "core")]
pub mod attachment_limits;
#[cfg(all(feature = "core", feature = "desktop"))]
#[path = "attachment_limits.rs"]
mod attachment_limits_desktop_decl;
#[cfg(feature = "core")]
pub mod attachment_partial_guard;
pub mod attachment_scan;
#[cfg(feature = "core")]
pub mod attachment_thumbnail;
#[cfg(feature = "core")]
pub mod attachment_thumbnail_policy;
// ai_carrier was once gated on `desktop` because it declared a
// #[tauri::command] and tauri only arrives with that feature. The command
// wrapper now lives in main.rs (the macro's __cmd__* helpers must sit beside
// the invoke_handler), so nothing here needs tauri: cover_ai is a
// non-optional dependency and credits/ai_consent are ungated. The gate had
// outlived its reason and broke the default-feature build, because
// broker.rs takes `&crate::ai_carrier::AiCarrierState` unconditionally.
pub mod ai_carrier;
pub mod ai_consent;
#[cfg(feature = "core")]
pub mod app_own_names;
pub mod attended_imap;
pub mod bad_message_rules;
#[cfg(all(feature = "core", feature = "desktop"))]
pub mod autoscrub_bridge;
#[cfg(all(feature = "core", feature = "desktop"))]
pub mod autoscrub_run;
pub mod background_priority;
#[cfg(all(feature = "core", feature = "desktop"))]
pub mod bad_message_rules;
pub mod browser_companion;
// The persistent footprint store is sealed with `ipc`'s process key, so it
// belongs to the same runtime boundary as the other core storage modules.
#[cfg(feature = "core")]
pub mod browser_footprint;
#[cfg(feature = "core")]
pub mod browser_profile_scan;
pub mod bundled_model_pack;
pub mod build_integrity;
pub mod burn_authorize;
pub mod burn_contract;
#[cfg(feature = "core")]
pub mod burn_dispatch;
#[cfg(feature = "core")]
pub mod burn_journal_bridge;
#[cfg(feature = "desktop")]
pub mod burn_review_state;
#[cfg(feature = "core")]
pub mod burn_server;
pub mod carrier_placement;
/// What a live carry receipt is bound to. Not behind a feature: the publication
/// gate that reads it must exist in every build that can compile the native
/// adapters.
pub mod carry_seam_contract;
pub mod cloud_autoscrub_authority;
pub mod cloud_autoscrub_consent;
pub mod cloud_autoscrub_envelope;
pub mod cloud_autoscrub_execution;
#[cfg(all(feature = "core", feature = "desktop"))]
pub mod cloud_autoscrub_run;
#[cfg(feature = "core")]
pub mod components;
pub mod consent_ledger;
pub mod control_contract;
// credits.rs existed but was never declared, so `crate::credits` failed to
// resolve the moment ai_carrier started using it - the file shipped as an
// orphan and only broke the build once something imported it.
pub mod credits;
// D-191: the process-wide `tracing` subscriber. In the lib, not in `main.rs`,
// so an integration test can install it against a hermetic path and read back
// the bytes it produced.
#[cfg(feature = "core")]
pub mod diagnostics;
pub mod discord_carrier_geometry;
#[cfg(feature = "desktop")]
pub mod entitlement_refresh;
pub mod execution_consent;
pub mod external_overlay;
pub(crate) mod firefox_migration_coordinator;
pub mod front_window_grab;
#[cfg(all(feature = "core", feature = "desktop"))]
pub mod friend_account_reach;
pub mod hosted_audience;
pub mod hosted_port;
pub mod hosted_provider_recipe;
pub mod hosted_session_port;
pub mod installed_build_version;
pub mod installed_build;
pub mod invite_clipboard;
/// The landing oracle: did this exact text land in the composer? Judged
/// through channels that did not write it. Pure above its syscall seam, so the
/// verdict is testable in every build that can compile this crate.
pub mod landing_oracle;
/// When the hidden main window may be shown. Pure, and deliberately not behind
/// `desktop`: the reveal rule is what decides whether the app is visible at all,
/// so it is testable in every build that can compile this crate.
pub mod main_window_reveal;
#[cfg(feature = "desktop")]
pub mod messenger_whitelist_kinds;
pub mod model_pack_install;
pub mod models;
pub mod mullvad_window_host;
pub mod named_places;
pub mod native_a11y;
pub mod native_apps;
pub mod native_attachment_jobs;
pub mod native_attachment_jobs_bridge;
pub mod native_discord_adapter;
#[cfg(all(feature = "core", feature = "desktop"))]
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
#[cfg(feature = "desktop")]
pub mod preferences;
#[cfg(feature = "desktop")]
pub mod privacy_scan;
#[cfg(feature = "core")]
pub mod pro_context_cover;
// Pure decision boundary: no store handle, no tauri, so it stays ungated and
// is checkable without the desktop build.
pub mod pro_marked_deletion;
pub mod proprietary_module_boundary;
pub mod proprietary_module_lifecycle;
pub mod scrub_erasure;
pub mod scrub_erasure_queue;
pub mod scrub_erasure_tracker;
pub mod scrub_evidence_manifest;
#[cfg(feature = "desktop")]
pub mod shared_conversation_scroll;
pub mod tor_pref;
#[cfg(feature = "desktop")]
pub mod scrub_hosted {
    pub mod checkpoint;
    pub mod fixture;
    pub mod friction;
    pub mod ordering;
    pub mod place_scope;
    pub mod proton_mail;
    pub mod reader;
    pub mod verify_surface;
    pub mod x_web;
    pub mod yahoo_mail;
}
#[cfg(feature = "desktop")]
pub mod messenger_whitelist_kinds;
#[cfg(feature = "core")]
pub mod remove_everything;
#[cfg(all(feature = "core", feature = "desktop"))]
pub mod revocation_drain_timer;
#[cfg(feature = "core")]
pub mod rn_attribution;
#[cfg(feature = "core")]
pub mod rn_recovery;
#[cfg(all(feature = "core", feature = "desktop"))]
pub mod run_choices;
#[cfg(feature = "desktop")]
pub mod scrub_hosted_port;
pub mod quiet_hours;
pub mod quiet_hours_notices;
pub mod scrub_setup_store;
/// **Binding Ledger 9, the seam ledger.** Adapters declared vs adapters with a
/// live carry receipt, ratcheted in both directions against
/// `carry-receipts/seam-ledger-baseline.json`. Test-only because its inputs --
/// `native_apps::tests::fleet_report` and the receipt verifier -- are, and
/// because a ledger is a gate rather than product code.
#[cfg(test)]
pub(crate) mod seam_ledger;
/// The one production `RawBackend` for `ipc::secure_local_store::SealedStore`.
/// Every other implementation in the tree is `#[cfg(test)]`, which is why the
/// offline send queue could not be wired at all before this module existed.
pub mod secure_disk_backend;
#[cfg(feature = "desktop")]
pub mod service_connections;
#[cfg(feature = "core")]
pub mod server_records;
#[cfg(all(feature = "core", feature = "desktop"))]
pub mod service_connections;
pub mod service_host;
#[cfg(feature = "core")]
pub mod services;
#[cfg(all(feature = "core", feature = "desktop"))]
pub mod update_apply;
#[cfg(all(feature = "core", feature = "desktop"))]
pub mod update_state_backup;
#[cfg(feature = "desktop")]
pub mod updates;
pub mod visual_binding;
#[cfg(feature = "desktop")]
pub mod web_surface_adapter;
#[cfg(feature = "desktop")]
pub mod website_driver;
pub mod whatsapp_accessibility;
pub mod whatsapp_qa_host;
#[cfg(feature = "core")]
pub mod whatsapp_qa_pairing;
pub mod whatsapp_qa_transport;
#[cfg(feature = "desktop")]
pub mod x_whitelist;

// Native executable verification is exercised only by Windows callers. Keep
// its fail-closed types available to cross-platform manifests and tests.
#[cfg_attr(not(any(target_os = "windows", test)), allow(dead_code))]
pub(crate) mod windows_executable_trust;

mod atomic_file;

#[cfg(feature = "desktop")]
pub mod placement;

#[cfg(feature = "core")]
pub mod broker;
pub mod chat_app_timer_policy;
pub mod chat_capture_protection;
/// **The claim state.** What OSL may publicly say about each ruled surface, and
/// why — the owner gate `PLAN.md` r4-5 calls "the claim-state gap". Not behind a
/// feature: `native_apps` derives every public support label from it, and the
/// publication gates that read it must exist in every build that can compile the
/// native adapters.
pub mod claim_state;
#[cfg(feature = "desktop")]
pub mod cleanup;
#[cfg(feature = "core")]
pub mod core_bridge;
#[cfg(feature = "desktop")]
pub mod deadman;
#[cfg(feature = "core")]
pub mod destruct_ack_rollup;
#[cfg(feature = "core")]
pub mod device_transfer;
#[cfg(all(feature = "core", feature = "discord-qa-shell", feature = "desktop"))]
pub mod discord_qa_identity;
#[cfg(all(feature = "core", feature = "discord-qa-shell", not(feature = "desktop")))]
#[path = "discord_qa_identity_core_shim.rs"]
pub mod discord_qa_identity;
#[cfg(all(feature = "core", feature = "discord-qa-shell"))]
pub mod discord_qa_inbound_receipt;
#[cfg(all(feature = "core", feature = "desktop"))]
pub mod eager_fetch;
#[cfg(all(feature = "core", feature = "desktop"))]
pub mod eager_fetch_retry;
pub mod identity_binding_verifier;
#[cfg(all(feature = "core", feature = "desktop"))]
pub mod identity_registry;
#[cfg(feature = "core")]
pub mod inbound_receipts;
pub mod isolated_worker;
#[cfg(feature = "desktop")]
pub mod mass_cleanup;
pub mod osl_chat_conversations;
// The chat-settings DANGER block. Needs the local message store, so it lives
// behind `core` like the store dependency itself.
#[cfg(feature = "core")]
pub mod osl_chat_danger_row;
#[cfg(feature = "desktop")]
pub mod osl_chat_delivery;
pub mod osl_chat_queue;
pub mod realtime_client;
pub mod realtime_decoy;
pub mod realtime_pipe;
pub mod realtime_resume;
pub mod realtime_subscription;
#[cfg(feature = "core")]
pub mod receipt_emit;
pub mod row_who_wrote_it;
#[cfg(all(feature = "core", feature = "desktop"))]
pub mod runtime_switches;
// Timed deletion and view-once expiry. Needs `ipc` (sealed-at-rest storage) and
// `store` (local plaintext cache), so it lives behind `core` like they do.
#[cfg(feature = "desktop")]
pub mod expiry_clock;
#[cfg(all(feature = "core", feature = "desktop"))]
pub mod message_expiry;
// View-once payloads are opened only after the native viewer proves capture
// protection; the module is dependency-free so its ordering tests run on all
// supported build hosts.
#[cfg(all(feature = "core", feature = "desktop"))]
pub mod password_lifecycle;
#[cfg(feature = "core")]
pub mod view_once_eligibility;
#[cfg(feature = "desktop")]
pub mod view_once_open;
#[cfg(feature = "desktop")]
pub mod view_once_watch;
pub mod visible_osl_mark;
// The pure half of the headless QA self-test driver in `main.rs`. It lives here
// only so it can actually be tested: the `osl-privacy-hub` binary cannot be
// built on a Linux host, so every `#[cfg(test)]` inside `main.rs` is compiled
// and never run.
#[cfg(all(feature = "core", feature = "discord-qa-shell", feature = "desktop"))]
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
#[cfg(all(feature = "core", feature = "desktop"))]
pub mod hub_command_surface;
#[cfg(all(feature = "core", not(feature = "desktop")))]
#[path = "hub_command_surface_core_shim.rs"]
pub mod hub_command_surface;
#[cfg(feature = "desktop")]
pub mod imap_verify;
#[cfg(all(feature = "core", feature = "desktop"))]
pub mod runtime_switches;
#[cfg(feature = "desktop")]
pub mod scrub_imap;
#[cfg(all(feature = "core", feature = "desktop"))]
pub mod scrub_index;
#[cfg(feature = "desktop")]
pub mod scrub_receipt;
#[cfg(feature = "core")]
pub mod security;
#[cfg(feature = "core")]
pub mod security_credentials;
#[cfg(feature = "desktop")]
pub mod sensitive_warning;
#[cfg(feature = "core")]
pub mod service_burn_selection;
#[cfg(feature = "core")]
pub mod service_scope_index;
#[cfg(feature = "desktop")]
pub mod shared_conversation_scroll;
#[cfg(feature = "desktop")]
pub mod shared_mailbox_reader;
pub mod signal_destination_binding;
#[cfg(feature = "core")]
pub mod signal_extra_device_sender;
#[cfg(feature = "core")]
pub mod spaces;
#[cfg(all(feature = "core", feature = "desktop"))]
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
