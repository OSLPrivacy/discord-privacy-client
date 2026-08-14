//! The OSL Chats chat-settings DANGER block.
//!
//! Three rows live here, and each one hands back a receipt of what it
//! actually did rather than a promise of what it meant to do:
//!
//! * **Clear history (this device)** drops the local copies of one chat and
//!   nothing else. The copies held by another device of yours, and the copy on
//!   their side, are named in the receipt as refused - this row cannot reach
//!   them and says so instead of implying it did.
//! * **Block** stops their new messages before they are ever written to the
//!   local store, and states what the blocked person can still see. Every
//!   message the block turns away is named in the receipt.
//! * **Burn this chat** runs the burn rules behind the ask step. With the ask
//!   step on and no confirmation the burn changes nothing and the receipt
//!   carries a needs-confirming refusal for every message it left alone.
//!
//! The receipt shape is deliberately the same for all three so the settings
//! screen can render one list: what was touched, and what was refused, both
//! named item by item.

use std::collections::BTreeSet;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use store::{MessageStore, StoredMessage};

/// The DANGER rows, in the order the chat-settings screen draws them.
pub const CLEAR_HISTORY_LABEL: &str = "Clear history (this device)";
pub const BLOCK_LABEL: &str = "Block";
pub const BURN_CHAT_LABEL: &str = "Burn this chat";

/// Clear history says, in the row itself, that it is local-only.
pub const CLEAR_HISTORY_SAYS: &str = "Removes the local copies on this device only. \
Copies held by your other devices and the copy on their side are left exactly as they are.";

/// Block says what the blocked person can still see.
pub const BLOCK_SAYS: &str = "Stops their messages reaching you. \
They can still see the messages you already sent them, your OSL display name, \
and your public profile card. Blocking deletes nothing and does not tell them they were blocked.";

/// Burn says that it is the destructive one and that it asks first.
pub const BURN_CHAT_SAYS: &str = "Runs the burn rules over this chat on this side: \
every local message is shredded and then removed. \
With the ask step on nothing is touched until you confirm.";

/// The answer the ask step returns while a burn is still unconfirmed.
pub const NEEDS_CONFIRMING: &str = "needs confirming";

/// The refusal reason written against every message an unconfirmed burn left
/// alone.
pub const BURN_NEEDS_CONFIRMING_REASON: &str =
    "needs confirming: burn this chat was not confirmed, so this message was left alone";

/// The refusal reason written against a copy this device cannot reach.
pub const OTHER_COPY_REFUSAL_REASON: &str =
    "not reachable from this device: this copy is held elsewhere and was not touched";

/// The refusal reason written against a message turned away by a block.
pub const BLOCKED_MESSAGE_REFUSAL_REASON: &str =
    "blocked: this person is blocked, so the message was refused before it was stored";

/// What a blocked person keeps seeing after the block lands.
pub const STILL_VISIBLE_AFTER_BLOCK: [&str; 3] = [
    "The messages you already sent them",
    "Your OSL display name",
    "Your public profile card",
];

/// `list_by_channel` takes a bound; the DANGER rows want the whole chat.
const WHOLE_CHAT_LIMIT: u32 = u32::MAX;

/// Which DANGER row produced a receipt.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DangerRowAction {
    ClearHistoryThisDevice,
    Block,
    BurnThisChat,
}

impl DangerRowAction {
    pub fn label(self) -> &'static str {
        match self {
            Self::ClearHistoryThisDevice => CLEAR_HISTORY_LABEL,
            Self::Block => BLOCK_LABEL,
            Self::BurnThisChat => BURN_CHAT_LABEL,
        }
    }

    pub fn says(self) -> &'static str {
        match self {
            Self::ClearHistoryThisDevice => CLEAR_HISTORY_SAYS,
            Self::Block => BLOCK_SAYS,
            Self::BurnThisChat => BURN_CHAT_SAYS,
        }
    }
}

/// One named thing a DANGER row touched, or refused to touch.
#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DangerRowItem {
    /// Stable name of the thing: a message id, a person id, or
    /// `"<holder>:<message id>"` for a copy this device does not hold.
    pub item_id: String,
    /// `local_message`, `other_copy`, `person`, or `incoming_message`.
    pub item_kind: String,
    /// The message id this item is about, when there is one.
    pub message_id: Option<String>,
    /// Who holds the copy, when the item is not local.
    pub holder: Option<String>,
    /// What happened to it, or why it was refused.
    pub detail: String,
}

/// What a DANGER row actually did.
#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DangerRowReceipt {
    pub action: DangerRowAction,
    pub action_label: String,
    pub chat_id: String,
    /// The sentence the row shows the person before they press it.
    pub says: String,
    /// For Block: what the blocked person can still see. Empty otherwise.
    pub still_visible_to_them: Vec<String>,
    /// `false` when the row refused to run at all (the ask step).
    pub carried_out: bool,
    /// The answer the ask step gave, when it stopped the row.
    pub needs_confirming: Option<String>,
    /// Every item this run changed, named.
    pub touched: Vec<DangerRowItem>,
    /// Every item this run deliberately did not change, named with a reason.
    pub refused: Vec<DangerRowItem>,
    /// Local messages present before the row ran.
    pub local_count_before: usize,
    /// Local messages present after the row ran.
    pub local_count_after: usize,
}

impl DangerRowReceipt {
    pub fn touched_ids(&self) -> Vec<String> {
        self.touched.iter().map(|it| it.item_id.clone()).collect()
    }

    pub fn refused_ids(&self) -> Vec<String> {
        self.refused.iter().map(|it| it.item_id.clone()).collect()
    }

    /// Message ids named in the touched list.
    pub fn touched_message_ids(&self) -> BTreeSet<String> {
        self.touched
            .iter()
            .filter_map(|it| it.message_id.clone())
            .collect()
    }

    /// Message ids named in the refused list.
    pub fn refused_message_ids(&self) -> BTreeSet<String> {
        self.refused
            .iter()
            .filter_map(|it| it.message_id.clone())
            .collect()
    }

    /// True when every id in `expected` is named somewhere in the receipt.
    pub fn names_every(&self, expected: &[String]) -> bool {
        let named: BTreeSet<&str> = self
            .touched
            .iter()
            .chain(self.refused.iter())
            .filter_map(|it| it.message_id.as_deref())
            .collect();
        expected.iter().all(|id| named.contains(id.as_str()))
    }

    /// One line per item, for the receipt panel and for evidence output.
    pub fn receipt_lines(&self) -> Vec<String> {
        let mut lines = Vec::with_capacity(self.touched.len() + self.refused.len());
        for item in &self.touched {
            lines.push(format!("touched {} {}", item.item_id, item.detail));
        }
        for item in &self.refused {
            lines.push(format!("refused {} {}", item.item_id, item.detail));
        }
        lines
    }
}

/// Clear history (this device).
#[derive(Debug, Clone, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClearChatHistoryRequest {
    pub chat_id: String,
    /// Named holders of the copies this row cannot reach - your other devices
    /// and their side. Each one is receipted as refused, per message.
    #[serde(default)]
    pub other_copy_holders: Vec<String>,
}

/// Block this person.
#[derive(Debug, Clone, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BlockPersonRequest {
    pub chat_id: String,
    pub person_id: String,
}

/// Burn this chat, behind the ask step.
#[derive(Debug, Clone, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BurnThisChatRequest {
    pub chat_id: String,
    /// The ask step from the burn rules. On means the burn refuses until
    /// `confirmed` is true.
    pub ask_step_on: bool,
    /// True only once the person has answered the ask step.
    pub confirmed: bool,
    #[serde(default)]
    pub other_copy_holders: Vec<String>,
}

/// The outcome of one inbound OSL Chat message meeting the block gate.
#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IncomingMessageOutcome {
    pub message_id: String,
    /// True when the message was written to the local store.
    pub stored: bool,
    /// Present when the block turned it away.
    pub refusal_receipt: Option<DangerRowReceipt>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct BlockRecord {
    chat_id: String,
    person_id: String,
    refused: Vec<DangerRowItem>,
}

/// The DANGER block's own state: who is blocked, and every message a block has
/// turned away so far.
#[derive(Default)]
pub struct OslChatDangerRowState {
    blocks: Mutex<Vec<BlockRecord>>,
}

impl OslChatDangerRowState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn clear_chat_history_this_device(
        &self,
        store: &MessageStore,
        input: ClearChatHistoryRequest,
    ) -> Result<DangerRowReceipt, String> {
        clear_chat_history_this_device_command(store, input)
    }

    pub fn block_person(&self, input: BlockPersonRequest) -> Result<DangerRowReceipt, String> {
        block_person_command(self, input)
    }

    pub fn burn_this_chat(
        &self,
        store: &MessageStore,
        input: BurnThisChatRequest,
    ) -> Result<DangerRowReceipt, String> {
        burn_this_chat_command(store, input)
    }

    /// The inbound gate. A blocked person's message never reaches the store.
    pub fn deliver_incoming_message(
        &self,
        store: &MessageStore,
        message: &StoredMessage,
    ) -> Result<IncomingMessageOutcome, String> {
        deliver_incoming_osl_chat_message(self, store, message)
    }

    /// The standing Block receipt for one chat: the person it touched, and
    /// every message it has refused since.
    pub fn block_receipt(&self, chat_id: &str) -> Result<Option<DangerRowReceipt>, String> {
        let blocks = self.lock_blocks()?;
        Ok(blocks
            .iter()
            .find(|record| record.chat_id == chat_id)
            .map(|record| block_receipt_from(record, 0, 0)))
    }

    pub fn is_blocked(&self, chat_id: &str, person_id: &str) -> Result<bool, String> {
        let blocks = self.lock_blocks()?;
        Ok(blocks
            .iter()
            .any(|record| record.chat_id == chat_id && record.person_id == person_id))
    }

    fn lock_blocks(&self) -> Result<std::sync::MutexGuard<'_, Vec<BlockRecord>>, String> {
        self.blocks
            .lock()
            .map_err(|_| "The OSL Chats danger block is unavailable".to_owned())
    }
}

fn normalize(value: &str, what: &str) -> Result<String, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(format!("{what} is missing"));
    }
    Ok(trimmed.to_owned())
}

fn live_message_ids(store: &MessageStore, chat_id: &str) -> Result<Vec<String>, String> {
    let mut ids: Vec<String> = store
        .list_by_channel(chat_id, WHOLE_CHAT_LIMIT)
        .map_err(|error| format!("This chat's local messages could not be read: {error}"))?
        .into_iter()
        .map(|msg| msg.discord_message_id)
        .collect();
    ids.sort();
    Ok(ids)
}

fn local_count(store: &MessageStore, chat_id: &str) -> Result<usize, String> {
    store
        .count_live_by_channel(chat_id, None)
        .map_err(|error| format!("This chat's local messages could not be counted: {error}"))
}

fn other_copy_refusals(holders: &[String], message_ids: &[String]) -> Vec<DangerRowItem> {
    let mut out = Vec::with_capacity(holders.len() * message_ids.len());
    for holder in holders {
        for message_id in message_ids {
            out.push(DangerRowItem {
                item_id: format!("{holder}:{message_id}"),
                item_kind: "other_copy".to_owned(),
                message_id: Some(message_id.clone()),
                holder: Some(holder.clone()),
                detail: OTHER_COPY_REFUSAL_REASON.to_owned(),
            });
        }
    }
    out
}

fn normalize_holders(holders: Vec<String>) -> Result<Vec<String>, String> {
    let mut out = Vec::with_capacity(holders.len());
    for holder in holders {
        let holder = normalize(&holder, "A named copy holder")?;
        if !out.contains(&holder) {
            out.push(holder);
        }
    }
    Ok(out)
}

/// Clear history (this device): drop the local copies of one chat, and name
/// every copy elsewhere that this row did not touch.
pub fn clear_chat_history_this_device_command(
    store: &MessageStore,
    input: ClearChatHistoryRequest,
) -> Result<DangerRowReceipt, String> {
    let chat_id = normalize(&input.chat_id, "The chat id")?;
    let holders = normalize_holders(input.other_copy_holders)?;

    let before_ids = live_message_ids(store, &chat_id)?;
    let count_before = local_count(store, &chat_id)?;

    let outcome = store
        .delete_message_records(&before_ids)
        .map_err(|error| format!("The local copies could not be removed: {error}"))?;

    let count_after = local_count(store, &chat_id)?;
    let still_here = live_message_ids(store, &chat_id)?;

    let mut touched = Vec::with_capacity(before_ids.len());
    let mut refused = Vec::new();
    for message_id in &before_ids {
        if still_here.contains(message_id) {
            refused.push(DangerRowItem {
                item_id: message_id.clone(),
                item_kind: "local_message".to_owned(),
                message_id: Some(message_id.clone()),
                holder: Some("this-device".to_owned()),
                detail: "still here: the local copy could not be removed".to_owned(),
            });
        } else {
            touched.push(DangerRowItem {
                item_id: message_id.clone(),
                item_kind: "local_message".to_owned(),
                message_id: Some(message_id.clone()),
                holder: Some("this-device".to_owned()),
                detail: "removed: the local copy on this device is gone".to_owned(),
            });
        }
    }
    refused.extend(other_copy_refusals(&holders, &before_ids));

    debug_assert_eq!(outcome.requested_count, before_ids.len());

    Ok(DangerRowReceipt {
        action: DangerRowAction::ClearHistoryThisDevice,
        action_label: CLEAR_HISTORY_LABEL.to_owned(),
        chat_id,
        says: CLEAR_HISTORY_SAYS.to_owned(),
        still_visible_to_them: Vec::new(),
        carried_out: true,
        needs_confirming: None,
        touched,
        refused,
        local_count_before: count_before,
        local_count_after: count_after,
    })
}

fn block_receipt_from(
    record: &BlockRecord,
    count_before: usize,
    count_after: usize,
) -> DangerRowReceipt {
    DangerRowReceipt {
        action: DangerRowAction::Block,
        action_label: BLOCK_LABEL.to_owned(),
        chat_id: record.chat_id.clone(),
        says: BLOCK_SAYS.to_owned(),
        still_visible_to_them: STILL_VISIBLE_AFTER_BLOCK
            .iter()
            .map(|line| (*line).to_owned())
            .collect(),
        carried_out: true,
        needs_confirming: None,
        touched: vec![DangerRowItem {
            item_id: record.person_id.clone(),
            item_kind: "person".to_owned(),
            message_id: None,
            holder: Some("this-device".to_owned()),
            detail: "blocked: their new messages are refused before they are stored".to_owned(),
        }],
        refused: record.refused.clone(),
        local_count_before: count_before,
        local_count_after: count_after,
    }
}

/// Block: stop their messages, and say what they can still see.
pub fn block_person_command(
    state: &OslChatDangerRowState,
    input: BlockPersonRequest,
) -> Result<DangerRowReceipt, String> {
    let chat_id = normalize(&input.chat_id, "The chat id")?;
    let person_id = normalize(&input.person_id, "The person id")?;

    let mut blocks = state.lock_blocks()?;
    if let Some(existing) = blocks
        .iter()
        .find(|record| record.chat_id == chat_id && record.person_id == person_id)
    {
        return Ok(block_receipt_from(existing, 0, 0));
    }
    let record = BlockRecord {
        chat_id,
        person_id,
        refused: Vec::new(),
    };
    let receipt = block_receipt_from(&record, 0, 0);
    blocks.push(record);
    Ok(receipt)
}

/// The inbound gate the block actually works through: a blocked person's
/// message is refused before `MessageStore::put`, and the refusal is receipted.
pub fn deliver_incoming_osl_chat_message(
    state: &OslChatDangerRowState,
    store: &MessageStore,
    message: &StoredMessage,
) -> Result<IncomingMessageOutcome, String> {
    let chat_id = normalize(&message.channel_id, "The chat id")?;
    let message_id = normalize(&message.discord_message_id, "The message id")?;

    let mut blocks = state.lock_blocks()?;
    let blocked = blocks.iter_mut().find(|record| {
        record.chat_id == chat_id
            && (record.person_id == message.sender_osl_user_id
                || record.person_id == message.sender_discord_id)
    });

    if let Some(record) = blocked {
        let item = DangerRowItem {
            item_id: message_id.clone(),
            item_kind: "incoming_message".to_owned(),
            message_id: Some(message_id.clone()),
            holder: Some(record.person_id.clone()),
            detail: BLOCKED_MESSAGE_REFUSAL_REASON.to_owned(),
        };
        if !record
            .refused
            .iter()
            .any(|seen| seen.item_id == item.item_id)
        {
            record.refused.push(item.clone());
        }
        let mut receipt = block_receipt_from(record, 0, 0);
        receipt.refused = vec![item];
        return Ok(IncomingMessageOutcome {
            message_id,
            stored: false,
            refusal_receipt: Some(receipt),
        });
    }
    drop(blocks);

    store
        .put(message)
        .map_err(|error| format!("The incoming message could not be stored: {error}"))?;
    Ok(IncomingMessageOutcome {
        message_id,
        stored: true,
        refusal_receipt: None,
    })
}

/// Burn this chat: the burn rules behind the ask step.
pub fn burn_this_chat_command(
    store: &MessageStore,
    input: BurnThisChatRequest,
) -> Result<DangerRowReceipt, String> {
    let chat_id = normalize(&input.chat_id, "The chat id")?;
    let holders = normalize_holders(input.other_copy_holders)?;

    let before_ids = live_message_ids(store, &chat_id)?;
    let count_before = local_count(store, &chat_id)?;

    // The ask step from the burn rules. Until it is answered the burn touches
    // nothing at all, and every message it left alone is named as refused.
    if input.ask_step_on && !input.confirmed {
        let mut refused: Vec<DangerRowItem> = before_ids
            .iter()
            .map(|message_id| DangerRowItem {
                item_id: message_id.clone(),
                item_kind: "local_message".to_owned(),
                message_id: Some(message_id.clone()),
                holder: Some("this-device".to_owned()),
                detail: BURN_NEEDS_CONFIRMING_REASON.to_owned(),
            })
            .collect();
        refused.extend(other_copy_refusals(&holders, &before_ids));
        return Ok(DangerRowReceipt {
            action: DangerRowAction::BurnThisChat,
            action_label: BURN_CHAT_LABEL.to_owned(),
            chat_id,
            says: BURN_CHAT_SAYS.to_owned(),
            still_visible_to_them: Vec::new(),
            carried_out: false,
            needs_confirming: Some(NEEDS_CONFIRMING.to_owned()),
            touched: Vec::new(),
            refused,
            local_count_before: count_before,
            local_count_after: count_before,
        });
    }

    // Shred first, then drop the row: a burn is stronger than a clear.
    let mut shred_failures = Vec::new();
    for message_id in &before_ids {
        if let Err(error) = store.mark_burned(message_id) {
            shred_failures.push((message_id.clone(), error.to_string()));
        }
    }
    store
        .delete_message_records(&before_ids)
        .map_err(|error| format!("The burned rows could not be removed: {error}"))?;

    let count_after = local_count(store, &chat_id)?;
    let still_here = live_message_ids(store, &chat_id)?;

    let mut touched = Vec::with_capacity(before_ids.len());
    let mut refused = Vec::new();
    for message_id in &before_ids {
        let failure = shred_failures
            .iter()
            .find(|(failed, _)| failed == message_id)
            .map(|(_, why)| why.clone());
        if still_here.contains(message_id) || failure.is_some() {
            refused.push(DangerRowItem {
                item_id: message_id.clone(),
                item_kind: "local_message".to_owned(),
                message_id: Some(message_id.clone()),
                holder: Some("this-device".to_owned()),
                detail: failure
                    .map(|why| format!("not burned: {why}"))
                    .unwrap_or_else(|| "not burned: the local copy is still here".to_owned()),
            });
        } else {
            touched.push(DangerRowItem {
                item_id: message_id.clone(),
                item_kind: "local_message".to_owned(),
                message_id: Some(message_id.clone()),
                holder: Some("this-device".to_owned()),
                detail: "burned: shredded and removed from this side".to_owned(),
            });
        }
    }
    refused.extend(other_copy_refusals(&holders, &before_ids));

    Ok(DangerRowReceipt {
        action: DangerRowAction::BurnThisChat,
        action_label: BURN_CHAT_LABEL.to_owned(),
        chat_id,
        says: BURN_CHAT_SAYS.to_owned(),
        still_visible_to_them: Vec::new(),
        carried_out: true,
        needs_confirming: None,
        touched,
        refused,
        local_count_before: count_before,
        local_count_after: count_after,
    })
}

// The crate's `--lib` test target does not currently build (unrelated,
// pre-existing breakage across other modules' `#[cfg(test)]` code), so the
// unit-level checks for this module live in
// `tests/task_5023_osl_chat_danger_row.rs`, which does build and run.
