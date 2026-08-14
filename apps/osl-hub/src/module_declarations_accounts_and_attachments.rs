// Accounts, access policy, and attachment lifecycle declarations.
// Append account/access or attachment work here.
//
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
