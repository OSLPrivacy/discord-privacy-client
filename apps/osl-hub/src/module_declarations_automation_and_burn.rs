// AI, automation, browser, and burn declarations. Append related work here.
//
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
pub mod automod_filter_bundle;
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
