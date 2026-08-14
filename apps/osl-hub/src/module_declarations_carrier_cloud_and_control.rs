// Carrier contracts, cloud cleanup, controls, and diagnostics declarations.
// Append related work here.
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
pub mod coach_tips;
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
pub mod discord_typing_box_check;
pub mod email_timer_contract;
pub mod english_catalogue_entry;
#[cfg(feature = "desktop")]
pub mod entitlement_refresh;
pub mod execution_consent;
pub mod external_overlay;
pub(crate) mod firefox_migration_coordinator;
#[cfg(feature = "core")]
pub mod friend_account_reach;
pub mod front_window_grab;
