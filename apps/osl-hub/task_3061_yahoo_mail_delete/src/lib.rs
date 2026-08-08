//! TASK 3061 check crate: the hub's own sources, compiled through `#[path]`.
//!
//! Nothing here re-implements the module under test. The four `#[path]`
//! declarations below name the exact files `apps/osl-hub/src/lib.rs` declares,
//! so a change to any of them changes what this crate builds:
//!
//! * `scrub_hosted/yahoo_mail_delete.rs` — TASK 3061, the Yahoo fill-in;
//! * `shared_mail_deleter.rs` — gate 3045's shared mail deleter, which it calls;
//! * `mail_owner_check.rs` — gate 3044's owner check, which that calls;
//! * `row_who_wrote_it.rs` — the only module `mail_owner_check` needs.

#[path = "../../src/mail_owner_check.rs"]
pub mod mail_owner_check;
#[path = "../../src/row_who_wrote_it.rs"]
pub mod row_who_wrote_it;
#[path = "../../src/shared_mail_deleter.rs"]
pub mod shared_mail_deleter;

// Declared flat rather than under an inline `mod scrub_hosted`: an inline module
// resolves `#[path]` against a `src/scrub_hosted/` directory that does not exist
// here, and rustc's `..` walk needs a real one. The file is the same file.
#[path = "../../src/scrub_hosted/yahoo_mail_delete.rs"]
pub mod yahoo_mail_delete;

use yahoo_mail_delete::{
    YahooMailRow, YahooMailTrashSurface, YAHOO_MAIL_SENT_FOLDER_ID, YAHOO_MAIL_TRASH_FOLDER_ID,
};

pub const SIGNED_IN_ADDRESS: &str = "owner@yahoo.example.test";
pub const MARKER: &str = "SCRUB-YH-DEL";
pub const MARKED_MESSAGE: &str = "sent-yh-3061-002";
pub const UNRELATED_TRASH_MESSAGE: &str = "trash-yh-3061-was-already-here";

/// The seeded Yahoo mailbox TASK 3061 names: Sent holds three messages matching
/// `SCRUB-YH-DEL`, and Trash holds one unrelated message the user binned earlier.
pub fn seeded_yahoo_surface() -> YahooMailTrashSurface {
    YahooMailTrashSurface::new([
        YahooMailRow::new(
            YAHOO_MAIL_SENT_FOLDER_ID,
            "sent-yh-3061-001",
            "SCRUB-YH-DEL renewal receipt",
            SIGNED_IN_ADDRESS,
        ),
        YahooMailRow::new(
            YAHOO_MAIL_SENT_FOLDER_ID,
            MARKED_MESSAGE,
            "SCRUB-YH-DEL address confirmation",
            SIGNED_IN_ADDRESS,
        ),
        YahooMailRow::new(
            YAHOO_MAIL_SENT_FOLDER_ID,
            "sent-yh-3061-003",
            "SCRUB-YH-DEL travel plan",
            SIGNED_IN_ADDRESS,
        ),
        YahooMailRow::new(
            YAHOO_MAIL_TRASH_FOLDER_ID,
            UNRELATED_TRASH_MESSAGE,
            "Yahoo newsletter binned last week",
            "news@example.test",
        ),
    ])
}
