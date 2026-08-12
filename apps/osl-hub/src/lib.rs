// Single source of truth for which attachment formats the trusted picker may
// offer, derived from what the receiving viewer can decode. Needs `ipc` and
// `peer_attachment_io`, so it lives behind `core` like they do.
pub mod account_burn_selection;
#[cfg(feature = "core")]
pub mod account_export;
#[cfg(feature = "core")]
pub mod account_identity_authority;
#[cfg(feature = "core")]
pub mod account_recovery;
pub mod adapter_profile_boot;
pub mod adapters;
#[cfg(feature = "core")]
pub mod allowed_place_commands;
#[cfg(feature = "core")]
#[cfg(feature = "core")]
pub mod attachment_formats;
pub mod attachment_limits;
#[cfg(feature = "core")]
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
#[cfg(feature = "core")]
pub mod autoscrub_bridge;
#[cfg(feature = "core")]
pub mod autoscrub_run;
pub mod background_priority;
pub mod bad_message_rules;
#[cfg(feature = "core")]
pub mod browser_companion;
// The persistent footprint store is sealed with `ipc`'s process key, so it
// belongs to the same runtime boundary as the other core storage modules.
#[cfg(feature = "core")]
pub mod browser_footprint;
#[cfg(feature = "core")]
pub mod browser_profile_scan;
pub mod build_integrity;
pub mod bundled_model_pack;
pub mod burn_authorize;
pub mod burn_contract;
#[cfg(feature = "core")]
pub mod burn_dispatch;
#[cfg(feature = "core")]
pub mod burn_journal_bridge;
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
#[cfg(feature = "core")]
pub mod cloud_autoscrub_run;
#[cfg(feature = "core")]
pub mod components;
pub mod consent_ledger;
pub mod control_contract;
#[cfg(feature = "core")]
pub mod coach_tips;
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
pub mod english_catalogue_entry;
#[cfg(feature = "desktop")]
pub mod entitlement_refresh;
pub mod execution_consent;
pub mod external_overlay;
pub(crate) mod firefox_migration_coordinator;
#[cfg(feature = "core")]
pub mod friend_account_reach;
pub mod front_window_grab;
pub mod hosted_audience;
pub mod hosted_port;
pub mod hosted_provider_recipe;
pub mod hosted_session_port;
pub mod instagram_send;
pub mod installed_build;
pub mod installed_build_version;
pub mod invite_clipboard;
// The iCloud Mail half of the shared mailbox reader (TASK 3071), and the iCloud
// fill-in of the shared mail deleter (TASK 3073). Both are pure and free of this
// crate's mail-website plumbing, so they are testable in every build that can
// compile this crate.
pub mod icloud_mail_deleter;
pub mod icloud_mailbox_reader;
/// The landing oracle: did this exact text land in the composer? Judged
/// through channels that did not write it. Pure above its syscall seam, so the
/// verdict is testable in every build that can compile this crate.
pub mod landing_oracle;
// The shared mail owner check (TASK 3044), carried out of `service_connections`
// by TASK 3045 so the shared mail deleter can reach it on its own.
pub mod mail_owner_check;
/// When the hidden main window may be shown. Pure, and deliberately not behind
/// `desktop`: the reveal rule is what decides whether the app is visible at all,
/// so it is testable in every build that can compile this crate.
pub mod main_window_reveal;
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
pub mod scrub_erasure;
pub mod scrub_erasure_queue;
pub mod scrub_erasure_tracker;
pub mod scrub_evidence_manifest;
pub mod shared_conversation_scroll;
#[cfg(feature = "core")]
pub mod shipping_icloud_mailbox_receive;
#[cfg(feature = "core")]
pub mod shipping_mailbox_pointer_reader;
pub mod tor_pref;
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
#[cfg(feature = "core")]
pub mod remove_everything;
#[cfg(all(feature = "core", feature = "desktop"))]
pub mod revocation_drain_timer;
#[cfg(feature = "core")]
pub mod rn_attribution;
#[cfg(feature = "core")]
pub mod rn_recovery;
#[cfg(feature = "core")]
pub mod run_choices;
pub mod scrub_hosted_port;
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
#[cfg(feature = "core")]
pub mod server_records;
pub mod service_connections;
#[cfg(feature = "core")]
pub mod service_host;
#[cfg(feature = "core")]
pub mod services;
#[cfg(feature = "core")]
pub mod update_apply;
#[cfg(feature = "core")]
pub mod update_state_backup;
pub mod updates;
pub mod visual_binding;
pub mod web_surface_adapter;
pub mod website_driver;
pub mod whatsapp_accessibility;
pub mod whatsapp_qa_host;
#[cfg(feature = "core")]
pub mod whatsapp_qa_pairing;
pub mod whatsapp_qa_transport;
pub mod whatsapp_window_composer;
pub mod x_whitelist;
/// Hermetic records for the direct X active-window discovery command.
pub mod x_window_composer;

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
pub mod burn_job_fence;
pub mod chat_app_timer_policy;
pub mod chat_capture_protection;
/// **The claim state.** What OSL may publicly say about each ruled surface, and
/// why — the owner gate `PLAN.md` r4-5 calls "the claim-state gap". Not behind a
/// feature: `native_apps` derives every public support label from it, and the
/// publication gates that read it must exist in every build that can compile the
/// native adapters.
pub mod claim_state;
#[cfg(feature = "core")]
pub mod cleanup;
#[cfg(feature = "core")]
pub mod core_bridge;
#[cfg(feature = "core")]
pub mod deadman;
#[cfg(feature = "core")]
pub mod destruct_ack_rollup;
#[cfg(feature = "core")]
pub mod device_transfer;
#[cfg(all(feature = "core", feature = "discord-qa-shell"))]
pub mod discord_qa_identity;
#[cfg(all(feature = "core", feature = "discord-qa-shell"))]
pub mod discord_qa_inbound_receipt;
#[cfg(feature = "core")]
pub mod eager_fetch;
#[cfg(feature = "core")]
pub mod eager_fetch_retry;
pub mod identity_binding_verifier;
#[cfg(feature = "core")]
pub mod identity_registry;
#[cfg(feature = "core")]
pub mod inbound_receipts;
pub mod isolated_worker;
#[cfg(feature = "core")]
pub mod mass_cleanup;
pub mod osl_chat_conversations;
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
#[cfg(feature = "core")]
pub mod runtime_switches;
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
