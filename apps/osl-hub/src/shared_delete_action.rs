//! One delete action per app, reached by two different approvals.
//!
//! ## What this module owns
//!
//! OSL removes a message from a chat app for exactly two reasons, and they
//! arrive from opposite directions:
//!
//! * a **confirmed Scrub mark** — a person looked at a finding the review
//!   produced and said "remove that one", and
//! * a **due timed-delete record** — a timer OSL already accepted (TASK 3306,
//!   [`crate::message_expiry`]) whose promised `delete_at` has arrived.
//!
//! Neither of those is a delete. Each is an *approval* naming one exact target
//! (`app` + `conversation` + `locator`). The removal itself is one action per
//! app — the thing that actually reaches into Discord, or WhatsApp, and takes
//! the row away. That action is written once per app and both approvals go
//! through it, so there is no second, quieter delete path that a timer can use
//! and a Scrub mark cannot.
//!
//! ## The rules, in order
//!
//! 1. **The target must be a well-formed, opaque triple.** Same alphabet as the
//!    timed-delete ledger's own ids, so a record that ledger accepted is always
//!    a target this module accepts.
//! 2. **Something must approve this exact target.** A confirmed mark for a
//!    different message, an unconfirmed mark, or a record whose deadline has
//!    not arrived approves nothing. No approval is [`REFUSAL_NOT_APPROVED`].
//! 3. **The target must be the signed-in account's own message.** This calls
//!    [`crate::privacy_scan::did_signed_in_account_send_message`] — the shared
//!    owner check — rather than comparing senders itself.
//! 4. **The app must have a delete action.** Exactly one, resolved from
//!    [`PerAppDeleteActions`].
//!
//! Approval is checked before ownership on purpose: an unapproved row is not a
//! message this module is about at all, whoever sent it. The order matters only
//! for a row that breaks both rules, and it then reports
//! [`REFUSAL_NOT_APPROVED`].
//!
//! Nothing here opens, parses or logs message content: a locator is carried
//! through opaquely, and a refusal names the target, never the text.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::privacy_scan::{did_signed_in_account_send_message, MessageOwnerCheckInput};

/// Longest opaque identifier a target component may be.
///
/// Same bound as the timed-delete ledger's `MAX_ID_LEN`, so every record that
/// ledger stores names a target this module can accept.
pub const MAX_TARGET_ID_LEN: usize = 96;

/// Nothing named this exact target: no confirmed Scrub mark, no due record.
pub const REFUSAL_NOT_APPROVED: &str = "not_approved";
/// The signed-in account did not send this message.
pub const REFUSAL_NOT_YOURS: &str = "not_yours";
/// The owner check could not tell who sent it, so it is not assumed.
pub const REFUSAL_OWNER_UNKNOWN: &str = "owner_unknown";
/// The app has no delete action registered.
pub const REFUSAL_NO_DELETE_ACTION: &str = "no_delete_action";
/// A second delete action was offered for an app that already has one.
pub const REFUSAL_DUPLICATE_DELETE_ACTION: &str = "duplicate_delete_action";
/// A target or approval field is empty, oversized or outside the id alphabet.
pub const REFUSAL_INVALID_FIELD: &str = "invalid_field";
/// The app's own delete action failed.
pub const REFUSAL_APP_DELETE_FAILED: &str = "app_delete_failed";

/// One exact message a delete may be asked for.
#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeleteTarget {
    pub app: String,
    pub conversation: String,
    pub locator: String,
}

impl DeleteTarget {
    pub fn new(
        app: impl Into<String>,
        conversation: impl Into<String>,
        locator: impl Into<String>,
    ) -> Self {
        Self {
            app: app.into(),
            conversation: conversation.into(),
            locator: locator.into(),
        }
    }

    fn is_well_formed(&self) -> bool {
        is_opaque_target_id(&self.app)
            && is_opaque_target_id(&self.conversation)
            && is_opaque_target_id(&self.locator)
    }

    /// Rendered for evidence and refusal text. Never carries message content.
    pub fn label(&self) -> String {
        format!("{}/{}/{}", self.app, self.conversation, self.locator)
    }
}

/// The same opaque-id alphabet the timed-delete ledger keys on.
fn is_opaque_target_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_TARGET_ID_LEN
        && value.bytes().all(|byte| {
            byte.is_ascii_digit() || byte == b'-' || byte == b':' || byte.is_ascii_lowercase()
        })
}

/// What a Scrub review left on one message.
///
/// `confirmed` is carried separately from the target on purpose: a mark the
/// review produced but nobody confirmed approves nothing, so a review that ran
/// unattended cannot delete by itself.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScrubMark {
    pub target: DeleteTarget,
    pub confirmed: bool,
}

impl ScrubMark {
    pub fn confirmed(target: DeleteTarget) -> Self {
        Self {
            target,
            confirmed: true,
        }
    }

    pub fn unconfirmed(target: DeleteTarget) -> Self {
        Self {
            target,
            confirmed: false,
        }
    }
}

/// Whether the carrier row is already protected by OSL encryption.
///
/// Mirrors `message_expiry::TimedDeleteProtection` so a stored record reads
/// straight into [`TimedDeleteRecordInput`].
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TimedDeleteProtection {
    Protected,
    Ordinary,
}

impl TimedDeleteProtection {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Protected => "protected",
            Self::Ordinary => "ordinary",
        }
    }
}

/// One timed-delete record, exactly as TASK 3306's ledger stores it.
///
/// The field names, their order, `deny_unknown_fields` and the snake_case
/// `protection` values are copied from `message_expiry::TimedDeleteRecord`, so
/// a record read back out of that sealed ledger deserializes into this type
/// unchanged. It is mirrored rather than imported because `message_expiry.rs`
/// does not currently compile in this tree (merge damage predating this
/// module); `timed_delete_record_wire_shape_matches_task_3306` pins the shape.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TimedDeleteRecordInput {
    pub app: String,
    pub conversation: String,
    pub locator: String,
    pub sent_at: i64,
    pub delete_at: i64,
    pub protection: TimedDeleteProtection,
}

impl TimedDeleteRecordInput {
    /// The target this record promises to remove.
    pub fn target(&self) -> DeleteTarget {
        DeleteTarget::new(
            self.app.clone(),
            self.conversation.clone(),
            self.locator.clone(),
        )
    }

    /// A record is due once its promised deadline has arrived.
    pub fn is_due_at(&self, now_unix_secs: i64) -> bool {
        now_unix_secs >= self.delete_at
    }
}

/// Which of the two authorities approved a removal.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeleteApproval {
    /// A person confirmed the Scrub review's mark on this exact message.
    ConfirmedScrubMark,
    /// A timer OSL already accepted for this exact message came due.
    DueTimedDelete,
}

impl DeleteApproval {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ConfirmedScrubMark => "scrub_mark",
            Self::DueTimedDelete => "timed_delete_due",
        }
    }
}

/// Everything one delete request may lean on.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeleteRequest {
    pub target: DeleteTarget,
    pub signed_in_account_sender: Option<String>,
    pub message_sender: Option<String>,
    /// What the Scrub review left on this message, if it reached it at all.
    #[serde(default)]
    pub scrub_mark: Option<ScrubMark>,
    /// The timed-delete records currently held for this account.
    #[serde(default)]
    pub timed_delete_records: Vec<TimedDeleteRecordInput>,
    pub now_unix_secs: i64,
}

/// A refusal, named so a caller and a screen can agree on the reason.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeleteRefusal {
    pub code: &'static str,
    pub message: String,
}

impl DeleteRefusal {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for DeleteRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for DeleteRefusal {}

/// What one accepted delete did.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeleteOutcome {
    pub target: DeleteTarget,
    pub approval: DeleteApproval,
    /// The one delete action the app has, by name.
    pub action_name: String,
}

/// The single removal an app knows how to perform.
///
/// A service fills in this trait once. It is deliberately narrow: it receives
/// an already-approved, already-owned target and does nothing but remove it.
/// Every rule that decides whether a removal may happen lives above it, so a
/// new app cannot accidentally ship its own weaker check.
pub trait AppDeleteAction {
    /// The app this action removes messages from.
    fn app(&self) -> &str;

    /// The name both approvals reach this action by.
    fn action_name(&self) -> &str;

    /// Remove the target. Called only after every rule above has passed.
    fn delete_message(&mut self, target: &DeleteTarget) -> Result<(), String>;
}

/// The delete actions the product has: at most one per app, by construction.
///
/// Keyed by app id, so a second action for an app cannot be held at all —
/// [`register`](Self::register) refuses it rather than replacing the first, so
/// the count for a registered app is always exactly 1.
#[derive(Default)]
pub struct PerAppDeleteActions {
    actions: BTreeMap<String, Box<dyn AppDeleteAction>>,
}

impl PerAppDeleteActions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, action: Box<dyn AppDeleteAction>) -> Result<(), DeleteRefusal> {
        let app = action.app().to_owned();
        if !is_opaque_target_id(&app) || action.action_name().is_empty() {
            return Err(DeleteRefusal::new(
                REFUSAL_INVALID_FIELD,
                "delete action app or name is invalid",
            ));
        }
        if let Some(existing) = self.actions.get(&app) {
            return Err(DeleteRefusal::new(
                REFUSAL_DUPLICATE_DELETE_ACTION,
                format!(
                    "app {app} already has the delete action {}",
                    existing.action_name()
                ),
            ));
        }
        self.actions.insert(app, action);
        Ok(())
    }

    /// Every app that has a delete action, in a stable order.
    pub fn apps(&self) -> Vec<&str> {
        self.actions.keys().map(String::as_str).collect()
    }

    /// How many delete actions an app has. Never more than 1.
    pub fn action_count_for_app(&self, app: &str) -> usize {
        usize::from(self.actions.contains_key(app))
    }

    pub fn action_name_for_app(&self, app: &str) -> Option<&str> {
        self.actions.get(app).map(|action| action.action_name())
    }

    /// Every delete action an app has, by name, so a count of 1 can be audited
    /// as a name rather than only as a number.
    pub fn action_names_for_app(&self, app: &str) -> Vec<&str> {
        self.actions
            .get(app)
            .map(|action| vec![action.action_name()])
            .unwrap_or_default()
    }

    pub fn total_action_count(&self) -> usize {
        self.actions.len()
    }

    fn action_for_app_mut(
        &mut self,
        app: &str,
    ) -> Result<&mut Box<dyn AppDeleteAction>, DeleteRefusal> {
        self.actions.get_mut(app).ok_or_else(|| {
            DeleteRefusal::new(
                REFUSAL_NO_DELETE_ACTION,
                format!("app {app} has no delete action"),
            )
        })
    }
}

/// Decide whether anything approves removing this exact target.
///
/// A confirmed Scrub mark is checked first because it is a person's decision
/// about this message specifically; a due record is the standing promise.
pub fn approve_delete(request: &DeleteRequest) -> Result<DeleteApproval, DeleteRefusal> {
    if !request.target.is_well_formed() {
        return Err(DeleteRefusal::new(
            REFUSAL_INVALID_FIELD,
            "delete target is not a well-formed app, conversation and message",
        ));
    }

    if let Some(mark) = &request.scrub_mark {
        if !mark.target.is_well_formed() {
            return Err(DeleteRefusal::new(
                REFUSAL_INVALID_FIELD,
                "scrub mark names a target that is not well formed",
            ));
        }
        if mark.confirmed && mark.target == request.target {
            return Ok(DeleteApproval::ConfirmedScrubMark);
        }
    }

    for record in &request.timed_delete_records {
        let target = record.target();
        if !target.is_well_formed() || record.delete_at <= record.sent_at || record.sent_at < 0 {
            return Err(DeleteRefusal::new(
                REFUSAL_INVALID_FIELD,
                "timed-delete record is not well formed",
            ));
        }
        if target == request.target && record.is_due_at(request.now_unix_secs) {
            return Ok(DeleteApproval::DueTimedDelete);
        }
    }

    Err(DeleteRefusal::new(
        REFUSAL_NOT_APPROVED,
        format!(
            "message {} has no confirmed Scrub mark and no due timed-delete record",
            request.target.label()
        ),
    ))
}

/// Confirm the signed-in account sent the target, using the shared owner check.
fn require_owned(request: &DeleteRequest) -> Result<(), DeleteRefusal> {
    let owned = did_signed_in_account_send_message(MessageOwnerCheckInput {
        signed_in_account_sender: request.signed_in_account_sender.clone(),
        message_sender: request.message_sender.clone(),
    })
    .map_err(|error| DeleteRefusal::new(REFUSAL_OWNER_UNKNOWN, error.to_string()))?;

    if !owned {
        return Err(DeleteRefusal::new(
            REFUSAL_NOT_YOURS,
            format!(
                "message {} is not yours: the signed-in account did not send it",
                request.target.label()
            ),
        ));
    }
    Ok(())
}

/// Send one approved, owned target through its app's single delete action.
///
/// This is the only way either authority reaches an app. A refusal returns
/// before [`AppDeleteAction::delete_message`] is called at all, so a refused
/// request never touches the app.
pub fn delete_one_owned_target(
    actions: &mut PerAppDeleteActions,
    request: &DeleteRequest,
) -> Result<DeleteOutcome, DeleteRefusal> {
    let approval = approve_delete(request)?;
    require_owned(request)?;

    let action = actions.action_for_app_mut(&request.target.app)?;
    let action_name = action.action_name().to_owned();
    action
        .delete_message(&request.target)
        .map_err(|error| DeleteRefusal::new(REFUSAL_APP_DELETE_FAILED, error))?;

    Ok(DeleteOutcome {
        target: request.target.clone(),
        approval,
        action_name,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const ME: &str = "task-3338-signed-in-account";
    const THEM: &str = "task-3338-other-account";

    struct RecordingAction {
        app: String,
        action_name: String,
        messages: Vec<DeleteTarget>,
        calls: Vec<DeleteTarget>,
    }

    impl RecordingAction {
        fn new(app: &str, messages: Vec<DeleteTarget>) -> Self {
            Self {
                app: app.to_owned(),
                action_name: format!("{app}.delete-own-message"),
                messages,
                calls: Vec::new(),
            }
        }
    }

    impl AppDeleteAction for RecordingAction {
        fn app(&self) -> &str {
            &self.app
        }

        fn action_name(&self) -> &str {
            &self.action_name
        }

        fn delete_message(&mut self, target: &DeleteTarget) -> Result<(), String> {
            self.calls.push(target.clone());
            let before = self.messages.len();
            self.messages.retain(|held| held != target);
            if self.messages.len() == before {
                return Err(format!("{} is not in this place", target.label()));
            }
            Ok(())
        }
    }

    fn discord_target(locator: &str) -> DeleteTarget {
        DeleteTarget::new("discord", "dm:task-3338", locator)
    }

    fn request(target: DeleteTarget, sender: &str) -> DeleteRequest {
        DeleteRequest {
            target,
            signed_in_account_sender: Some(ME.to_owned()),
            message_sender: Some(sender.to_owned()),
            scrub_mark: None,
            timed_delete_records: Vec::new(),
            now_unix_secs: 1_900_000_000,
        }
    }

    fn due_record(target: &DeleteTarget) -> TimedDeleteRecordInput {
        TimedDeleteRecordInput {
            app: target.app.clone(),
            conversation: target.conversation.clone(),
            locator: target.locator.clone(),
            sent_at: 1_899_000_000,
            delete_at: 1_899_900_000,
            protection: TimedDeleteProtection::Protected,
        }
    }

    fn actions() -> PerAppDeleteActions {
        let mut actions = PerAppDeleteActions::new();
        actions
            .register(Box::new(RecordingAction::new(
                "discord",
                vec![
                    discord_target("task-3338-marked"),
                    discord_target("task-3338-timer"),
                    discord_target("task-3338-plain"),
                    discord_target("task-3338-theirs"),
                ],
            )))
            .expect("first discord action registers");
        actions
    }

    #[test]
    fn a_confirmed_scrub_mark_approves_the_message_it_names() {
        let target = discord_target("task-3338-marked");
        let mut request = request(target.clone(), ME);
        request.scrub_mark = Some(ScrubMark::confirmed(target.clone()));

        let outcome = delete_one_owned_target(&mut actions(), &request).expect("approved");
        assert_eq!(outcome.approval, DeleteApproval::ConfirmedScrubMark);
        assert_eq!(outcome.action_name, "discord.delete-own-message");
        assert_eq!(outcome.target, target);
    }

    #[test]
    fn a_due_timed_delete_record_approves_an_unmarked_message() {
        let target = discord_target("task-3338-timer");
        let mut request = request(target.clone(), ME);
        request.timed_delete_records = vec![due_record(&target)];

        let outcome = delete_one_owned_target(&mut actions(), &request).expect("approved");
        assert_eq!(outcome.approval, DeleteApproval::DueTimedDelete);
        assert_eq!(outcome.action_name, "discord.delete-own-message");
    }

    #[test]
    fn an_unmarked_message_with_no_due_record_is_refused() {
        let target = discord_target("task-3338-plain");
        let request = request(target, ME);

        let refusal = delete_one_owned_target(&mut actions(), &request).expect_err("refused");
        assert_eq!(refusal.code, REFUSAL_NOT_APPROVED);
    }

    #[test]
    fn another_persons_message_is_refused_even_when_marked() {
        let target = discord_target("task-3338-theirs");
        let mut request = request(target.clone(), THEM);
        request.scrub_mark = Some(ScrubMark::confirmed(target));

        let refusal = delete_one_owned_target(&mut actions(), &request).expect_err("refused");
        assert_eq!(refusal.code, REFUSAL_NOT_YOURS);
    }

    #[test]
    fn an_unconfirmed_mark_is_not_an_approval() {
        let target = discord_target("task-3338-marked");
        let mut request = request(target.clone(), ME);
        request.scrub_mark = Some(ScrubMark::unconfirmed(target));

        let refusal = approve_delete(&request).expect_err("refused");
        assert_eq!(refusal.code, REFUSAL_NOT_APPROVED);
    }

    #[test]
    fn a_mark_or_record_for_another_message_approves_nothing() {
        let target = discord_target("task-3338-plain");
        let other = discord_target("task-3338-marked");
        let mut request = request(target, ME);
        request.scrub_mark = Some(ScrubMark::confirmed(other.clone()));
        request.timed_delete_records = vec![due_record(&other)];

        let refusal = approve_delete(&request).expect_err("refused");
        assert_eq!(refusal.code, REFUSAL_NOT_APPROVED);
    }

    #[test]
    fn a_record_whose_deadline_has_not_arrived_is_not_due() {
        let target = discord_target("task-3338-timer");
        let mut request = request(target.clone(), ME);
        request.timed_delete_records = vec![due_record(&target)];
        request.now_unix_secs = 1_899_899_999;

        let refusal = approve_delete(&request).expect_err("refused");
        assert_eq!(refusal.code, REFUSAL_NOT_APPROVED);
    }

    #[test]
    fn an_app_holds_exactly_one_delete_action() {
        let mut actions = actions();
        assert_eq!(actions.action_count_for_app("discord"), 1);

        let refusal = actions
            .register(Box::new(RecordingAction::new("discord", Vec::new())))
            .expect_err("second discord action refused");
        assert_eq!(refusal.code, REFUSAL_DUPLICATE_DELETE_ACTION);
        assert_eq!(actions.action_count_for_app("discord"), 1);
        assert_eq!(actions.total_action_count(), 1);
    }

    #[test]
    fn an_app_with_no_delete_action_is_refused_rather_than_guessed() {
        let target = DeleteTarget::new("whatsapp", "chat:task-3338", "task-3338-marked");
        let mut request = request(target.clone(), ME);
        request.scrub_mark = Some(ScrubMark::confirmed(target));

        let refusal = delete_one_owned_target(&mut actions(), &request).expect_err("refused");
        assert_eq!(refusal.code, REFUSAL_NO_DELETE_ACTION);
    }

    #[test]
    fn timed_delete_record_wire_shape_matches_task_3306() {
        // Field-for-field the JSON `message_expiry::TimedDeleteRecord` serializes
        // to, including `deny_unknown_fields` and the snake_case protection.
        let stored = r#"{"app":"discord","conversation":"dm:task-3338","locator":"task-3338-timer","sent_at":1899000000,"delete_at":1899900000,"protection":"protected"}"#;
        let record: TimedDeleteRecordInput =
            serde_json::from_str(stored).expect("3306 record reads back");
        assert_eq!(record.target(), discord_target("task-3338-timer"));
        assert_eq!(record.protection.as_str(), "protected");
        assert_eq!(serde_json::to_string(&record).expect("re-encodes"), stored);
    }
}
