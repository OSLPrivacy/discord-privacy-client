//! Lifecycle commands for protected Signal messages.
//!
//! Signal's desktop surface is placement-only, so these commands operate on
//! OSL's protected-message records rather than pretending to drive Signal's
//! own disappearing-message controls.  Every command proves ownership before
//! it changes a row.  A multi-target burn validates the entire selection first,
//! making a mixed owned/unowned selection an atomic refusal.

use std::collections::BTreeMap;

/// Signal timer requests are accepted up to thirty days.
pub const MAX_SIGNAL_TIMER_SECONDS: u64 = 30 * 24 * 60 * 60;
/// A view-once viewer may stay open for one through sixty seconds.
pub const MAX_SIGNAL_VIEW_ONCE_SECONDS: u64 = 60;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SignalProtectedMessage {
    pub message_id: String,
    pub owner_id: String,
    pub local_present: bool,
    pub remote_present: bool,
    pub expiry_at_unix_seconds: Option<u64>,
    pub view_once: bool,
}

impl SignalProtectedMessage {
    pub fn new(message_id: impl Into<String>, owner_id: impl Into<String>) -> Self {
        Self {
            message_id: message_id.into(),
            owner_id: owner_id.into(),
            local_present: true,
            remote_present: true,
            expiry_at_unix_seconds: None,
            view_once: false,
        }
    }
}

#[derive(Default)]
pub struct SignalProtectedMessageStore {
    rows: BTreeMap<String, SignalProtectedMessage>,
}

impl SignalProtectedMessageStore {
    pub fn insert(&mut self, row: SignalProtectedMessage) -> Result<(), String> {
        if row.message_id.trim().is_empty() || row.owner_id.trim().is_empty() {
            return Err("Signal protected row needs an id and owner".to_owned());
        }
        if self.rows.insert(row.message_id.clone(), row).is_some() {
            return Err("Signal protected row already exists".to_owned());
        }
        Ok(())
    }

    pub fn get(&self, message_id: &str) -> Option<&SignalProtectedMessage> {
        self.rows.get(message_id)
    }

    fn owned_row_mut(
        &mut self,
        actor_id: &str,
        message_id: &str,
    ) -> Result<&mut SignalProtectedMessage, String> {
        require_identity(actor_id)?;
        let row = self
            .rows
            .get_mut(message_id)
            .ok_or_else(|| format!("Signal protected message {message_id} is missing"))?;
        if row.owner_id != actor_id {
            return Err(format!(
                "Signal protected message {message_id} is not owned by {actor_id}"
            ));
        }
        Ok(row)
    }

    fn owned_rows_mut(&mut self, actor_id: &str, message_ids: &[String]) -> Result<(), String> {
        require_identity(actor_id)?;
        if message_ids.is_empty() {
            return Err("Signal burn needs at least one owned message".to_owned());
        }
        let mut unique = std::collections::BTreeSet::new();
        for message_id in message_ids {
            if message_id.trim().is_empty() || !unique.insert(message_id) {
                return Err("Signal burn target list is invalid".to_owned());
            }
            let row = self
                .rows
                .get(message_id)
                .ok_or_else(|| format!("Signal protected message {message_id} is missing"))?;
            if row.owner_id != actor_id {
                return Err(format!(
                    "Signal protected message {message_id} is not owned by {actor_id}"
                ));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SignalExpiryReceipt {
    pub command: &'static str,
    pub target_message_id: String,
    pub owner_id: String,
    pub expires_at_unix_seconds: u64,
    pub view_once: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SignalBurnSide {
    YourSide,
    TheirSide,
    BothSides,
}

impl SignalBurnSide {
    pub const fn command_name(self) -> &'static str {
        match self {
            Self::YourSide => "signal_burn_your_side",
            Self::TheirSide => "signal_burn_their_side",
            Self::BothSides => "signal_burn_both_sides",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SignalBurnReceipt {
    pub command: &'static str,
    pub side: SignalBurnSide,
    pub owner_id: String,
    pub target_message_ids: Vec<String>,
    pub local_removed_count: usize,
    pub remote_removed_count: usize,
}

/// Set a one-to-thirty-day Signal timer and return its absolute expiry.
pub fn cmd_signal_set_timer(
    store: &mut SignalProtectedMessageStore,
    actor_id: &str,
    message_id: &str,
    duration_seconds: u64,
    now_unix_seconds: u64,
) -> Result<SignalExpiryReceipt, String> {
    let expires_at_unix_seconds = checked_expiry(
        duration_seconds,
        MAX_SIGNAL_TIMER_SECONDS,
        now_unix_seconds,
        "timer",
    )?;
    let row = store.owned_row_mut(actor_id, message_id)?;
    row.expiry_at_unix_seconds = Some(expires_at_unix_seconds);
    row.view_once = false;
    Ok(SignalExpiryReceipt {
        command: "signal_set_timer",
        target_message_id: row.message_id.clone(),
        owner_id: actor_id.to_owned(),
        expires_at_unix_seconds,
        view_once: false,
    })
}

/// Set a one-to-sixty-second Signal view-once window and return its expiry.
pub fn cmd_signal_set_view_once(
    store: &mut SignalProtectedMessageStore,
    actor_id: &str,
    message_id: &str,
    duration_seconds: u64,
    now_unix_seconds: u64,
) -> Result<SignalExpiryReceipt, String> {
    let expires_at_unix_seconds = checked_expiry(
        duration_seconds,
        MAX_SIGNAL_VIEW_ONCE_SECONDS,
        now_unix_seconds,
        "view-once",
    )?;
    let row = store.owned_row_mut(actor_id, message_id)?;
    row.expiry_at_unix_seconds = Some(expires_at_unix_seconds);
    row.view_once = true;
    Ok(SignalExpiryReceipt {
        command: "signal_set_view_once",
        target_message_id: row.message_id.clone(),
        owner_id: actor_id.to_owned(),
        expires_at_unix_seconds,
        view_once: true,
    })
}

/// Remove only the caller's local protected copies.
pub fn cmd_signal_burn_your_side(
    store: &mut SignalProtectedMessageStore,
    actor_id: &str,
    message_ids: Vec<String>,
) -> Result<SignalBurnReceipt, String> {
    burn_owned(store, actor_id, message_ids, SignalBurnSide::YourSide)
}

/// Remove only the caller's remote protected copies.
pub fn cmd_signal_burn_their_side(
    store: &mut SignalProtectedMessageStore,
    actor_id: &str,
    message_ids: Vec<String>,
) -> Result<SignalBurnReceipt, String> {
    burn_owned(store, actor_id, message_ids, SignalBurnSide::TheirSide)
}

/// Remove both protected copies, but only for messages owned by the caller.
pub fn cmd_signal_burn_both_sides(
    store: &mut SignalProtectedMessageStore,
    actor_id: &str,
    message_ids: Vec<String>,
) -> Result<SignalBurnReceipt, String> {
    burn_owned(store, actor_id, message_ids, SignalBurnSide::BothSides)
}

fn burn_owned(
    store: &mut SignalProtectedMessageStore,
    actor_id: &str,
    message_ids: Vec<String>,
    side: SignalBurnSide,
) -> Result<SignalBurnReceipt, String> {
    // Validate every target before changing the first row.
    store.owned_rows_mut(actor_id, &message_ids)?;
    let mut local_removed_count = 0;
    let mut remote_removed_count = 0;
    for message_id in &message_ids {
        let row = store
            .rows
            .get_mut(message_id)
            .expect("ownership validation kept the Signal row present");
        if matches!(side, SignalBurnSide::YourSide | SignalBurnSide::BothSides) && row.local_present
        {
            row.local_present = false;
            local_removed_count += 1;
        }
        if matches!(side, SignalBurnSide::TheirSide | SignalBurnSide::BothSides)
            && row.remote_present
        {
            row.remote_present = false;
            remote_removed_count += 1;
        }
    }
    Ok(SignalBurnReceipt {
        command: side.command_name(),
        side,
        owner_id: actor_id.to_owned(),
        target_message_ids: message_ids,
        local_removed_count,
        remote_removed_count,
    })
}

fn require_identity(actor_id: &str) -> Result<(), String> {
    if actor_id.trim().is_empty() {
        Err("Signal command needs an owner".to_owned())
    } else {
        Ok(())
    }
}

fn checked_expiry(
    duration_seconds: u64,
    maximum_seconds: u64,
    now_unix_seconds: u64,
    kind: &str,
) -> Result<u64, String> {
    if duration_seconds == 0 || duration_seconds > maximum_seconds {
        return Err(format!(
            "Signal {kind} must be between 1 and {maximum_seconds} seconds"
        ));
    }
    now_unix_seconds
        .checked_add(duration_seconds)
        .ok_or_else(|| "Signal expiry is out of range".to_owned())
}
