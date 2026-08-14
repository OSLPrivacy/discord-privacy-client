//! The catalogue of OSL-central moderation systems that must not exist.
//!
//! The 11 August 2026 owner ruling permits an Enclave to govern itself and
//! forbids OSL from governing anybody. The difference is not a matter of
//! wording, so this file is the machine-readable form of the forbidden half:
//! every identifier a central reporting, banning, evidence, review, appeal,
//! moderator-tooling, global-block-list, reputation or server-readable-content
//! path would have to introduce.
//!
//! ## Why this list lives alone in its own file
//!
//! The sweep in `bin/task_6594_inventory.rs` reads these strings and then
//! searches the feature's real source and UI files for them. A catalogue of
//! forbidden strings necessarily *contains* every forbidden string, so the one
//! file that declares them is the single excluded path in that sweep and the
//! inventory names it in its output. Nothing else is excluded, and no other
//! file may declare a needle.

/// One forbidden central system and the identifiers that would build it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CentralSystem {
    /// The system's name, used verbatim when the sweep names a leak.
    pub system: &'static str,
    /// Lower-case identifiers searched for in a case-insensitive sweep.
    pub needles: &'static [&'static str],
}

/// The path this catalogue occupies, relative to the workspace root. The
/// sweep excludes exactly this file and reports the exclusion.
pub const CATALOGUE_PATH: &str = "crates/ipc/src/central_moderation_needles.rs";

/// Every OSL-central moderation system the product must not contain.
pub const FORBIDDEN_CENTRAL_SYSTEMS: [CentralSystem; 10] = [
    CentralSystem {
        system: "osl-report",
        needles: &[
            "report_message",
            "reportmessage",
            "report_to_osl",
            "report_button",
        ],
    },
    CentralSystem {
        system: "abuse-inbox",
        needles: &["abuse_inbox", "abuseinbox", "abuse_queue"],
    },
    CentralSystem {
        system: "osl-ban-or-suspension",
        needles: &[
            "osl_ban",
            "global_ban",
            "globalban",
            "account_suspension",
            "suspend_account",
        ],
    },
    CentralSystem {
        system: "evidence-collection",
        needles: &["evidence_record", "evidence_locker", "evidence_collection"],
    },
    CentralSystem {
        system: "content-review",
        needles: &["content_review", "moderation_queue", "review_queue"],
    },
    CentralSystem {
        system: "appeals",
        needles: &["appeal_process", "appeals_inbox", "submit_appeal"],
    },
    CentralSystem {
        system: "osl-moderator-tooling",
        needles: &["moderator_tool", "osl_moderator", "mod_console"],
    },
    CentralSystem {
        system: "global-block-list",
        needles: &["global_block_list", "globalblocklist", "global_blocklist"],
    },
    CentralSystem {
        system: "cross-enclave-reputation",
        needles: &[
            "reputation_score",
            "cross_enclave_reputation",
            "user_reputation",
        ],
    },
    CentralSystem {
        system: "server-readable-content-path",
        needles: &[
            "server_readable_content",
            "plaintext_content_path",
            "read_content_for_moderation",
        ],
    },
];

/// Every needle across every system, flattened.
pub fn all_needles() -> Vec<(&'static str, &'static str)> {
    FORBIDDEN_CENTRAL_SYSTEMS
        .iter()
        .flat_map(|entry| {
            entry
                .needles
                .iter()
                .map(move |needle| (entry.system, *needle))
        })
        .collect()
}
