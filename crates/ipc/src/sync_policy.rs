//! Device-pairing sync classification.
//!
//! This is an allow-list. A new payload kind is not synchronised merely
//! because it is absent from the refused list: it must be named below first.
//! The pairing boundary is deliberately represented as two separate kinds so
//! that an old message cannot quietly become an ordinary sync payload.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SyncRefusalKind {
    pub kind: &'static str,
    pub reason: &'static str,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SyncRoute {
    Automatic,
    /// The sole non-automatic route for old messages. Its caller must show
    /// `COPY_MY_HISTORY_HERE_WARNING` before it sends anything.
    CopyMyHistoryHere,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SyncPayloadCheckError {
    Refused {
        kind: &'static str,
        reason: &'static str,
    },
    MissingHistoryCopyWarning,
    UnknownKind {
        kind: String,
    },
}

pub const COPY_MY_HISTORY_HERE_ACTION: &str = "Copy my history here";
pub const COPY_MY_HISTORY_HERE_WARNING: &str = "This puts your history in one more place.";

/// Exactly seven kinds are automatically refused. Keep the paired password
/// roles together: they all unlock this machine's local security boundary.
pub const REFUSED_SYNC_KINDS: &[SyncRefusalKind] = &[
    SyncRefusalKind {
        kind: "account_and_burn_or_stealth_passwords",
        reason: "Account, burn, and stealth passwords stay local because copying them widens what one stolen machine costs.",
    },
    SyncRefusalKind {
        kind: "chip_sealed_material",
        reason: "Chip-sealed material cannot move because it is bound to this machine's hardware protection.",
    },
    SyncRefusalKind {
        kind: "message_protection_session_state",
        reason: "Message-protection session state cannot be shared without counter loss or key reuse.",
    },
    SyncRefusalKind {
        kind: "carrier_sign_ins",
        reason: "Carrier sign-ins can trigger the carrier's theft checks when moved to another machine.",
    },
    SyncRefusalKind {
        kind: "attachment_contents",
        reason: "Attachment contents are fetched when wanted instead of being copied automatically.",
    },
    SyncRefusalKind {
        kind: "scrub_findings",
        reason: "Scrub findings describe one machine, not a portable account fact.",
    },
    SyncRefusalKind {
        kind: "pre_pairing_message_history",
        reason: "Pre-pairing message history was not addressed to the new device and moves only through the explicit Copy my history here action after its warning.",
    },
];

/// These kinds are the automatic side of the pairing boundary. Every named
/// kind in this module occurs on exactly one side of the classification.
pub const ALLOWED_SYNC_KINDS: &[&str] = &[
    "identity_public_profile",
    "friend_roster",
    "peer_public_key_bundles",
    "safety_number_pins",
    "conversation_membership",
    "server_whitelist_rules",
    "channel_whitelist_rules",
    "message_metadata",
    "attachment_pointers",
    "post_pairing_messages",
];

pub fn refused_sync_kinds() -> &'static [SyncRefusalKind] {
    REFUSED_SYNC_KINDS
}

pub fn allowed_sync_kinds() -> &'static [&'static str] {
    ALLOWED_SYNC_KINDS
}

/// Classify every payload item before its transport is allowed to send.
///
/// `pre_pairing_message_history` is refused on the automatic route. The
/// history-copy route is intentionally narrower: it can carry that one kind,
/// and only after the exact warning was acknowledged.
pub fn check_sync_payload_before_send<'a>(
    route: SyncRoute,
    warning_acknowledged: bool,
    kinds: impl IntoIterator<Item = &'a str>,
) -> Result<(), SyncPayloadCheckError> {
    for kind in kinds {
        if kind == "pre_pairing_message_history" && route == SyncRoute::CopyMyHistoryHere {
            if warning_acknowledged {
                continue;
            }
            return Err(SyncPayloadCheckError::MissingHistoryCopyWarning);
        }
        if let Some(refused) = REFUSED_SYNC_KINDS
            .iter()
            .find(|refused| refused.kind == kind)
        {
            return Err(SyncPayloadCheckError::Refused {
                kind: refused.kind,
                reason: refused.reason,
            });
        }
        if !ALLOWED_SYNC_KINDS.contains(&kind) || route == SyncRoute::CopyMyHistoryHere {
            return Err(SyncPayloadCheckError::UnknownKind {
                kind: kind.to_owned(),
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn task_4811_policy_is_complete_and_refuses_before_send() {
        let refused = refused_sync_kinds();
        let allowed = allowed_sync_kinds();
        println!("TASK4811_REFUSED_COUNT={}", refused.len());
        println!("TASK4811_ALLOWED_COUNT={}", allowed.len());
        assert_eq!(refused.len(), 7);
        assert!(allowed.len() >= 9);

        let mut names = HashSet::new();
        for item in refused {
            println!("TASK4811_REFUSED kind={} reason={}", item.kind, item.reason);
            assert!(!item.kind.trim().is_empty());
            assert!(!item.reason.trim().is_empty());
            assert!(
                names.insert(item.kind),
                "kind appears on both sides: {}",
                item.kind
            );
        }
        for kind in allowed {
            println!("TASK4811_ALLOWED kind={kind}");
            assert!(names.insert(*kind), "kind appears on both sides: {kind}");
        }

        let mut attempts = 0;
        for item in refused {
            match check_sync_payload_before_send(SyncRoute::Automatic, false, [item.kind]) {
                Err(SyncPayloadCheckError::Refused { kind, reason }) => {
                    attempts += 1;
                    println!("TASK4811_REFUSAL_ATTEMPT kind={kind} sent=0 reason={reason}");
                    assert_eq!(kind, item.kind);
                    assert_eq!(reason, item.reason);
                }
                other => panic!("refused kind did not stop before send: {other:?}"),
            }
        }
        println!("TASK4811_REFUSAL_ATTEMPTS={attempts}");
        assert_eq!(attempts, 7);

        check_sync_payload_before_send(SyncRoute::Automatic, false, allowed.iter().copied())
            .unwrap();
        match check_sync_payload_before_send(
            SyncRoute::Automatic,
            false,
            ["future_unclassified_kind"],
        ) {
            Err(SyncPayloadCheckError::UnknownKind { kind }) => {
                println!("TASK4811_UNKNOWN_KIND kind={kind}")
            }
            other => panic!("unclassified kind did not fail closed: {other:?}"),
        }
        assert!(matches!(
            check_sync_payload_before_send(
                SyncRoute::CopyMyHistoryHere,
                false,
                ["pre_pairing_message_history"]
            ),
            Err(SyncPayloadCheckError::MissingHistoryCopyWarning)
        ));
        check_sync_payload_before_send(
            SyncRoute::CopyMyHistoryHere,
            true,
            ["pre_pairing_message_history"],
        )
        .unwrap();
        println!("TASK4811_HISTORY_COPY action={COPY_MY_HISTORY_HERE_ACTION:?} warning={COPY_MY_HISTORY_HERE_WARNING:?} admitted=pre_pairing_message_history");
    }
}
