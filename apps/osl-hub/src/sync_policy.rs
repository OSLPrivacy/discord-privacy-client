#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SyncRefusalKind {
    pub kind: &'static str,
    pub reason: &'static str,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SyncPayloadCheckError {
    Refused {
        kind: &'static str,
        reason: &'static str,
    },
    UnknownKind {
        kind: String,
    },
}

pub const REFUSED_SYNC_KINDS: [SyncRefusalKind; 6] = [
    SyncRefusalKind {
        kind: "account_passwords",
        reason: "They unlock the local store, and copying them widens what one stolen machine costs.",
    },
    SyncRefusalKind {
        kind: "machine_sealed_material",
        reason: "A machine's own chip seals it, so it cannot be moved and pretending it can gives the person a broken device.",
    },
    SyncRefusalKind {
        kind: "message_protection_session_state",
        reason: "Two devices sharing one session break the message counter and either lose messages or reuse a key.",
    },
    SyncRefusalKind {
        kind: "carrier_sign_ins",
        reason: "Moving a Discord or mail session to another machine trips that service's own theft checks and gets the account locked.",
    },
    SyncRefusalKind {
        kind: "attachment_contents",
        reason: "Attachments are fetched when wanted, and copying them multiplies the data bill for nothing.",
    },
    SyncRefusalKind {
        kind: "scrub_findings",
        reason: "Scrub findings describe the machine they were found on.",
    },
];

pub const ALLOWED_SYNC_KINDS: [&str; 12] = [
    "identity_public_profile",
    "friend_roster",
    "peer_public_key_bundles",
    "safety_number_pins",
    "conversation_membership",
    "server_whitelist_rules",
    "channel_whitelist_rules",
    "message_metadata",
    "encrypted_message_records",
    "attachment_pointers",
    "burn_marker_records",
    "timed_delete_receipts",
];

pub fn refused_sync_kinds() -> &'static [SyncRefusalKind] {
    &REFUSED_SYNC_KINDS
}

pub fn allowed_sync_kinds() -> &'static [&'static str] {
    &ALLOWED_SYNC_KINDS
}

pub fn check_sync_payload_before_send<'a>(
    kinds: impl IntoIterator<Item = &'a str>,
) -> Result<(), SyncPayloadCheckError> {
    for kind in kinds {
        if let Some(refused) = REFUSED_SYNC_KINDS
            .iter()
            .find(|refused| refused.kind == kind)
        {
            return Err(SyncPayloadCheckError::Refused {
                kind: refused.kind,
                reason: refused.reason,
            });
        }
        if !ALLOWED_SYNC_KINDS.contains(&kind) {
            return Err(SyncPayloadCheckError::UnknownKind {
                kind: kind.to_owned(),
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::{
        allowed_sync_kinds, check_sync_payload_before_send, refused_sync_kinds,
        SyncPayloadCheckError,
    };

    #[test]
    fn task_4811_sync_policy_lists_exact_refusals_and_refuses_before_send() {
        let refused = refused_sync_kinds();
        let allowed = allowed_sync_kinds();

        println!("TASK4811_REFUSED_COUNT={}", refused.len());
        println!("TASK4811_ALLOWED_COUNT={}", allowed.len());

        assert_eq!(
            refused.len(),
            6,
            "there must be exactly six refused sync kinds"
        );
        assert!(
            allowed.len() >= 8,
            "allowed side must name at least eight sync kinds"
        );

        let mut refused_seen = HashSet::new();
        for item in refused {
            println!("TASK4811_REFUSED kind={} reason={}", item.kind, item.reason);
            assert!(
                !item.kind.trim().is_empty(),
                "refused kind must not be blank"
            );
            assert!(
                !item.reason.trim().is_empty(),
                "refused reason must not be blank"
            );
            assert!(
                refused_seen.insert(item.kind),
                "refused kind must be unique: {}",
                item.kind
            );
            assert_eq!(
                item.reason.matches('.').count(),
                1,
                "each refused kind must have exactly one written reason: {}",
                item.kind
            );
        }

        let mut allowed_seen = HashSet::new();
        for kind in allowed {
            println!("TASK4811_ALLOWED kind={kind}");
            assert!(!kind.trim().is_empty(), "allowed kind must not be blank");
            assert!(
                allowed_seen.insert(*kind),
                "allowed kind must be unique: {kind}"
            );
            assert!(
                !refused_seen.contains(kind),
                "kind appears on both allowed and refused sides: {kind}"
            );
        }

        let mut refusal_attempts = 0;
        for item in refused {
            match check_sync_payload_before_send([item.kind]) {
                Err(SyncPayloadCheckError::Refused { kind, reason }) => {
                    refusal_attempts += 1;
                    println!("TASK4811_REFUSAL_ATTEMPT kind={kind} sent=0 reason={reason}");
                    assert_eq!(kind, item.kind);
                    assert_eq!(reason, item.reason);
                }
                other => panic!("refused kind did not stop before send: {other:?}"),
            }
        }
        println!("TASK4811_REFUSAL_ATTEMPTS={refusal_attempts}");
        assert_eq!(refusal_attempts, 6);

        check_sync_payload_before_send(allowed.iter().copied())
            .expect("known allowed sync kinds pass the pre-send check");
        println!(
            "TASK4811_ALLOWED_PAYLOAD sent=1 kinds={}",
            allowed.join("|")
        );

        match check_sync_payload_before_send(["future_unclassified_kind"]) {
            Err(SyncPayloadCheckError::UnknownKind { kind }) => {
                println!("TASK4811_UNKNOWN_KIND kind={kind}");
                assert_eq!(kind, "future_unclassified_kind");
            }
            other => panic!("unknown kind must be named as neither allowed nor refused: {other:?}"),
        }
    }
}
