//! Pro-only deletion of reviewed, marked messages.
//!
//! This module performs no I/O and holds no store handle. It is the decision
//! boundary the review screen (TASK 1446) hands its decisions to:
//!
//! 1. `count_marked_messages` refuses a Free requester outright, and for Pro
//!    returns the final count of reviewed, marked messages plus a confirmation
//!    token bound to exactly that set.
//! 2. `delete_marked_messages` refuses anything that did not come through step
//!    1: no token means the count was never shown, and a stale token means the
//!    marked set changed after the count the requester actually read.
//!
//! Deletion is therefore reachable only for Pro, only for messages that were
//! reviewed and marked, and only after a final count was produced and shown.
//!
//! Locators are carried through untouched but are never parsed or opened here;
//! TASK 1445 already strips jump links from them upstream.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::burn_contract::ProductTier;

const MAX_MARKED_MESSAGES: usize = 1_000;
const MAX_ACCOUNT_ID_BYTES: usize = 256;
const MAX_MESSAGE_LOCATOR_BYTES: usize = 256;

pub const PRO_REQUIRED_REFUSAL: &str =
    "Deleting marked messages is a Pro feature. Your plan is Free, so nothing was deleted.";
pub const COUNT_NOT_SHOWN_REFUSAL: &str =
    "The final count of marked messages has not been shown yet, so nothing was deleted.";
pub const COUNT_CHANGED_REFUSAL: &str =
    "The marked messages changed after the count you were shown, so nothing was deleted. Count them again.";
pub const NOT_CONFIRMED_REFUSAL: &str = "The deletion was not confirmed, so nothing was deleted.";
pub const NOTHING_MARKED_REFUSAL: &str =
    "No reviewed message is marked for deletion, so there is nothing to delete.";
pub const UNREVIEWED_REFUSAL: &str =
    "A message is marked for deletion but has not been reviewed, so nothing was deleted.";
pub const TOO_MANY_REFUSAL: &str =
    "Too many messages were sent to this command, so nothing was deleted.";
pub const INVALID_MESSAGE_REFUSAL: &str =
    "A message in this request is missing its account or its place, so nothing was deleted.";

/// The plan the requester is on, as it arrives from the caller. Kept separate
/// from [`ProductTier`] only because that type is the crate-wide contract type
/// and carries no serde derives.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RequesterPlan {
    Free,
    Pro,
}

impl RequesterPlan {
    pub fn tier(self) -> ProductTier {
        match self {
            Self::Free => ProductTier::Free,
            Self::Pro => ProductTier::Pro,
        }
    }
}

/// What the review screen decided about one result. `Pending` means the
/// requester never opened it, so it is neither kept nor marked.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ReviewDecision {
    Pending,
    Kept,
    MarkedForDeletion,
}

/// One result carried over from the review session.
///
/// `reviewed` is recorded separately from `decision` on purpose: a marked row
/// that was never actually reviewed is a bug upstream, and this command must
/// refuse the whole request rather than quietly delete it.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviewedMessage {
    pub account_id: String,
    pub message_locator: String,
    pub decision: ReviewDecision,
    pub reviewed: bool,
}

impl ReviewedMessage {
    fn is_marked_and_reviewed(&self) -> bool {
        self.reviewed && self.decision == ReviewDecision::MarkedForDeletion
    }

    fn is_marked_without_review(&self) -> bool {
        !self.reviewed && self.decision == ReviewDecision::MarkedForDeletion
    }
}

/// A message this command reports on, without any decision attached.
#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MarkedMessageRef {
    pub account_id: String,
    pub message_locator: String,
}

/// Per-account share of the final count, so the confirmation screen can say
/// which account loses what.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MarkedAccountCount {
    pub account_id: String,
    pub marked_count: usize,
}

/// The final count a Pro requester must receive before any confirmation.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MarkedDeletionCount {
    pub marked_count: usize,
    pub kept_count: usize,
    pub pending_count: usize,
    pub account_counts: Vec<MarkedAccountCount>,
    pub messages: Vec<MarkedMessageRef>,
    /// Bound to exactly the marked set above. Deletion is unreachable without it.
    pub confirmation_token: String,
    pub confirmation_prompt: String,
    pub confirmation_required: bool,
}

/// The result of an accepted deletion.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MarkedDeletionOutcome {
    pub deleted_count: usize,
    pub deleted: Vec<MarkedMessageRef>,
    pub kept_untouched_count: usize,
    pub confirmation_token: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MarkedDeletionError {
    ProRequired,
    CountNotShown,
    CountChanged,
    NotConfirmed,
    NothingMarked,
    MarkedButNotReviewed,
    TooManyMessages,
    InvalidMessage,
}

impl MarkedDeletionError {
    pub fn code(self) -> &'static str {
        match self {
            Self::ProRequired => "pro_required",
            Self::CountNotShown => "count_not_shown",
            Self::CountChanged => "count_changed",
            Self::NotConfirmed => "not_confirmed",
            Self::NothingMarked => "nothing_marked",
            Self::MarkedButNotReviewed => "marked_but_not_reviewed",
            Self::TooManyMessages => "too_many_messages",
            Self::InvalidMessage => "invalid_message",
        }
    }

    pub fn message(self) -> &'static str {
        match self {
            Self::ProRequired => PRO_REQUIRED_REFUSAL,
            Self::CountNotShown => COUNT_NOT_SHOWN_REFUSAL,
            Self::CountChanged => COUNT_CHANGED_REFUSAL,
            Self::NotConfirmed => NOT_CONFIRMED_REFUSAL,
            Self::NothingMarked => NOTHING_MARKED_REFUSAL,
            Self::MarkedButNotReviewed => UNREVIEWED_REFUSAL,
            Self::TooManyMessages => TOO_MANY_REFUSAL,
            Self::InvalidMessage => INVALID_MESSAGE_REFUSAL,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MarkedDeletionCountRequest {
    pub plan: RequesterPlan,
    pub messages: Vec<ReviewedMessage>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MarkedDeletionDeleteRequest {
    pub plan: RequesterPlan,
    pub messages: Vec<ReviewedMessage>,
    /// Empty when the requester never asked for a count.
    #[serde(default)]
    pub confirmation_token: String,
    #[serde(default)]
    pub confirmed: bool,
}

fn validate(messages: &[ReviewedMessage]) -> Result<(), MarkedDeletionError> {
    if messages.len() > MAX_MARKED_MESSAGES {
        return Err(MarkedDeletionError::TooManyMessages);
    }
    for message in messages {
        if message.account_id.trim().is_empty()
            || message.account_id.len() > MAX_ACCOUNT_ID_BYTES
            || message.message_locator.trim().is_empty()
            || message.message_locator.len() > MAX_MESSAGE_LOCATOR_BYTES
        {
            return Err(MarkedDeletionError::InvalidMessage);
        }
    }
    Ok(())
}

fn marked_refs(messages: &[ReviewedMessage]) -> Vec<MarkedMessageRef> {
    let mut refs = messages
        .iter()
        .filter(|message| message.is_marked_and_reviewed())
        .map(|message| MarkedMessageRef {
            account_id: message.account_id.clone(),
            message_locator: message.message_locator.clone(),
        })
        .collect::<Vec<_>>();
    refs.sort();
    refs
}

/// Deterministic over the marked set and its size, so a token issued for one
/// count cannot confirm a different one.
fn confirmation_token(refs: &[MarkedMessageRef]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"OSL/pro-marked-deletion/v1");
    hasher.update((refs.len() as u64).to_be_bytes());
    for item in refs {
        hasher.update((item.account_id.len() as u64).to_be_bytes());
        hasher.update(item.account_id.as_bytes());
        hasher.update((item.message_locator.len() as u64).to_be_bytes());
        hasher.update(item.message_locator.as_bytes());
    }
    let digest: [u8; 32] = hasher.finalize().into();
    let mut token = String::with_capacity(64);
    for byte in digest {
        token.push_str(&format!("{byte:02x}"));
    }
    token
}

fn plain_count_line(marked_count: usize) -> String {
    if marked_count == 1 {
        "Delete 1 marked message? This cannot be undone.".to_owned()
    } else {
        format!("Delete {marked_count} marked messages? This cannot be undone.")
    }
}

/// Step 1. Free is refused here, before any count exists. Pro receives the
/// final count and the token that step 2 requires.
pub fn count_marked_messages(
    tier: ProductTier,
    messages: &[ReviewedMessage],
) -> Result<MarkedDeletionCount, MarkedDeletionError> {
    if tier != ProductTier::Pro {
        return Err(MarkedDeletionError::ProRequired);
    }
    validate(messages)?;
    if messages
        .iter()
        .any(ReviewedMessage::is_marked_without_review)
    {
        return Err(MarkedDeletionError::MarkedButNotReviewed);
    }
    let refs = marked_refs(messages);
    if refs.is_empty() {
        return Err(MarkedDeletionError::NothingMarked);
    }
    let mut account_counts: Vec<MarkedAccountCount> = Vec::new();
    for item in &refs {
        match account_counts
            .iter_mut()
            .find(|entry| entry.account_id == item.account_id)
        {
            Some(entry) => entry.marked_count += 1,
            None => account_counts.push(MarkedAccountCount {
                account_id: item.account_id.clone(),
                marked_count: 1,
            }),
        }
    }
    let marked_count = refs.len();
    Ok(MarkedDeletionCount {
        marked_count,
        kept_count: messages
            .iter()
            .filter(|message| message.decision == ReviewDecision::Kept)
            .count(),
        pending_count: messages
            .iter()
            .filter(|message| message.decision == ReviewDecision::Pending)
            .count(),
        account_counts,
        confirmation_token: confirmation_token(&refs),
        messages: refs,
        confirmation_prompt: plain_count_line(marked_count),
        confirmation_required: true,
    })
}

/// Step 2. Reachable only for Pro, only with the token step 1 issued for this
/// exact marked set, and only once the requester confirmed.
pub fn delete_marked_messages(
    tier: ProductTier,
    messages: &[ReviewedMessage],
    supplied_token: &str,
    confirmed: bool,
) -> Result<MarkedDeletionOutcome, MarkedDeletionError> {
    if tier != ProductTier::Pro {
        return Err(MarkedDeletionError::ProRequired);
    }
    validate(messages)?;
    if messages
        .iter()
        .any(ReviewedMessage::is_marked_without_review)
    {
        return Err(MarkedDeletionError::MarkedButNotReviewed);
    }
    if supplied_token.trim().is_empty() {
        return Err(MarkedDeletionError::CountNotShown);
    }
    let refs = marked_refs(messages);
    if refs.is_empty() {
        return Err(MarkedDeletionError::NothingMarked);
    }
    let expected = confirmation_token(&refs);
    if supplied_token != expected {
        return Err(MarkedDeletionError::CountChanged);
    }
    if !confirmed {
        return Err(MarkedDeletionError::NotConfirmed);
    }
    Ok(MarkedDeletionOutcome {
        deleted_count: refs.len(),
        kept_untouched_count: messages.len() - refs.len(),
        deleted: refs,
        confirmation_token: expected,
    })
}

/// JSON command surface. `command` is `count` or `delete`; the reply is always
/// a JSON object with an `ok` field, so a refusal is never mistaken for a
/// transport failure.
pub fn run_pro_marked_deletion_command(command: &str, request_json: &str) -> String {
    match command {
        "count" => {
            let request: MarkedDeletionCountRequest = match serde_json::from_str(request_json) {
                Ok(request) => request,
                Err(error) => return bad_request_json(command, &error.to_string()),
            };
            match count_marked_messages(request.plan.tier(), &request.messages) {
                Ok(count) => ok_json(command, &count),
                Err(error) => refusal_json(command, error),
            }
        }
        "delete" => {
            let request: MarkedDeletionDeleteRequest = match serde_json::from_str(request_json) {
                Ok(request) => request,
                Err(error) => return bad_request_json(command, &error.to_string()),
            };
            match delete_marked_messages(
                request.plan.tier(),
                &request.messages,
                &request.confirmation_token,
                request.confirmed,
            ) {
                Ok(outcome) => ok_json(command, &outcome),
                Err(error) => refusal_json(command, error),
            }
        }
        other => serde_json::json!({
            "ok": false,
            "command": other,
            "errorCode": "unknown_command",
            "error": "This command is not part of marked deletion.",
        })
        .to_string(),
    }
}

fn ok_json<T: Serialize>(command: &str, payload: &T) -> String {
    let value = match serde_json::to_value(payload) {
        Ok(value) => value,
        Err(error) => return bad_request_json(command, &error.to_string()),
    };
    serde_json::json!({ "ok": true, "command": command, "result": value }).to_string()
}

fn refusal_json(command: &str, error: MarkedDeletionError) -> String {
    serde_json::json!({
        "ok": false,
        "command": command,
        "errorCode": error.code(),
        "error": error.message(),
    })
    .to_string()
}

fn bad_request_json(command: &str, detail: &str) -> String {
    serde_json::json!({
        "ok": false,
        "command": command,
        "errorCode": "bad_request",
        "error": format!("This request could not be read: {detail}"),
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn marked(account: &str, locator: &str) -> ReviewedMessage {
        ReviewedMessage {
            account_id: account.to_owned(),
            message_locator: locator.to_owned(),
            decision: ReviewDecision::MarkedForDeletion,
            reviewed: true,
        }
    }

    fn kept(account: &str, locator: &str) -> ReviewedMessage {
        ReviewedMessage {
            account_id: account.to_owned(),
            message_locator: locator.to_owned(),
            decision: ReviewDecision::Kept,
            reviewed: true,
        }
    }

    fn pending(account: &str, locator: &str) -> ReviewedMessage {
        ReviewedMessage {
            account_id: account.to_owned(),
            message_locator: locator.to_owned(),
            decision: ReviewDecision::Pending,
            reviewed: false,
        }
    }

    fn fixture() -> Vec<ReviewedMessage> {
        vec![
            marked("discord-account-alpha-1444", "blocked-local-reference-1"),
            kept("discord-account-alpha-1444", "blocked-local-reference-2"),
            marked("telegram-account-beta-1444", "blocked-local-reference-3"),
            pending("telegram-account-beta-1444", "blocked-local-reference-4"),
        ]
    }

    #[test]
    fn task_1449_free_is_refused_and_pro_receives_the_marked_count_before_confirmation() {
        let messages = fixture();

        let free = count_marked_messages(ProductTier::Free, &messages);
        assert_eq!(free, Err(MarkedDeletionError::ProRequired));
        let free_delete = delete_marked_messages(ProductTier::Free, &messages, "any-token", true);
        assert_eq!(free_delete, Err(MarkedDeletionError::ProRequired));
        println!(
            "TASK1449_FREE_COUNT refused={} code={} message=\"{}\"",
            free.is_err(),
            MarkedDeletionError::ProRequired.code(),
            MarkedDeletionError::ProRequired.message()
        );

        let count = count_marked_messages(ProductTier::Pro, &messages)
            .expect("Pro receives the marked count");
        assert_eq!(count.marked_count, 2);
        assert_eq!(count.kept_count, 1);
        assert_eq!(count.pending_count, 1);
        assert!(count.confirmation_required);
        assert_eq!(count.confirmation_token.len(), 64);
        assert_eq!(
            count.confirmation_prompt,
            "Delete 2 marked messages? This cannot be undone."
        );
        println!(
            "TASK1449_PRO_COUNT marked_count={} kept_count={} pending_count={} prompt=\"{}\" confirmation_required={}",
            count.marked_count,
            count.kept_count,
            count.pending_count,
            count.confirmation_prompt,
            count.confirmation_required
        );

        // The count reaches the requester before any confirmation: a delete
        // attempted without the token step 1 issued is refused.
        let no_token = delete_marked_messages(ProductTier::Pro, &messages, "", true);
        assert_eq!(no_token, Err(MarkedDeletionError::CountNotShown));
        let unconfirmed = delete_marked_messages(
            ProductTier::Pro,
            &messages,
            &count.confirmation_token,
            false,
        );
        assert_eq!(unconfirmed, Err(MarkedDeletionError::NotConfirmed));

        let outcome =
            delete_marked_messages(ProductTier::Pro, &messages, &count.confirmation_token, true)
                .expect("Pro deletes after the count and the confirmation");
        assert_eq!(outcome.deleted_count, count.marked_count);
        assert_eq!(outcome.kept_untouched_count, 2);
        println!(
            "TASK1449_PRO_DELETE deleted_count={} kept_untouched_count={}",
            outcome.deleted_count, outcome.kept_untouched_count
        );
    }

    #[test]
    fn task_1449_only_reviewed_marked_messages_are_counted_and_deleted() {
        let messages = fixture();
        let count =
            count_marked_messages(ProductTier::Pro, &messages).expect("count for the fixture");
        assert_eq!(
            count.messages,
            vec![
                MarkedMessageRef {
                    account_id: "discord-account-alpha-1444".to_owned(),
                    message_locator: "blocked-local-reference-1".to_owned(),
                },
                MarkedMessageRef {
                    account_id: "telegram-account-beta-1444".to_owned(),
                    message_locator: "blocked-local-reference-3".to_owned(),
                },
            ]
        );
        assert_eq!(
            count.account_counts,
            vec![
                MarkedAccountCount {
                    account_id: "discord-account-alpha-1444".to_owned(),
                    marked_count: 1,
                },
                MarkedAccountCount {
                    account_id: "telegram-account-beta-1444".to_owned(),
                    marked_count: 1,
                },
            ]
        );

        let outcome =
            delete_marked_messages(ProductTier::Pro, &messages, &count.confirmation_token, true)
                .expect("delete after count");
        assert_eq!(outcome.deleted, count.messages);
    }

    #[test]
    fn task_1449_marked_without_review_refuses_the_whole_request() {
        let mut messages = fixture();
        messages.push(ReviewedMessage {
            account_id: "discord-account-alpha-1444".to_owned(),
            message_locator: "blocked-local-reference-5".to_owned(),
            decision: ReviewDecision::MarkedForDeletion,
            reviewed: false,
        });
        assert_eq!(
            count_marked_messages(ProductTier::Pro, &messages),
            Err(MarkedDeletionError::MarkedButNotReviewed)
        );
        assert_eq!(
            delete_marked_messages(ProductTier::Pro, &messages, "token", true),
            Err(MarkedDeletionError::MarkedButNotReviewed)
        );
    }

    #[test]
    fn task_1449_a_stale_count_cannot_confirm_a_changed_marked_set() {
        let messages = fixture();
        let count = count_marked_messages(ProductTier::Pro, &messages).expect("first count");

        let mut changed = messages.clone();
        changed.push(marked(
            "discord-account-alpha-1444",
            "blocked-local-reference-9",
        ));
        assert_eq!(
            delete_marked_messages(ProductTier::Pro, &changed, &count.confirmation_token, true),
            Err(MarkedDeletionError::CountChanged)
        );

        let recounted = count_marked_messages(ProductTier::Pro, &changed).expect("second count");
        assert_eq!(recounted.marked_count, 3);
        assert_ne!(recounted.confirmation_token, count.confirmation_token);
        assert_eq!(
            delete_marked_messages(
                ProductTier::Pro,
                &changed,
                &recounted.confirmation_token,
                true
            )
            .expect("delete after recount")
            .deleted_count,
            3
        );
    }

    #[test]
    fn task_1449_nothing_marked_is_refused_for_pro() {
        let messages = vec![
            kept("discord-account-alpha-1444", "blocked-local-reference-1"),
            pending("telegram-account-beta-1444", "blocked-local-reference-4"),
        ];
        assert_eq!(
            count_marked_messages(ProductTier::Pro, &messages),
            Err(MarkedDeletionError::NothingMarked)
        );
    }

    #[test]
    fn task_1449_json_command_refuses_free_and_reports_the_count_for_pro() {
        let messages = serde_json::to_string(&fixture()).expect("fixture json");
        let free = run_pro_marked_deletion_command(
            "count",
            &format!("{{\"plan\":\"free\",\"messages\":{messages}}}"),
        );
        let free_value: serde_json::Value = serde_json::from_str(&free).expect("free reply json");
        assert_eq!(free_value["ok"], serde_json::json!(false));
        assert_eq!(free_value["errorCode"], serde_json::json!("pro_required"));
        assert_eq!(free_value["error"], serde_json::json!(PRO_REQUIRED_REFUSAL));
        assert!(free_value.get("result").is_none());
        println!("TASK1449_JSON_FREE={free}");

        let pro = run_pro_marked_deletion_command(
            "count",
            &format!("{{\"plan\":\"pro\",\"messages\":{messages}}}"),
        );
        let pro_value: serde_json::Value = serde_json::from_str(&pro).expect("pro reply json");
        assert_eq!(pro_value["ok"], serde_json::json!(true));
        assert_eq!(pro_value["result"]["markedCount"], serde_json::json!(2));
        assert_eq!(
            pro_value["result"]["confirmationRequired"],
            serde_json::json!(true)
        );
        println!("TASK1449_JSON_PRO={pro}");
    }
}
