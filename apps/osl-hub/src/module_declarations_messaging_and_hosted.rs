// Messaging, hosted/provider, mail, and place declarations. Append related work here.
/// TASK 6832: real GIF messages — provider search and direct-file picks fetched
/// through the client privacy proxy, tracker-stripped, then encrypted as
/// ordinary attachments under the same channel keys. Pure above its transport
/// seam, so the whole contract is testable in every build that can compile this
/// crate.
pub mod gif_message;
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
pub mod enclave_key_lifecycle;
pub mod mail_owner_check;
/// When the hidden main window may be shown. Pure, and deliberately not behind
/// `desktop`: the reveal rule is what decides whether the app is visible at all,
/// so it is testable in every build that can compile this crate.
pub mod main_window_reveal;
pub mod messenger_cover_placement;
pub mod messenger_whitelist_kinds;
// Signed, recipient-bound invitations are deliberately separate from the
// general membership writer.  An invite is only an offer until the intended
// friend accepts its exact signed record.
pub mod model_pack_install;
pub mod models;
pub mod mullvad_window_host;
pub mod named_places;
pub mod place_membership_invites;
