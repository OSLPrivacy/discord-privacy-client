//! Named audit cells where OSL-protected data can sit at rest.
//!
//! This type deliberately carries no path, account, peer, scope, message, or
//! other caller-provided identifier. It is a classifier for at-rest audits, not
//! an event payload.

use core::fmt;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum AtRestBoundary {
    /// Device-sealed local identity material, usually represented by
    /// `identity.json`.
    SealedIdentity,
    /// Local account-scoped state files such as peer, membership, scope,
    /// preference, and sender-key state.
    AccountStateFiles,
    /// Durable message history and message metadata.
    MessageStore,
    /// Temporary or cached attachment bytes staged outside the message store.
    AttachmentStaging,
    /// Renderer-side browser storage such as localStorage or IndexedDB.
    UiLocalStorage,
    /// Import, backup, staging, and rollback copies retained during account
    /// replacement or recovery.
    BackupRollbackCopies,
    /// Bytes persisted by the operating system, filesystem, swap, crash dumps,
    /// removable drives, or other physical media below the application layer.
    PhysicalMedia,
}

impl AtRestBoundary {
    pub const ALL: [Self; 7] = [
        Self::SealedIdentity,
        Self::AccountStateFiles,
        Self::MessageStore,
        Self::AttachmentStaging,
        Self::UiLocalStorage,
        Self::BackupRollbackCopies,
        Self::PhysicalMedia,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SealedIdentity => "sealed_identity",
            Self::AccountStateFiles => "account_state_files",
            Self::MessageStore => "message_store",
            Self::AttachmentStaging => "attachment_staging",
            Self::UiLocalStorage => "ui_local_storage",
            Self::BackupRollbackCopies => "backup_rollback_copies",
            Self::PhysicalMedia => "physical_media",
        }
    }
}

impl fmt::Debug for AtRestBoundary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl fmt::Display for AtRestBoundary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_covers_every_named_at_rest_boundary() {
        let all = AtRestBoundary::ALL;

        assert!(all.contains(&AtRestBoundary::SealedIdentity));
        assert!(all.contains(&AtRestBoundary::AccountStateFiles));
        assert!(all.contains(&AtRestBoundary::MessageStore));
        assert!(all.contains(&AtRestBoundary::AttachmentStaging));
        assert!(all.contains(&AtRestBoundary::UiLocalStorage));
        assert!(all.contains(&AtRestBoundary::BackupRollbackCopies));
        assert!(all.contains(&AtRestBoundary::PhysicalMedia));
        assert_eq!(all.len(), 7);
    }

    #[test]
    fn debug_and_display_are_static_boundary_names_only() {
        let forbidden_fragments = [
            "/",
            "\\",
            ".json",
            "peer_map",
            "scope_membership",
            "123456789012345678",
            "alice@example.com",
            "dm:",
            "server_channel:",
        ];

        for boundary in AtRestBoundary::ALL {
            let debug = format!("{boundary:?}");
            let display = boundary.to_string();

            assert_eq!(debug, boundary.as_str());
            assert_eq!(display, boundary.as_str());
            for fragment in forbidden_fragments {
                assert!(
                    !debug.contains(fragment),
                    "Debug output leaked forbidden fragment {fragment:?}: {debug}"
                );
                assert!(
                    !display.contains(fragment),
                    "Display output leaked forbidden fragment {fragment:?}: {display}"
                );
            }
        }
    }
}
