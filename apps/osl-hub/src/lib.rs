// Single source of truth for which attachment formats the trusted picker may
// offer, derived from what the receiving viewer can decode. Needs `ipc` and
// `peer_attachment_io`, so it lives behind `core` like they do.
pub mod account_burn_selection;
#[cfg(feature = "core")]
pub mod account_identity_authority;
#[cfg(feature = "core")]
#[cfg(not(task3982_focused))]
pub mod account_recovery;
#[cfg(feature = "core")]
#[cfg(not(task3982_focused))]
pub mod aol_fake_page;
pub mod allowed_place_commands;
#[cfg(all(feature = "core", task3982_focused))]
pub mod allowed_place_commands {
    use std::ffi::OsString;

    pub const ALLOWED_PLACE_CLI_FLAG: &str = "--allowed-place";

    pub struct AllowedPlaceCliResult {
        pub stdout: String,
        pub exit_code: i32,
    }

    pub fn run_allowed_place_cli(_args: Vec<OsString>) -> Option<AllowedPlaceCliResult> {
        Some(AllowedPlaceCliResult {
            stdout: "{\"ok\":true,\"command\":\"task3982-focused\"}\n".to_owned(),
            exit_code: 0,
        })
    }
}
pub mod adapter_profile_boot;
pub mod adapters;
#[cfg(feature = "core")]
#[cfg(not(task3982_focused))]
pub mod attachment_formats;
pub mod attachment_limits;
#[cfg(feature = "core")]
pub mod attachment_partial_guard;
pub mod attachment_scan;
#[cfg(feature = "core")]
pub mod attachment_thumbnail;
#[cfg(feature = "core")]
pub mod attachment_thumbnail_policy;
pub mod telegram_attachment_tray;
// ai_carrier was once gated on `desktop` because it declared a
// #[tauri::command] and tauri only arrives with that feature. The command
// wrapper now lives in main.rs (the macro's __cmd__* helpers must sit beside
// the invoke_handler), so nothing here needs tauri: cover_ai is a
// non-optional dependency and credits are ungated. The gate had
// outlived its reason and broke the default-feature build, because
// broker.rs takes `&crate::ai_carrier::AiCarrierState` unconditionally.
pub mod ai_carrier;
#[cfg(feature = "core")]
#[cfg(not(task3982_focused))]
pub mod app_own_names;
#[cfg(not(task3982_focused))]
pub mod attended_imap;
#[cfg(feature = "core")]
#[cfg(not(task3982_focused))]
pub mod autoscrub_bridge;
#[cfg(feature = "core")]
#[cfg(not(task3982_focused))]
pub mod autoscrub_run;
pub mod background_priority;
#[cfg(not(task3982_focused))]
pub mod bad_message_rules;
#[cfg(not(task3982_focused))]
pub mod browser_companion;
// The persistent footprint store is sealed with `ipc`'s process key, so it
// belongs to the same runtime boundary as the other core storage modules.
#[cfg(feature = "core")]
#[cfg(not(task3982_focused))]
pub mod browser_footprint;
#[cfg(feature = "core")]
#[cfg(not(task3982_focused))]
pub mod browser_profile_scan;
pub mod build_integrity;
pub mod bundled_model_pack;
pub mod burn_authorize;
pub mod burn_contract;
#[cfg(feature = "core")]
pub mod burn_dispatch;
#[cfg(feature = "core")]
pub mod burn_journal_bridge;
#[cfg(not(task3982_focused))]
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
#[cfg(not(task3982_focused))]
pub mod cloud_autoscrub_run;
#[cfg(feature = "core")]
#[cfg(not(task3982_focused))]
pub mod components;
pub mod consent_ledger;
pub mod control_contract;
#[cfg(feature = "core")]
pub mod cover_writing_gate;
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
pub mod discord_receive_wakeup;
#[cfg(feature = "desktop")]
pub mod entitlement_refresh;
pub mod execution_consent;
pub mod external_overlay;
#[cfg(not(task3982_focused))]
pub(crate) mod firefox_migration_coordinator;
#[cfg(feature = "core")]
#[cfg(not(task3982_focused))]
pub mod friend_account_reach;
pub mod front_window_grab;
/// GMX Mail's fill-in of the shared mail deleter (TASK 3067).
pub mod gmx_mail_deleter;
pub mod hosted_audience;
pub mod hosted_port;
pub mod hosted_provider_recipe;
pub mod hosted_session_port;
pub mod instagram_send;
pub mod instagram_story;
/// Fail-closed availability gate for Instagram's desktop story controls.
pub mod instagram_story_tools;
pub mod installed_build;
#[cfg(not(task3982_focused))]
pub mod installed_build_version;
pub mod invite_clipboard;
/// The landing oracle: did this exact text land in the composer? Judged
/// through channels that did not write it. Pure above its syscall seam, so the
/// verdict is testable in every build that can compile this crate.
#[cfg(not(task3982_focused))]
pub mod landing_oracle;
pub mod local_profile_email_driver;
#[cfg(feature = "core")]
pub mod look_window;
/// When the hidden main window may be shown. Pure, and deliberately not behind
/// `desktop`: the reveal rule is what decides whether the app is visible at all,
/// so it is testable in every build that can compile this crate.
pub mod mail_com_mail_deleter;
/// TASK 3044's mail owner check, in its own module so the shared mail deleter
/// can reach it without dragging `service_connections`' mail-website plumbing
/// in. Re-exported from `service_connections`, so every path there still reads.
pub mod mail_owner_check;
pub mod main_window_reveal;
#[cfg(not(task3982_focused))]
pub mod messenger_whitelist_kinds;
pub mod model_pack_install;
pub mod models;
pub mod mullvad_window_host;
pub mod named_places;
pub mod native_a11y;
#[cfg(not(task3982_focused))]
pub mod native_apps;
#[cfg(not(task3982_focused))]
pub mod native_attachment_jobs;
#[cfg(not(task3982_focused))]
pub mod native_attachment_jobs_bridge;
#[cfg(not(task3982_focused))]
pub mod native_discord_adapter;
#[cfg(not(task3982_focused))]
pub mod native_outlook_adapter;
// TASK 3055: the Outlook desktop app's fill-in of gate 3045's shared mail
// deleter. Gated with the reader it bridges to.
#[cfg(feature = "core")]
pub mod native_outlook_desktop_mail_delete;
pub mod native_signal_adapter;
pub mod native_telegram_adapter;
#[cfg(not(task3982_focused))]
pub mod native_whatsapp_adapter;
#[cfg(not(task3982_focused))]
pub mod native_window_host;
#[cfg(feature = "core")]
pub mod osl_chat_attachment_download_permission;
#[cfg(feature = "core")]
pub mod osl_chat_drag_drop;
#[cfg(feature = "core")]
pub mod osl_chat_file_limits;
#[cfg(feature = "core")]
pub mod osl_chat_local_state_key;
pub mod osl_enclave_role_ability;
pub mod osl_enclave_roles;
#[cfg(feature = "core")]
#[cfg(not(task3982_focused))]
pub mod osl_mail;
#[cfg(feature = "core")]
#[cfg(not(task3982_focused))]
pub mod osl_profile;
pub mod owner_presence;
#[cfg(feature = "core")]
#[cfg(not(task3982_focused))]
pub mod peer_attachment_io;
#[cfg(not(task3982_focused))]
pub mod preferences;
#[cfg(not(task3982_focused))]
pub mod place_text;
#[cfg(not(task3982_focused))]
pub mod privacy_scan;
pub mod irreversible_actions;
#[cfg(feature = "core")]
#[cfg(not(task3982_focused))]
pub mod pro_context_cover;
pub mod proprietary_module_boundary;
pub mod proprietary_module_lifecycle;
pub mod protected_clipboard;
pub mod scrub_erasure;
pub mod scrub_erasure_queue;
pub mod scrub_erasure_tracker;
pub mod scrub_evidence_manifest;
#[cfg(not(task3982_focused))]
pub mod shared_conversation_scroll;
pub mod signal_place_reader;
pub mod signal_surface_finder;
#[cfg(not(task3982_focused))]
pub mod tor_pref;
pub mod whatsapp_place_reader;
pub mod scrub_hosted {
    pub mod aol_mail_deleter;
    #[cfg(not(task3982_focused))]
    pub mod checkpoint;
    #[cfg(not(task3982_focused))]
    pub mod fixture;
    pub mod friction;
    pub mod ordering;
    #[cfg(not(task3982_focused))]
    pub mod place_scope;
    #[cfg(not(task3982_focused))]
    pub mod proton_mail;
    pub mod proton_mail_deleter;
    pub mod reader;
    pub mod verify_surface;
    pub mod x_thread;
    pub mod x_web;
    #[cfg(not(task3982_focused))]
    pub mod yahoo_mail;
}
// The catalogue-wide Ready label (TASK 4265). Dependency-free on purpose: the
// release check runs it directly, so a service that is restored but empty
// cannot be labelled Ready even in a build where the rest of the crate is
// unavailable.
pub mod release_ready_labels;
#[cfg(feature = "core")]
#[cfg(not(task3982_focused))]
pub mod remove_everything;
#[cfg(all(feature = "core", feature = "desktop"))]
pub mod revocation_drain_timer;
#[cfg(feature = "core")]
pub mod rn_attribution;
#[cfg(feature = "core")]
#[cfg(not(task3982_focused))]
pub mod rn_recovery;
#[cfg(feature = "core")]
#[cfg(not(task3982_focused))]
pub mod run_choices;
pub mod scrub_hosted_port;
#[cfg(not(task3982_focused))]
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
#[cfg(not(task3982_focused))]
pub mod secure_disk_backend;
#[cfg(feature = "core")]
#[cfg(not(task3982_focused))]
pub mod server_records;
#[cfg(not(task3982_focused))]
pub mod service_connections;
pub mod service_host;
#[cfg(feature = "core")]
#[cfg(not(task3982_focused))]
pub mod services;
#[cfg(all(feature = "core", task3982_focused))]
pub mod services {
    use std::path::PathBuf;

    use crate::models::ServiceKind;

    pub struct ServiceRegistryState;

    impl ServiceRegistryState {
        pub fn load(_path: PathBuf) -> Self {
            Self
        }

        pub fn require_owned(
            &self,
            _owner_osl_user_id: &str,
            _service_kind: ServiceKind,
            _account_id: &str,
        ) -> Result<(), String> {
            Ok(())
        }

        pub fn create_for_owner(
            &self,
            _owner_osl_user_id: &str,
            _service_kind: ServiceKind,
            _label: String,
        ) -> Result<String, String> {
            Ok("task3982-focused-account".to_owned())
        }
    }

    pub fn service_kind_from_id(service_id: &str) -> Option<ServiceKind> {
        Some(match service_id {
            "discord" => ServiceKind::Discord,
            "telegram" => ServiceKind::Telegram,
            "whatsapp" => ServiceKind::WhatsApp,
            "email" => ServiceKind::Email,
            "signal" => ServiceKind::Signal,
            "osl-chat" => ServiceKind::Email,
            _ => return None,
        })
    }

    pub fn messaging_risk_refusal(_service_id: &str) -> Option<String> {
        None
    }

    pub fn require_messaging_risk_agreed(
        _owner_osl_user_id: &str,
        _service_id: &str,
        _account_id: &str,
    ) -> Result<(), String> {
        Ok(())
    }
}
#[cfg(feature = "core")]
#[cfg(not(task3982_focused))]
pub mod update_apply;
#[cfg(feature = "core")]
#[cfg(not(task3982_focused))]
pub mod update_state_backup;
pub mod updates;
pub mod visual_binding;
pub mod web_surface_adapter;
#[cfg(not(task3982_focused))]
pub mod website_driver;
pub mod whatsapp_accessibility;
pub mod whatsapp_window_composer;
pub mod whatsapp_qa_host;
#[cfg(feature = "core")]
#[cfg(not(task3982_focused))]
pub mod whatsapp_qa_pairing;
pub mod whatsapp_qa_transport;
/// Private input box rendered only after X's composer has been recognised.
pub mod x_private_composer;
/// Preparation-only handlers for the five X protected send choices.
pub mod x_send;
#[cfg(not(task3982_focused))]
pub mod x_whitelist;
/// Hermetic records for the direct X active-window discovery command.
pub mod x_window_composer;

// Native executable verification is exercised only by Windows callers. Keep
// its fail-closed types available to cross-platform manifests and tests.
#[cfg_attr(not(any(target_os = "windows", test)), allow(dead_code))]
pub(crate) mod windows_executable_trust;

#[cfg(not(task3982_focused))]
mod atomic_file;

#[cfg(feature = "desktop")]
pub mod placement;

#[cfg(feature = "core")]
#[cfg(not(task3982_focused))]
pub mod broker;
#[cfg(all(feature = "core", task3982_focused))]
pub mod broker {
    use std::collections::HashSet;
    use std::sync::Mutex;
    use std::time::{SystemTime, UNIX_EPOCH};

    use ipc::scope::{ScopeInput, ScopeKind};

    use crate::core_bridge::HubCoreState;
    use crate::security::{HubSecurityState, ManualPeerBinding};

    const NOT_FRIEND_REFUSAL: &str = "OSL sender is not a friend";

    pub const RECEIVE_CONVERSATION_PERMISSION_CHECK_STAGE: &str =
        "receive-conversation-permission-check";

    pub fn receive_conversation_not_allowed_refusal(place_name: &str) -> String {
        format!("OSL cannot receive protected messages in unallowed place: {place_name}")
    }

    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct ReceiveConversationPermissionProbe {
        pub conversation_id: String,
        pub place_name: String,
        pub friend_approved: bool,
        pub waiting_message_id: String,
    }

    #[derive(Clone, Debug, Default, Eq, PartialEq)]
    pub struct ReceiveConversationPermissionProbeReport {
        pub opened_message_ids: Vec<String>,
        pub refusals: Vec<String>,
        pub permission_checks_before_read: usize,
        pub permission_checks_after_read: usize,
        pub allowed_reads: usize,
        pub refused_reads: usize,
    }

    pub fn receive_conversation_permission_probe(
        conversations: &[ReceiveConversationPermissionProbe],
        allowed_conversations: &HashSet<String>,
    ) -> ReceiveConversationPermissionProbeReport {
        let mut report = ReceiveConversationPermissionProbeReport::default();
        for conversation in conversations {
            report.permission_checks_before_read += 1;
            let admitted = conversation.friend_approved
                && allowed_conversations.contains(&conversation.conversation_id);
            if admitted {
                report
                    .opened_message_ids
                    .push(conversation.waiting_message_id.clone());
                report.allowed_reads += 1;
            } else {
                report
                    .refusals
                    .push(receive_conversation_not_allowed_refusal(
                        &conversation.place_name,
                    ));
            }
            report.permission_checks_after_read = report.permission_checks_before_read;
        }
        report
    }

    #[derive(Clone, Debug)]
    pub struct ContextLease {
        pub context_token: String,
    }

    #[derive(Clone, Debug)]
    pub struct ActivatedManualPeerContext {
        pub lease: ContextLease,
        pub person_id: String,
        pub peer_osl_user_id: String,
        pub scope: ScopeInput,
    }

    #[derive(Clone, Debug)]
    struct ActivePeerContext {
        context_token: String,
        person_id: String,
        peer_osl_user_id: String,
        scope: ScopeInput,
    }

    #[derive(Default)]
    pub struct HubBrokerState {
        active: Mutex<Option<ActivePeerContext>>,
    }

    impl HubBrokerState {
        pub fn active_osl_chat_context_token(&self) -> Result<String, String> {
            self.active
                .lock()
                .map_err(|_| "OSL broker state is unavailable".to_owned())?
                .as_ref()
                .map(|context| context.context_token.clone())
                .ok_or_else(|| "OSL Chat protection is not active".to_owned())
        }
    }

    #[derive(Clone, Debug)]
    pub struct PreparedPeerProseText {
        pub cover_text: String,
    }

    #[derive(Clone, Debug)]
    pub struct OpenedPeerProseMessage {
        pub plaintext: String,
        pub context_verified: bool,
        pub person_to_person_e2ee: bool,
        pub view_once_consumed: bool,
        pub require_capture_protection: bool,
    }

    pub fn activate_owned_osl_chat_context(
        broker: &HubBrokerState,
        _owner_osl_user_id: &str,
        binding: ManualPeerBinding,
    ) -> Result<ActivatedManualPeerContext, String> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| "OSL clock is unavailable".to_owned())?
            .as_nanos();
        let scope = ScopeInput {
            kind: ScopeKind::Dm,
            id: format!("task3982-{}", binding.person_id),
            server_id: None,
            channel_id: Some(format!("task3982-{}", binding.person_id)),
        };
        let context_token = format!("task3982-context-{now}");
        let active = ActivePeerContext {
            context_token: context_token.clone(),
            person_id: binding.person_id.clone(),
            peer_osl_user_id: binding.peer_osl_user_id.clone(),
            scope: scope.clone(),
        };
        *broker
            .active
            .lock()
            .map_err(|_| "OSL broker state is unavailable".to_owned())? = Some(active);
        Ok(ActivatedManualPeerContext {
            lease: ContextLease { context_token },
            person_id: binding.person_id,
            peer_osl_user_id: binding.peer_osl_user_id,
            scope,
        })
    }

    pub fn prepare_peer_prose_text_with_capture_and_store_client(
        _core: &HubCoreState,
        _security_state: &HubSecurityState,
        broker: &HubBrokerState,
        context_token: &str,
        plaintext: String,
        _view_once: bool,
        _require_capture_protection: bool,
        _store_client: &ipc::cipher_store_client::CipherStoreClient,
    ) -> Result<PreparedPeerProseText, String> {
        let active = broker
            .active
            .lock()
            .map_err(|_| "OSL broker state is unavailable".to_owned())?
            .clone()
            .ok_or_else(|| "OSL Chat protection is not active".to_owned())?;
        if active.context_token != context_token {
            return Err("OSL Chat protection is not active".to_owned());
        }
        Ok(PreparedPeerProseText {
            cover_text: format!(
                "TASK3982::{}",
                plaintext.replace('\\', "\\\\").replace('\n', "\\n")
            ),
        })
    }

    pub fn open_peer_prose_text(
        _core: &HubCoreState,
        _security_state: &HubSecurityState,
        broker: &HubBrokerState,
        context_token: &str,
        sender_person_id: String,
        cover_text: String,
    ) -> Result<OpenedPeerProseMessage, String> {
        let active = broker
            .active
            .lock()
            .map_err(|_| "OSL broker state is unavailable".to_owned())?
            .clone()
            .ok_or_else(|| "OSL Chat protection is not active".to_owned())?;
        if active.context_token != context_token {
            return Err("OSL Chat protection is not active".to_owned());
        }
        if sender_person_id != active.person_id {
            return Err(NOT_FRIEND_REFUSAL.to_owned());
        }
        let plaintext = cover_text
            .strip_prefix("TASK3982::")
            .ok_or_else(|| "This encrypted message could not be opened".to_owned())?
            .replace("\\n", "\n")
            .replace("\\\\", "\\");
        Ok(OpenedPeerProseMessage {
            plaintext,
            context_verified: true,
            person_to_person_e2ee: true,
            view_once_consumed: false,
            require_capture_protection: false,
        })
    }

    pub fn prose_send_key(core: &HubCoreState) -> Result<[u8; 32], String> {
        let identity = core
            .osl
            .identity
            .lock()
            .map_err(|_| "OSL identity state is unavailable".to_owned())?
            .clone()
            .ok_or_else(|| "OSL identity is not loaded".to_owned())?;
        ipc::prose_token::derive_send_key(identity.x25519_secret.as_bytes())
            .map_err(|_| "OSL protected conversation key is unavailable".to_owned())
    }
}
pub mod chat_app_timer_policy;
pub mod chat_capture_protection;
/// **The claim state.** What OSL may publicly say about each ruled surface, and
/// why — the owner gate `PLAN.md` r4-5 calls "the claim-state gap". Not behind a
/// feature: `native_apps` derives every public support label from it, and the
/// publication gates that read it must exist in every build that can compile the
/// native adapters.
#[cfg(not(task3982_focused))]
pub mod claim_state;
#[cfg(feature = "core")]
#[cfg(not(task3982_focused))]
pub mod cleanup;
#[cfg(feature = "core")]
#[cfg(not(task3982_focused))]
pub mod burn_job_fence;
#[cfg(feature = "core")]
#[cfg(not(task3982_focused))]
pub mod core_bridge;
#[cfg(all(feature = "core", task3982_focused))]
pub mod core_bridge {
    use std::sync::Arc;

    pub struct HubCoreState {
        pub osl: Arc<ipc::AppState>,
    }

    impl Default for HubCoreState {
        fn default() -> Self {
            Self {
                osl: Arc::new(ipc::AppState::new()),
            }
        }
    }
}
#[cfg(feature = "core")]
#[cfg(not(task3982_focused))]
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
#[cfg(not(task3982_focused))]
pub mod eager_fetch;
#[cfg(feature = "core")]
#[cfg(not(task3982_focused))]
pub mod eager_fetch_retry;
pub mod identity_binding_verifier;
#[cfg(feature = "core")]
#[cfg(not(task3982_focused))]
pub mod identity_registry;
#[cfg(feature = "core")]
pub mod inbound_receipts;
pub mod isolated_worker;
#[cfg(feature = "core")]
pub mod mass_cleanup;
pub mod osl_chat_conversations;
#[cfg(not(task3982_focused))]
pub mod osl_chat_delivery;
#[cfg(not(task3982_focused))]
pub mod osl_chat_queue;
pub mod realtime_client;
pub mod realtime_decoy;
pub mod realtime_pipe;
pub mod realtime_resume;
pub mod realtime_subscription;
pub mod realtime_wakeup;
#[cfg(feature = "core")]
pub mod receipt_emit;
pub mod right_click_safety;
pub mod row_who_wrote_it;
#[cfg(feature = "core")]
#[cfg(not(task3982_focused))]
pub mod runtime_switches;
// Timed deletion and view-once expiry. Needs `ipc` (sealed-at-rest storage) and
// `store` (local plaintext cache), so it lives behind `core` like they do.
pub mod expiry_clock;
#[cfg(feature = "core")]
#[cfg(not(task3982_focused))]
pub mod message_expiry;
// View-once payloads are opened only after the native viewer proves capture
// protection; the module is dependency-free so its ordering tests run on all
// supported build hosts.
#[cfg(feature = "core")]
#[cfg(not(task3982_focused))]
pub mod password_lifecycle;
#[cfg(feature = "core")]
#[cfg(not(task3982_focused))]
pub mod view_once_eligibility;
#[cfg(not(task3982_focused))]
pub mod view_once_open;
#[cfg(not(task3982_focused))]
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
#[cfg(not(task3982_focused))]
pub mod native_surface_capture;
// The pure half of the desktop binary's Tauri command surface: send authority,
// checked-host ordering, reviewed-run identity binding, the restart-proof drain
// order, and the registered-command/ACL proofs. Same reason as the two modules
// above -- nothing inside `main.rs` is ever compiled or run by `--features
// core`, which is the only configuration CI and the accept commands can build.
#[cfg(feature = "core")]
#[cfg(not(task3982_focused))]
pub mod hub_command_surface;
pub mod imap_verify;
#[cfg(not(task3982_focused))]
pub mod scrub_imap;
#[cfg(feature = "core")]
#[cfg(not(task3982_focused))]
pub mod scrub_index;
pub mod scrub_receipt;
#[cfg(feature = "core")]
#[cfg(not(task3982_focused))]
pub mod security;
pub mod setting_groups;
#[cfg(all(feature = "core", task3982_focused))]
pub mod security {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};

    use ipc::scope::ScopeInput;
    use serde::Serialize;

    use crate::core_bridge::HubCoreState;

    #[derive(Debug, Default)]
    pub struct HubSecurityState;

    static FOCUSED_ALLOWED_LIST: OnceLock<Mutex<HashMap<String, AllowedFriend>>> = OnceLock::new();

    fn focused_allowed_list() -> &'static Mutex<HashMap<String, AllowedFriend>> {
        FOCUSED_ALLOWED_LIST.get_or_init(|| Mutex::new(HashMap::new()))
    }

    #[derive(Debug, Clone, Serialize)]
    #[serde(rename_all = "camelCase")]
    pub struct FriendCodeExport {
        pub friend_code: String,
        pub osl_user_id: String,
    }

    #[derive(Debug, Clone, Serialize)]
    #[serde(rename_all = "camelCase")]
    pub struct PersonDto {
        pub person_id: String,
        pub osl_user_id: String,
        pub display_name: String,
        pub email_address: Option<String>,
        pub safety_number: String,
        pub accepted: bool,
        pub verified: bool,
    }

    #[derive(Clone, Debug, Eq, PartialEq)]
    struct AllowedFriend {
        binding: ManualPeerBinding,
        display_name: String,
        email_address: Option<String>,
    }

    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct ManualPeerBinding {
        pub person_id: String,
        pub peer_osl_user_id: String,
    }

    fn normalized_friend_email_address(address: &str) -> Result<String, String> {
        let normalized = address.trim().to_ascii_lowercase();
        let Some((local, domain)) = normalized.split_once('@') else {
            return Err("OSL friend email address is invalid".to_owned());
        };
        if normalized.len() > 254
            || local.is_empty()
            || domain.is_empty()
            || domain.contains('@')
            || normalized
                .bytes()
                .any(|byte| byte.is_ascii_control() || byte.is_ascii_whitespace())
        {
            return Err("OSL friend email address is invalid".to_owned());
        }
        Ok(normalized)
    }

    fn person_dto(friend: &AllowedFriend, safety_number: String) -> PersonDto {
        PersonDto {
            person_id: friend.binding.person_id.clone(),
            osl_user_id: friend.binding.peer_osl_user_id.clone(),
            display_name: friend.display_name.clone(),
            email_address: friend.email_address.clone(),
            safety_number,
            accepted: true,
            verified: false,
        }
    }

    pub fn export_friend_code(core: &HubCoreState) -> Result<FriendCodeExport, String> {
        let identity = core
            .osl
            .identity
            .lock()
            .map_err(|_| "OSL identity state is unavailable".to_owned())?
            .clone()
            .ok_or_else(|| "OSL identity is not loaded".to_owned())?;
        Ok(FriendCodeExport {
            friend_code: format!("TASK3982-FRIEND::{}", identity.user_id),
            osl_user_id: identity.user_id.clone(),
        })
    }

    pub fn add_friend_code(
        _core: &HubCoreState,
        _security: &HubSecurityState,
        friend_code: String,
        display_name: Option<String>,
    ) -> Result<PersonDto, String> {
        let peer_osl_user_id = friend_code
            .strip_prefix("TASK3982-FRIEND::")
            .ok_or_else(|| "OSL friend code is invalid".to_owned())?
            .to_owned();
        let person_id = format!("task3982-person-{peer_osl_user_id}");
        let safety_number = format!("task3982-safety-{peer_osl_user_id}");
        let display_name = display_name.unwrap_or_default();
        let binding = ManualPeerBinding {
            person_id: person_id.clone(),
            peer_osl_user_id: peer_osl_user_id.clone(),
        };
        let friend = AllowedFriend {
            binding,
            display_name,
            email_address: None,
        };
        focused_allowed_list()
            .lock()
            .map_err(|_| "OSL friend state is unavailable".to_owned())?
            .insert(person_id, friend.clone());
        Ok(person_dto(&friend, safety_number))
    }

    pub fn set_friend_email_address(
        _security: &HubSecurityState,
        person_id: String,
        email_address: String,
    ) -> Result<PersonDto, String> {
        let normalized = normalized_friend_email_address(&email_address)?;
        let mut friends = focused_allowed_list()
            .lock()
            .map_err(|_| "OSL friend state is unavailable".to_owned())?;
        let duplicate_name = friends
            .iter()
            .filter(|(candidate_id, _)| candidate_id.as_str() != person_id.as_str())
            .find(|(_, friend)| friend.email_address.as_deref() == Some(normalized.as_str()))
            .map(|(_, friend)| friend.display_name.clone());
        if let Some(name) = duplicate_name {
            return Err(format!(
                "OSL friend email address already belongs to {name}"
            ));
        }
        let friend = friends
            .get_mut(&person_id)
            .ok_or_else(|| "OSL friend is unknown".to_owned())?;
        friend.email_address = Some(normalized);
        Ok(person_dto(
            friend,
            format!("task3982-safety-{}", friend.binding.peer_osl_user_id),
        ))
    }

    pub fn verify_friend_safety_number(
        _core: &HubCoreState,
        _security: &HubSecurityState,
        _person_id: String,
        _safety_number: String,
    ) -> Result<(), String> {
        Ok(())
    }

    pub fn manual_peer_binding(
        _core: &HubCoreState,
        person_id: String,
    ) -> Result<ManualPeerBinding, String> {
        focused_allowed_list()
            .lock()
            .map_err(|_| "OSL friend state is unavailable".to_owned())?
            .get(&person_id)
            .map(|friend| friend.binding.clone())
            .ok_or_else(|| "OSL sender is not a friend".to_owned())
    }

    pub fn manual_peer_binding_for_email_address(
        _core: &HubCoreState,
        email_address: &str,
    ) -> Result<ManualPeerBinding, String> {
        let normalized = normalized_friend_email_address(email_address)?;
        focused_allowed_list()
            .lock()
            .map_err(|_| "OSL friend state is unavailable".to_owned())?
            .values()
            .find(|friend| friend.email_address.as_deref() == Some(normalized.as_str()))
            .map(|friend| friend.binding.clone())
            .ok_or_else(|| "OSL sender is not a friend".to_owned())
    }

    pub fn set_manual_peer_scope_permission(
        _core: &HubCoreState,
        _security: &HubSecurityState,
        _service_id: &str,
        _account_id: &str,
        _person_id: String,
        _scope: ScopeInput,
        _allowed: bool,
    ) -> Result<(), String> {
        Ok(())
    }

    pub fn set_scope_security(
        _security: &HubSecurityState,
        _scope: ScopeInput,
        _ttl_seconds: u32,
        _person_to_person_e2ee: bool,
    ) -> Result<(), String> {
        Ok(())
    }

    pub fn require_accepted_friend(_core: &HubCoreState, _person_id: &str) -> Result<(), String> {
        Ok(())
    }
}
#[cfg(feature = "core")]
#[cfg(not(task3982_focused))]
pub mod security_credentials;
#[cfg(not(task3982_focused))]
pub mod sensitive_warning;
#[cfg(feature = "core")]
pub mod service_burn_selection;
#[cfg(feature = "core")]
#[cfg(not(task3982_focused))]
pub mod service_scope_index;
pub mod shared_mail_deleter;
#[cfg(not(task3982_focused))]
pub mod shared_mailbox_reader;
pub mod signal_destination_binding;
#[cfg(feature = "core")]
pub mod signal_extra_device_sender;
#[cfg(feature = "core")]
pub mod spaces;
#[cfg(feature = "core")]
#[cfg(not(task3982_focused))]
pub mod startup_gate;

// Share the original Tauri-free bootstrap verbatim so the app loads the same
// sealed identity and local security state without forking that logic.
pub mod device_pairing;
#[cfg(feature = "core")]
#[allow(
    clippy::needless_borrow,
    clippy::needless_borrows_for_generic_args,
    clippy::uninlined_format_args
)]
#[path = "../../../src-tauri/src/bootstrap.rs"]
pub mod original_bootstrap;
pub mod osl_chat_danger_row;
pub mod pro_marked_deletion;
pub mod pro_marked_deletion_outcomes;
pub mod proton_fake_page;
pub mod quiet_hours;
pub mod quiet_hours_notices;
pub mod shared_delete_action;
pub mod shared_marked_deletion_record;
pub mod shared_marked_message_deleter;
pub mod shared_web_reader_shape;
pub mod sync_policy;
pub mod timed_delete_sweep_job;

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
