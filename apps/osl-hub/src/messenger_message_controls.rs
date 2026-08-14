//! Messenger-specific timer, view-once, and burn target commands.
//!
//! These commands are deliberately pure. The browser/desktop boundary supplies
//! the protected-message records it has already authenticated, and this module
//! returns the exact expiry or deletion targets that the side-effecting layer
//! may act on. Keeping target selection here makes the ownership check common
//! to all three burn choices.

use std::collections::{BTreeMap, BTreeSet};

/// Messenger per-message timers are bounded to thirty days.
pub const MESSENGER_MAX_TIMER_SECONDS: u32 = 30 * 24 * 60 * 60;
/// A Messenger view-once reveal may remain visible for at most one minute.
pub const MESSENGER_MAX_VIEW_ONCE_SECONDS: u32 = 60;

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct MessengerExpiryDto {
    pub mode: &'static str,
    pub lifetime_seconds: u32,
    pub expires_at_unix_seconds: u64,
}

/// Return the absolute expiry carried by a Messenger timed message.
pub fn cmd_osl_messenger_timer_expiry(
    now_unix_seconds: u64,
    lifetime_seconds: u32,
) -> Result<MessengerExpiryDto, String> {
    checked_expiry(
        "timer",
        now_unix_seconds,
        lifetime_seconds,
        MESSENGER_MAX_TIMER_SECONDS,
        "31-days",
    )
}

/// Return the absolute expiry carried by a Messenger view-once message.
pub fn cmd_osl_messenger_view_once_expiry(
    now_unix_seconds: u64,
    display_seconds: u32,
) -> Result<MessengerExpiryDto, String> {
    checked_expiry(
        "view-once",
        now_unix_seconds,
        display_seconds,
        MESSENGER_MAX_VIEW_ONCE_SECONDS,
        "61-seconds",
    )
}

fn checked_expiry(
    mode: &'static str,
    now_unix_seconds: u64,
    lifetime_seconds: u32,
    maximum_seconds: u32,
    refusal_name: &'static str,
) -> Result<MessengerExpiryDto, String> {
    if lifetime_seconds == 0 || lifetime_seconds > maximum_seconds {
        return Err(format!("OSL: Messenger request refused: {refusal_name}"));
    }
    let expires_at_unix_seconds = now_unix_seconds
        .checked_add(u64::from(lifetime_seconds))
        .ok_or_else(|| "OSL: Messenger expiry overflow".to_owned())?;
    Ok(MessengerExpiryDto {
        mode,
        lifetime_seconds,
        expires_at_unix_seconds,
    })
}

/// One protected row reported by the Messenger receiving/sender-record layer.
///
/// `owner_osl_user_id` binds the row to the signed-in OSL identity;
/// `messenger_account_id` binds it to the active browser profile. Both must
/// match in addition to `authored_by_self` before a burn command may return it.
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct MessengerProtectedMessageRecord {
    pub owner_osl_user_id: String,
    pub messenger_account_id: String,
    pub conversation_id: String,
    pub messenger_message_id: String,
    pub authored_by_self: bool,
    pub created_at_unix_ms: i64,
}

#[derive(Clone, Copy, Debug)]
pub struct MessengerBurnRequest<'a> {
    pub owner_osl_user_id: &'a str,
    pub messenger_account_id: &'a str,
    pub conversation_id: &'a str,
    /// Empty means every owned message in the named conversation. Non-empty
    /// means an exact reviewed subset; every requested id must be owned.
    pub requested_message_ids: &'a [&'a str],
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct MessengerBurnTargetsDto {
    pub choice: &'static str,
    pub owned_target_ids: Vec<String>,
    pub local_target_ids: Vec<String>,
    pub remote_target_ids: Vec<String>,
}

/// Select only sender-owned Messenger records for deletion on this device.
pub fn cmd_osl_messenger_burn_your_side(
    records: &[MessengerProtectedMessageRecord],
    request: MessengerBurnRequest<'_>,
) -> Result<MessengerBurnTargetsDto, String> {
    let targets = select_owned_targets(records, request)?;
    Ok(MessengerBurnTargetsDto {
        choice: "your-side",
        owned_target_ids: targets.clone(),
        local_target_ids: targets,
        remote_target_ids: Vec::new(),
    })
}

/// Select only sender-owned Messenger wrapped-key records for remote deletion.
pub fn cmd_osl_messenger_burn_their_side(
    records: &[MessengerProtectedMessageRecord],
    request: MessengerBurnRequest<'_>,
) -> Result<MessengerBurnTargetsDto, String> {
    let targets = select_owned_targets(records, request)?;
    Ok(MessengerBurnTargetsDto {
        choice: "their-side",
        owned_target_ids: targets.clone(),
        local_target_ids: Vec::new(),
        remote_target_ids: targets,
    })
}

/// Select the same owned Messenger ids for coordinated local and remote burn.
pub fn cmd_osl_messenger_burn_both_sides(
    records: &[MessengerProtectedMessageRecord],
    request: MessengerBurnRequest<'_>,
) -> Result<MessengerBurnTargetsDto, String> {
    let targets = select_owned_targets(records, request)?;
    Ok(MessengerBurnTargetsDto {
        choice: "both-sides",
        owned_target_ids: targets.clone(),
        local_target_ids: targets.clone(),
        remote_target_ids: targets,
    })
}

fn select_owned_targets(
    records: &[MessengerProtectedMessageRecord],
    request: MessengerBurnRequest<'_>,
) -> Result<Vec<String>, String> {
    if request.owner_osl_user_id.trim().is_empty()
        || request.messenger_account_id.trim().is_empty()
        || request.conversation_id.trim().is_empty()
    {
        return Err("OSL: Messenger burn scope is missing".to_owned());
    }

    // A map gives deterministic creation-time ordering and rejects ambiguous
    // duplicate fixture/store rows instead of choosing one by accident.
    let mut in_scope = BTreeMap::<&str, (&MessengerProtectedMessageRecord, bool)>::new();
    for record in records.iter().filter(|record| {
        record.owner_osl_user_id == request.owner_osl_user_id
            && record.messenger_account_id == request.messenger_account_id
            && record.conversation_id == request.conversation_id
    }) {
        let duplicate = in_scope
            .insert(
                record.messenger_message_id.as_str(),
                (record, record.authored_by_self),
            )
            .is_some();
        if duplicate {
            return Err("OSL: Messenger burn records are ambiguous".to_owned());
        }
    }

    let requested: BTreeSet<&str> = request.requested_message_ids.iter().copied().collect();
    if requested.len() != request.requested_message_ids.len()
        || requested.iter().any(|id| id.trim().is_empty())
    {
        return Err("OSL: Messenger burn request is malformed".to_owned());
    }

    if !requested.is_empty()
        && requested.iter().any(|id| {
            !in_scope
                .get(id)
                .is_some_and(|(_, authored_by_self)| *authored_by_self)
        })
    {
        return Err("OSL: Messenger request refused: another-person-delete".to_owned());
    }

    let mut targets = in_scope
        .values()
        .filter_map(|(record, authored_by_self)| {
            (*authored_by_self
                && (requested.is_empty()
                    || requested.contains(record.messenger_message_id.as_str())))
            .then_some((
                record.created_at_unix_ms,
                record.messenger_message_id.clone(),
            ))
        })
        .collect::<Vec<_>>();
    targets.sort_by(|left, right| left.cmp(right));
    Ok(targets.into_iter().map(|(_, id)| id).collect())
}
