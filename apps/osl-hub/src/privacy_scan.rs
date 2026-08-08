//! Bounded, deterministic, local-only message risk scanning.
//!
//! This module deliberately has no HTTP/model dependency and performs no I/O.
//! A trusted caller may provide messages from an explicit local export or data
//! already visible to the signed-in user. The separate Scrub index can persist
//! those inputs encrypted and deterministically reproduce findings from disk.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashSet};

use crate::attachment_scan::{
    scan_attachments, AttachmentAnalyzers, LocalAttachmentCandidate, UninspectedAttachment,
    MAX_ATTACHMENTS_PER_MESSAGE, MAX_ATTACHMENT_BYTES,
};

const MAX_MESSAGES: usize = 2_000;
const MAX_TEXT_BYTES: usize = 8 * 1024;
const MAX_LOCATOR_BYTES: usize = 256;
const MAX_PREVIEW_CHARS: usize = 120;
const MAX_SENDER_BYTES: usize = 256;
const MAX_EMAIL_RECIPIENTS: usize = 64;
const MAX_EMAIL_RECIPIENT_BYTES: usize = 256;
const MAX_EMAIL_BURN_ID_BYTES: usize = 256;
const MAX_ATTACHMENT_ID_BYTES: usize = 128;
const MAX_ATTACHMENT_DISPLAY_NAME_BYTES: usize = 256;
const MAX_ATTACHMENT_ENCODED_BYTES: usize = MAX_ATTACHMENT_BYTES.div_ceil(3) * 4;
pub(crate) const MAX_ATTACHMENT_BATCH_ENCODED_BYTES: usize = 12 * 1024 * 1024;
pub(crate) const MAX_FINDINGS: usize = 1_000;
pub const GMAIL_ORDINARY_ATTACHMENT_LIMIT_MB: u64 = 25;
pub const GMAIL_ORDINARY_ATTACHMENT_LIMIT_BYTES: u64 =
    GMAIL_ORDINARY_ATTACHMENT_LIMIT_MB * 1024 * 1024;
pub const MAIL_DOT_COM_FREE_ORDINARY_ATTACHMENT_LIMIT_MB: u64 = 30;
pub const MAIL_DOT_COM_PREMIUM_ORDINARY_ATTACHMENT_LIMIT_MB: u64 = 100;
pub const EXCHANGE_ORDINARY_ATTACHMENT_LIMIT_MB: u64 = 150;
const MAIL_ATTACHMENT_MIB: u64 = 1024 * 1024;
const MAILCOM_FREE_ORDINARY_ATTACHMENT_LIMIT_MB: u32 = 30;
const MAILCOM_PREMIUM_ORDINARY_ATTACHMENT_LIMIT_MB: u32 = 100;
const EXCHANGE_DEFAULT_ORDINARY_ATTACHMENT_LIMIT_MB: u32 = 10;
const BLOCKED_JUMP_REFERENCE: &str = "blocked-local-reference";

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LocalMessageCandidate {
    pub service_id: String,
    pub account_id: String,
    pub conversation_id: String,
    pub message_locator: String,
    pub authored_by_self: bool,
    pub created_at_unix_ms: Option<i64>,
    pub text: String,
    #[serde(default)]
    pub reply_recipient: Option<String>,
    #[serde(default)]
    pub visible_recipients: Vec<String>,
    #[serde(default)]
    pub hidden_recipients: Vec<String>,
    #[serde(default)]
    pub email_thread_identity: Option<String>,
    #[serde(default)]
    pub email_folder_identity: Option<String>,
    #[serde(default)]
    pub attachments: Vec<LocalAttachmentCandidate>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ServiceFoundMessageInput {
    pub service_id: String,
    pub sender: Option<String>,
    pub signed_in_account_sender: Option<String>,
    pub sent_at_unix_ms: i64,
    pub place: String,
    pub text: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FoundMessageRecord {
    pub sender: String,
    pub sent_at_unix_ms: i64,
    pub place: String,
    pub text: String,
    pub sent_by_signed_in_account: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MessageOwnerCheckInput {
    pub signed_in_account_sender: Option<String>,
    pub message_sender: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MessageOwnerCheckError {
    UnknownSignedInAccountSender,
    UnknownMessageSender,
    InvalidSender,
}

impl std::fmt::Display for MessageOwnerCheckError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownSignedInAccountSender => {
                f.write_str("signed-in account sender is unknown")
            }
            Self::UnknownMessageSender => f.write_str("message sender is unknown"),
            Self::InvalidSender => f.write_str("message owner check sender is invalid"),
        }
    }
}

impl std::error::Error for MessageOwnerCheckError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FoundMessageRecordError {
    MissingSender,
    MissingSignedInAccountSender,
    InvalidField,
}

impl std::fmt::Display for FoundMessageRecordError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingSender => f.write_str("found message sender is missing"),
            Self::MissingSignedInAccountSender => {
                f.write_str("signed-in account sender is missing")
            }
            Self::InvalidField => f.write_str("found message record field is invalid"),
        }
    }
}

impl std::error::Error for FoundMessageRecordError {}

impl From<MessageOwnerCheckError> for FoundMessageRecordError {
    fn from(value: MessageOwnerCheckError) -> Self {
        match value {
            MessageOwnerCheckError::UnknownMessageSender => Self::MissingSender,
            MessageOwnerCheckError::UnknownSignedInAccountSender => {
                Self::MissingSignedInAccountSender
            }
            MessageOwnerCheckError::InvalidSender => Self::InvalidField,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrdinaryAttachmentSetItem {
    pub display_name: String,
    pub size_bytes: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OrdinaryAttachmentLimitProfile {
    MailcomFree,
    MailcomPremium,
    Exchange { company_limit_mb: Option<u32> },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrdinaryAttachmentSetDecision {
    pub accepted: bool,
    pub provider_label: String,
    pub limit_mb: u32,
    pub total_mb: u64,
    pub rejected_attachment_name: Option<String>,
    pub refusal: Option<String>,
}

impl OrdinaryAttachmentLimitProfile {
    fn provider_label(self) -> String {
        match self {
            Self::MailcomFree => "Mail.com Free".to_owned(),
            Self::MailcomPremium => "Mail.com Premium".to_owned(),
            Self::Exchange {
                company_limit_mb: None,
            } => "Exchange default".to_owned(),
            Self::Exchange {
                company_limit_mb: Some(limit_mb),
            } => format!("Exchange company {limit_mb} MB"),
        }
    }

    fn limit_mb(self) -> u32 {
        match self {
            Self::MailcomFree => MAILCOM_FREE_ORDINARY_ATTACHMENT_LIMIT_MB,
            Self::MailcomPremium => MAILCOM_PREMIUM_ORDINARY_ATTACHMENT_LIMIT_MB,
            Self::Exchange { company_limit_mb } => {
                company_limit_mb.unwrap_or(EXCHANGE_DEFAULT_ORDINARY_ATTACHMENT_LIMIT_MB)
            }
        }
    }
}

#[derive(Clone, Copy, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PrivacyRiskCategory {
    Credential,
    RecoveryMaterial,
    PaymentCard,
    GovernmentIdentity,
    PreciseLocation,
    Profanity,
    SexualContent,
    SensitiveHealth,
    ControlledSubstances,
    PotentiallyUnlawfulConduct,
    WorkSensitiveInformation,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalPrivacyFinding {
    pub service_id: String,
    pub account_id: String,
    pub conversation_id: String,
    pub message_locator: String,
    pub authored_by_self: bool,
    pub created_at_unix_ms: Option<i64>,
    pub category: PrivacyRiskCategory,
    pub confidence: u8,
    pub reason: &'static str,
    pub local_preview: String,
    pub can_request_delete: bool,
    pub attachment_path: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalPrivacyScanResult {
    pub findings: Vec<LocalPrivacyFinding>,
    pub email_protection_checks: Vec<EmailProtectionCheckDisplay>,
    pub email_burn_target_lists: Vec<EmailBurnTargetListDisplay>,
    pub messages_scanned: usize,
    pub messages_rejected: usize,
    pub truncated: bool,
    pub analysis_location: &'static str,
    pub persisted: bool,
    pub attachments_scanned: usize,
    pub images_checked: bool,
    pub videos_checked: bool,
    pub attachment_types_scanned: Vec<String>,
    pub uninspected_attachments: Vec<UninspectedAttachment>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedReviewMatch {
    pub full_text: String,
    pub reason: &'static str,
    pub service: String,
    pub place: String,
    pub date: String,
    pub time: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EmailProtectionCheckDisplay {
    pub message_locator: String,
    pub reply_recipients: Vec<String>,
    pub reply_all_recipients: Vec<String>,
    pub visible_recipients: Vec<String>,
    pub distinct_recipient_count: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EmailBurnScope {
    Thread,
    Folder,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewResultAccountGroup {
    pub account_id: String,
    pub matches: Vec<SavedReviewMatch>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EmailBurnTargetListDisplay {
    pub scope: EmailBurnScope,
    pub identity: String,
    pub message_locators: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ProtectedEmailReplyAction {
    Reply,
    ReplyAll,
}

impl ProtectedEmailReplyAction {
    fn draft_kind(self) -> &'static str {
        match self {
            Self::Reply => "reply",
            Self::ReplyAll => "replyAll",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewResultStoreOutput {
    pub groups: Vec<ReviewResultAccountGroup>,
    pub total_matches: usize,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProtectedEmailReplyDraft {
    pub draft_id: String,
    pub message_locator: String,
    pub action: ProtectedEmailReplyAction,
    pub recipients: Vec<String>,
    pub protected: bool,
    pub ordinary_attachment_bytes: u64,
    pub osl_stored_file_bytes: u64,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum EmailDraftAttachmentStorage {
    Ordinary,
    OslStored,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EmailDraftAttachment {
    pub display_name: String,
    pub size_bytes: u64,
    pub storage: EmailDraftAttachmentStorage,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum EmailDraftMailLimitProfile {
    Gmail,
    MailDotComFree,
    MailDotComPremium,
    Exchange,
}

impl EmailDraftMailLimitProfile {
    fn display_name(self) -> &'static str {
        match self {
            Self::Gmail => "Gmail",
            Self::MailDotComFree => "Mail.com free",
            Self::MailDotComPremium => "Mail.com premium",
            Self::Exchange => "Exchange",
        }
    }

    fn ordinary_attachment_limit_mb(self) -> u64 {
        match self {
            Self::Gmail => GMAIL_ORDINARY_ATTACHMENT_LIMIT_MB,
            Self::MailDotComFree => MAIL_DOT_COM_FREE_ORDINARY_ATTACHMENT_LIMIT_MB,
            Self::MailDotComPremium => MAIL_DOT_COM_PREMIUM_ORDINARY_ATTACHMENT_LIMIT_MB,
            Self::Exchange => EXCHANGE_ORDINARY_ATTACHMENT_LIMIT_MB,
        }
    }

    fn ordinary_attachment_limit_bytes(self) -> u64 {
        self.ordinary_attachment_limit_mb() * 1024 * 1024
    }
}

/// Scan bounded caller-provided text entirely in process memory.
///
/// Invalid or oversized records are rejected rather than partially scanned.
/// At most one finding per category is emitted for a message. The caller must
/// still confirm that a locator belongs to the active trusted service context
/// before offering jump/delete actions.
pub fn scan_local_messages(messages: Vec<LocalMessageCandidate>) -> LocalPrivacyScanResult {
    scan_local_messages_with_analyzers(messages, AttachmentAnalyzers::default())
}

pub fn group_saved_matches_by_account(
    messages: Vec<LocalMessageCandidate>,
) -> ReviewResultStoreOutput {
    let mut grouped = BTreeMap::<String, Vec<SavedReviewMatch>>::new();
    for message in messages.into_iter().take(MAX_MESSAGES) {
        if !valid_candidate(&message) {
            continue;
        }
        let (date, time) = match message.created_at_unix_ms {
            Some(unix_ms) => utc_date_time(unix_ms),
            None => ("unknown".to_owned(), "unknown".to_owned()),
        };
        for (_, _, reason) in classify(&message.text) {
            grouped
                .entry(message.account_id.clone())
                .or_default()
                .push(SavedReviewMatch {
                    full_text: message.text.clone(),
                    reason,
                    service: service_label(&message.service_id).to_owned(),
                    place: message.conversation_id.clone(),
                    date: date.clone(),
                    time: time.clone(),
                });
        }
    }

    let total_matches = grouped.values().map(Vec::len).sum();
    let groups = grouped
        .into_iter()
        .map(|(account_id, matches)| ReviewResultAccountGroup {
            account_id,
            matches,
        })
        .collect();
    ReviewResultStoreOutput {
        groups,
        total_matches,
    }
}

pub fn name_found_message_record(
    input: ServiceFoundMessageInput,
) -> Result<FoundMessageRecord, FoundMessageRecordError> {
    if input.service_id.is_empty()
        || input.service_id.len() > 32
        || !input.service_id.bytes().all(valid_id_byte)
        || input.place.is_empty()
        || input.place.len() > 256
        || input.text.is_empty()
        || input.text.len() > MAX_TEXT_BYTES
        || input.text.contains('\0')
    {
        return Err(FoundMessageRecordError::InvalidField);
    }

    let sent_by_signed_in_account = did_signed_in_account_send_message(MessageOwnerCheckInput {
        signed_in_account_sender: input.signed_in_account_sender,
        message_sender: input.sender.clone(),
    })?;
    let sender = normalized_sender(input.sender, MessageOwnerCheckError::UnknownMessageSender)?;

    Ok(FoundMessageRecord {
        sender,
        sent_at_unix_ms: input.sent_at_unix_ms,
        place: input.place,
        text: input.text,
        sent_by_signed_in_account,
    })
}

pub fn name_found_message_records(
    inputs: Vec<ServiceFoundMessageInput>,
) -> Result<Vec<FoundMessageRecord>, FoundMessageRecordError> {
    inputs.into_iter().map(name_found_message_record).collect()
}

pub fn did_signed_in_account_send_message(
    input: MessageOwnerCheckInput,
) -> Result<bool, MessageOwnerCheckError> {
    let signed_in_account_sender = normalized_sender(
        input.signed_in_account_sender,
        MessageOwnerCheckError::UnknownSignedInAccountSender,
    )?;
    let message_sender = normalized_sender(
        input.message_sender,
        MessageOwnerCheckError::UnknownMessageSender,
    )?;

    Ok(message_sender == signed_in_account_sender)
}

/// Reject oversized attachment IPC inputs before they are cloned, decoded, or
/// passed to the parser boundary. This validates only the transport envelope;
/// byte-derived type detection and extraction remain in `attachment_scan`.
pub fn validate_attachment_input_batch(messages: &[LocalMessageCandidate]) -> Result<(), String> {
    let mut aggregate_encoded_bytes = 0usize;
    for message in messages {
        if message.attachments.len() > MAX_ATTACHMENTS_PER_MESSAGE {
            return Err(format!(
                "A message exceeds the {MAX_ATTACHMENTS_PER_MESSAGE}-attachment input limit"
            ));
        }
        for attachment in &message.attachments {
            if !valid_attachment_input(attachment) {
                return Err("Attachment metadata or base64 input is invalid".into());
            }
            aggregate_encoded_bytes = aggregate_encoded_bytes
                .checked_add(attachment.content_base64.len())
                .ok_or_else(|| "Attachment input exceeds the aggregate limit".to_owned())?;
            if aggregate_encoded_bytes > MAX_ATTACHMENT_BATCH_ENCODED_BYTES {
                return Err("Attachment input exceeds the aggregate encoded-byte limit".into());
            }
        }
    }
    Ok(())
}

pub fn check_ordinary_attachment_set_limit(
    profile: OrdinaryAttachmentLimitProfile,
    attachments: &[OrdinaryAttachmentSetItem],
) -> OrdinaryAttachmentSetDecision {
    let provider_label = profile.provider_label();
    let limit_mb = profile.limit_mb();
    let limit_bytes = u64::from(limit_mb) * MAIL_ATTACHMENT_MIB;
    let mut total_bytes = 0u64;

    for attachment in attachments {
        total_bytes = total_bytes.saturating_add(attachment.size_bytes);
        if total_bytes > limit_bytes {
            let total_mb = total_bytes.div_ceil(MAIL_ATTACHMENT_MIB);
            let refusal = format!(
                "{provider_label} refuses ordinary attachments over {limit_mb} MB: {} makes the ordinary attachment set {total_mb} MB",
                attachment.display_name
            );
            return OrdinaryAttachmentSetDecision {
                accepted: false,
                provider_label,
                limit_mb,
                total_mb,
                rejected_attachment_name: Some(attachment.display_name.clone()),
                refusal: Some(refusal),
            };
        }
    }

    OrdinaryAttachmentSetDecision {
        accepted: true,
        provider_label,
        limit_mb,
        total_mb: total_bytes.div_ceil(MAIL_ATTACHMENT_MIB),
        rejected_attachment_name: None,
        refusal: None,
    }
}

/// Scan with explicitly supplied local media capabilities. The desktop build
/// supplies none until separately installed adapters and model packs have been
/// verified, so unavailable media is recorded rather than treated as clean.
pub fn scan_local_messages_with_analyzers(
    messages: Vec<LocalMessageCandidate>,
    analyzers: AttachmentAnalyzers<'_>,
) -> LocalPrivacyScanResult {
    let mut findings = Vec::new();
    let mut email_protection_checks = Vec::new();
    let mut thread_burn_targets = BTreeMap::<String, Vec<String>>::new();
    let mut folder_burn_targets = BTreeMap::<String, Vec<String>>::new();
    let mut messages_scanned = 0usize;
    let mut messages_rejected = messages.len().saturating_sub(MAX_MESSAGES);
    let mut truncated = messages.len() > MAX_MESSAGES;
    let mut attachments_scanned = 0usize;
    let mut attachment_types_scanned = Vec::new();
    let mut uninspected_attachments = Vec::new();

    for message in messages.into_iter().take(MAX_MESSAGES) {
        if !valid_candidate(&message) {
            messages_rejected += 1;
            continue;
        }
        messages_scanned += 1;
        if let Some(check) = email_protection_check(&message) {
            email_protection_checks.push(check.display);
        }
        collect_email_burn_targets(&message, &mut thread_burn_targets, &mut folder_burn_targets);
        let mut categories = HashSet::new();
        for (category, confidence, reason) in classify(&message.text) {
            if !categories.insert((category, None::<String>)) {
                continue;
            }
            if findings.len() >= MAX_FINDINGS {
                truncated = true;
                break;
            }
            findings.push(LocalPrivacyFinding {
                service_id: message.service_id.clone(),
                account_id: message.account_id.clone(),
                conversation_id: message.conversation_id.clone(),
                message_locator: result_message_locator(&message.message_locator),
                authored_by_self: message.authored_by_self,
                created_at_unix_ms: message.created_at_unix_ms,
                category,
                confidence,
                reason,
                local_preview: result_preview(&message.text),
                can_request_delete: message.authored_by_self,
                attachment_path: None,
            });
        }

        let attachment_scan = scan_attachments(&message.attachments, analyzers);
        attachments_scanned =
            attachments_scanned.saturating_add(attachment_scan.attachments_scanned);
        attachment_types_scanned.extend(attachment_scan.attachment_types_scanned);
        uninspected_attachments.extend(attachment_scan.uninspected_attachments);

        for fragment in attachment_scan.extracted_text {
            for (category, confidence, reason) in classify(&fragment.text) {
                if !categories.insert((category, Some(fragment.path.clone()))) {
                    continue;
                }
                if findings.len() >= MAX_FINDINGS {
                    truncated = true;
                    break;
                }
                findings.push(LocalPrivacyFinding {
                    service_id: message.service_id.clone(),
                    account_id: message.account_id.clone(),
                    conversation_id: message.conversation_id.clone(),
                    message_locator: result_message_locator(&message.message_locator),
                    authored_by_self: message.authored_by_self,
                    created_at_unix_ms: message.created_at_unix_ms,
                    category,
                    confidence,
                    reason,
                    local_preview: result_preview(&fragment.text),
                    can_request_delete: message.authored_by_self,
                    attachment_path: Some(result_attachment_path(&fragment.path)),
                });
            }
        }
        for signal in attachment_scan.explicit_media_signals {
            let category = PrivacyRiskCategory::SexualContent;
            if !categories.insert((category, Some(signal.path.clone()))) {
                continue;
            }
            if findings.len() >= MAX_FINDINGS {
                truncated = true;
                break;
            }
            findings.push(LocalPrivacyFinding {
                service_id: message.service_id.clone(),
                account_id: message.account_id.clone(),
                conversation_id: message.conversation_id.clone(),
                message_locator: result_message_locator(&message.message_locator),
                authored_by_self: message.authored_by_self,
                created_at_unix_ms: message.created_at_unix_ms,
                category,
                confidence: signal.confidence,
                reason: signal.reason,
                local_preview: "Local media classification signal".into(),
                can_request_delete: message.authored_by_self,
                attachment_path: Some(result_attachment_path(&signal.path)),
            });
        }
    }

    attachment_types_scanned.sort();
    attachment_types_scanned.dedup();
    let images_checked = attachment_types_scanned.iter().any(|kind| kind == "image")
        && !uninspected_attachments
            .iter()
            .any(|item| item.detected_type == "image");
    let videos_checked = attachment_types_scanned.iter().any(|kind| kind == "video")
        && !uninspected_attachments
            .iter()
            .any(|item| item.detected_type == "video");

    LocalPrivacyScanResult {
        findings,
        email_protection_checks,
        email_burn_target_lists: email_burn_target_lists(thread_burn_targets, folder_burn_targets),
        messages_scanned,
        messages_rejected,
        truncated,
        analysis_location: "this_device_only",
        persisted: false,
        attachments_scanned,
        images_checked,
        videos_checked,
        attachment_types_scanned,
        uninspected_attachments,
    }
}

struct EmailProtectionCheck {
    display: EmailProtectionCheckDisplay,
}

fn email_protection_check(message: &LocalMessageCandidate) -> Option<EmailProtectionCheck> {
    if message.service_id != "email"
        || (message.visible_recipients.is_empty() && message.hidden_recipients.is_empty())
    {
        return None;
    }

    let distinct_recipients = message
        .visible_recipients
        .iter()
        .chain(message.hidden_recipients.iter())
        .cloned()
        .collect::<BTreeSet<_>>();

    Some(EmailProtectionCheck {
        display: EmailProtectionCheckDisplay {
            message_locator: message.message_locator.clone(),
            reply_recipients: message.reply_recipient.iter().cloned().collect::<Vec<_>>(),
            reply_all_recipients: unique_ordered_recipients(&message.visible_recipients),
            visible_recipients: message.visible_recipients.clone(),
            distinct_recipient_count: distinct_recipients.len(),
        },
    })
}

fn collect_email_burn_targets(
    message: &LocalMessageCandidate,
    thread_burn_targets: &mut BTreeMap<String, Vec<String>>,
    folder_burn_targets: &mut BTreeMap<String, Vec<String>>,
) {
    if message.service_id != "email" {
        return;
    }

    if let Some(thread_identity) = &message.email_thread_identity {
        push_unique_locator(
            thread_burn_targets
                .entry(thread_identity.clone())
                .or_default(),
            &message.message_locator,
        );
    }
    if let Some(folder_identity) = &message.email_folder_identity {
        push_unique_locator(
            folder_burn_targets
                .entry(folder_identity.clone())
                .or_default(),
            &message.message_locator,
        );
    }
}

fn push_unique_locator(targets: &mut Vec<String>, locator: &str) {
    if !targets.iter().any(|existing| existing == locator) {
        targets.push(locator.to_owned());
    }
}

fn email_burn_target_lists(
    thread_burn_targets: BTreeMap<String, Vec<String>>,
    folder_burn_targets: BTreeMap<String, Vec<String>>,
) -> Vec<EmailBurnTargetListDisplay> {
    thread_burn_targets
        .into_iter()
        .map(|(identity, message_locators)| EmailBurnTargetListDisplay {
            scope: EmailBurnScope::Thread,
            identity,
            message_locators,
        })
        .chain(
            folder_burn_targets
                .into_iter()
                .map(|(identity, message_locators)| EmailBurnTargetListDisplay {
                    scope: EmailBurnScope::Folder,
                    identity,
                    message_locators,
                }),
        )
        .collect()
}

pub fn create_protected_email_reply_draft(
    check: &EmailProtectionCheckDisplay,
    action: ProtectedEmailReplyAction,
) -> Result<ProtectedEmailReplyDraft, String> {
    create_protected_email_reply_draft_with_attachments(check, action, &[])
}

pub fn create_protected_email_reply_draft_with_attachments(
    check: &EmailProtectionCheckDisplay,
    action: ProtectedEmailReplyAction,
    attachments: &[EmailDraftAttachment],
) -> Result<ProtectedEmailReplyDraft, String> {
    create_protected_email_reply_draft_with_attachments_for_profile(
        check,
        action,
        attachments,
        EmailDraftMailLimitProfile::Gmail,
    )
}

pub fn create_protected_email_reply_draft_with_attachments_for_profile(
    check: &EmailProtectionCheckDisplay,
    action: ProtectedEmailReplyAction,
    attachments: &[EmailDraftAttachment],
    mail_limit_profile: EmailDraftMailLimitProfile,
) -> Result<ProtectedEmailReplyDraft, String> {
    let recipients = match action {
        ProtectedEmailReplyAction::Reply => check.reply_recipients.clone(),
        ProtectedEmailReplyAction::ReplyAll => check.reply_all_recipients.clone(),
    };
    if recipients.is_empty() || !valid_recipient_list(&recipients) {
        return Err("Protected email reply draft recipients are invalid".to_owned());
    }
    let attachment_bytes = email_draft_attachment_bytes(attachments, mail_limit_profile)?;
    Ok(ProtectedEmailReplyDraft {
        draft_id: format!(
            "protected-email-{}-{}",
            check.message_locator,
            action.draft_kind()
        ),
        message_locator: check.message_locator.clone(),
        action,
        recipients,
        protected: true,
        ordinary_attachment_bytes: attachment_bytes.ordinary,
        osl_stored_file_bytes: attachment_bytes.osl_stored,
    })
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct EmailDraftAttachmentBytes {
    ordinary: u64,
    osl_stored: u64,
}

fn email_draft_attachment_bytes(
    attachments: &[EmailDraftAttachment],
    mail_limit_profile: EmailDraftMailLimitProfile,
) -> Result<EmailDraftAttachmentBytes, String> {
    let mut bytes = EmailDraftAttachmentBytes::default();
    for attachment in attachments {
        if attachment.display_name.is_empty()
            || attachment.display_name.len() > MAX_ATTACHMENT_DISPLAY_NAME_BYTES
            || attachment
                .display_name
                .chars()
                .any(unsafe_attachment_metadata_char)
        {
            return Err("Email draft attachment name is invalid".to_owned());
        }

        match attachment.storage {
            EmailDraftAttachmentStorage::Ordinary => {
                bytes.ordinary = bytes
                    .ordinary
                    .checked_add(attachment.size_bytes)
                    .ok_or_else(|| mail_attachment_over_limit_message(mail_limit_profile))?;
                if bytes.ordinary > mail_limit_profile.ordinary_attachment_limit_bytes() {
                    return Err(format!(
                        "{} refuses ordinary attachments over {} MB: {} makes the ordinary attachment set {} MB",
                        mail_limit_profile.display_name(),
                        mail_limit_profile.ordinary_attachment_limit_mb(),
                        attachment.display_name,
                        bytes_to_whole_mb(bytes.ordinary),
                    ));
                }
            }
            EmailDraftAttachmentStorage::OslStored => {
                bytes.osl_stored = bytes
                    .osl_stored
                    .checked_add(attachment.size_bytes)
                    .ok_or_else(|| {
                        "OSL-stored email draft attachment total is invalid".to_owned()
                    })?;
            }
        }
    }
    Ok(bytes)
}

fn mail_attachment_over_limit_message(mail_limit_profile: EmailDraftMailLimitProfile) -> String {
    format!(
        "{} refuses ordinary attachments over {} MB",
        mail_limit_profile.display_name(),
        mail_limit_profile.ordinary_attachment_limit_mb()
    )
}

fn bytes_to_whole_mb(bytes: u64) -> u64 {
    bytes / (1024 * 1024)
}

fn unique_ordered_recipients(recipients: &[String]) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut unique = Vec::new();
    for recipient in recipients {
        if seen.insert(recipient.clone()) {
            unique.push(recipient.clone());
        }
    }
    unique
}

fn valid_candidate(message: &LocalMessageCandidate) -> bool {
    !message.service_id.is_empty()
        && message.service_id.len() <= 32
        && message.service_id.bytes().all(valid_id_byte)
        && !message.account_id.is_empty()
        && message.account_id.len() <= 128
        && !message.conversation_id.is_empty()
        && message.conversation_id.len() <= 256
        && !message.message_locator.is_empty()
        && message.message_locator.len() <= MAX_LOCATOR_BYTES
        && (!message.text.is_empty() || !message.attachments.is_empty())
        && message.text.len() <= MAX_TEXT_BYTES
        && !message.text.contains('\0')
        && message
            .reply_recipient
            .as_ref()
            .is_none_or(|recipient| valid_recipient_display_value(recipient))
        && valid_recipient_list(&message.visible_recipients)
        && valid_recipient_list(&message.hidden_recipients)
        && message
            .email_thread_identity
            .as_ref()
            .is_none_or(|identity| valid_email_burn_identity(identity))
        && message
            .email_folder_identity
            .as_ref()
            .is_none_or(|identity| valid_email_burn_identity(identity))
        && message.attachments.len() <= MAX_ATTACHMENTS_PER_MESSAGE
        && message.attachments.iter().all(valid_attachment_input)
}

fn normalized_sender(
    value: Option<String>,
    unknown_error: MessageOwnerCheckError,
) -> Result<String, MessageOwnerCheckError> {
    let sender = value
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .ok_or(unknown_error)?;
    if sender.len() > MAX_SENDER_BYTES || sender.chars().any(char::is_control) {
        return Err(MessageOwnerCheckError::InvalidSender);
    }
    Ok(sender)
}

fn valid_email_burn_identity(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_EMAIL_BURN_ID_BYTES
        && !value.chars().any(unsafe_attachment_metadata_char)
}

fn valid_recipient_list(recipients: &[String]) -> bool {
    recipients.len() <= MAX_EMAIL_RECIPIENTS
        && recipients
            .iter()
            .all(|recipient| valid_recipient_display_value(recipient))
}

fn valid_recipient_display_value(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_EMAIL_RECIPIENT_BYTES
        && !value.chars().any(|ch| {
            ch.is_control() || matches!(ch, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
        })
}

fn valid_attachment_input(value: &LocalAttachmentCandidate) -> bool {
    !value.attachment_id.is_empty()
        && value.attachment_id.len() <= MAX_ATTACHMENT_ID_BYTES
        && !value
            .attachment_id
            .chars()
            .any(unsafe_attachment_metadata_char)
        && !value.display_name.is_empty()
        && value.display_name.len() <= MAX_ATTACHMENT_DISPLAY_NAME_BYTES
        && !value
            .display_name
            .chars()
            .any(unsafe_attachment_metadata_char)
        && !value.content_base64.is_empty()
        && value.content_base64.len() <= MAX_ATTACHMENT_ENCODED_BYTES
        && valid_base64_envelope(&value.content_base64)
}

fn valid_base64_envelope(value: &str) -> bool {
    if value.len() % 4 != 0 {
        return false;
    }
    let padding = value.bytes().rev().take_while(|byte| *byte == b'=').count();
    padding <= 2
        && value[..value.len() - padding]
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'/'))
        && value[value.len() - padding..]
            .bytes()
            .all(|byte| byte == b'=')
}

fn unsafe_attachment_metadata_char(value: char) -> bool {
    value.is_control()
        || matches!(
            value,
            '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}'
        )
}

fn valid_id_byte(byte: u8) -> bool {
    byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_')
}

fn service_label(service_id: &str) -> &str {
    match service_id {
        "discord" => "Discord",
        "telegram" => "Telegram",
        "whatsapp" => "WhatsApp",
        "email" => "Email",
        "signal" => "Signal",
        other => other,
    }
}

fn utc_date_time(unix_ms: i64) -> (String, String) {
    let seconds = unix_ms.div_euclid(1_000);
    let days = seconds.div_euclid(86_400);
    let second_of_day = seconds.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    let hour = second_of_day / 3_600;
    let minute = (second_of_day % 3_600) / 60;
    (
        format!("{year:04}-{month:02}-{day:02}"),
        format!("{hour:02}:{minute:02}"),
    )
}

fn civil_from_days(days_since_unix_epoch: i64) -> (i64, u32, u32) {
    let z = days_since_unix_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    year += if month <= 2 { 1 } else { 0 };
    (year, month as u32, day as u32)
}

fn classify(text: &str) -> Vec<(PrivacyRiskCategory, u8, &'static str)> {
    let lower = text.to_ascii_lowercase();
    let mut findings = Vec::new();

    if contains_secret_assignment(&lower)
        || ["ghp_", "xoxb-", "sk_live_", "rk_live_", "akia"]
            .iter()
            .any(|prefix| lower.contains(prefix))
    {
        findings.push((
            PrivacyRiskCategory::Credential,
            94,
            "This looks like a password, API key, or access credential.",
        ));
    }
    if [
        "recovery phrase",
        "seed phrase",
        "backup phrase",
        "private key",
        "recovery code",
    ]
    .iter()
    .any(|term| lower.contains(term))
    {
        findings.push((
            PrivacyRiskCategory::RecoveryMaterial,
            92,
            "This may expose account or wallet recovery material.",
        ));
    }
    if digit_runs(text).iter().any(|digits| luhn_valid(digits)) {
        findings.push((
            PrivacyRiskCategory::PaymentCard,
            91,
            "This contains a number shaped like a payment card.",
        ));
    }
    if contains_ssn_shape(text)
        || [
            "passport number",
            "driver's license number",
            "drivers license number",
            "national id number",
        ]
        .iter()
        .any(|term| lower.contains(term))
    {
        findings.push((
            PrivacyRiskCategory::GovernmentIdentity,
            88,
            "This may contain a government identity number.",
        ));
    }
    if [
        "my address is",
        "home address is",
        "meet me at",
        "i live at",
    ]
    .iter()
    .any(|term| lower.contains(term))
        && text.chars().any(|character| character.is_ascii_digit())
    {
        findings.push((
            PrivacyRiskCategory::PreciseLocation,
            80,
            "This may reveal a precise home or meeting location.",
        ));
    }
    if contains_any_word(
        &lower,
        &["fuck", "fucking", "shit", "bitch", "asshole", "cunt"],
    ) {
        findings.push((
            PrivacyRiskCategory::Profanity,
            70,
            "This contains language you may prefer not to keep in message history.",
        ));
    }
    if contains_any_word(
        &lower,
        &["porn", "pornographic", "nude", "nudes", "sexting"],
    ) || ["sexually explicit", "explicit photo", "explicit video"]
        .iter()
        .any(|term| lower.contains(term))
    {
        findings.push((
            PrivacyRiskCategory::SexualContent,
            72,
            "This may contain sexual or explicit content worth reviewing in context.",
        ));
    }
    if [
        "my diagnosis",
        "diagnosed with",
        "medical record",
        "medical results",
        "therapy session",
        "health insurance number",
        "my prescription",
    ]
    .iter()
    .any(|term| lower.contains(term))
    {
        findings.push((
            PrivacyRiskCategory::SensitiveHealth,
            78,
            "This may contain private health information worth reviewing in context.",
        ));
    }
    if contains_any_word(
        &lower,
        &[
            "cocaine",
            "heroin",
            "meth",
            "methamphetamine",
            "fentanyl",
            "mdma",
        ],
    ) || ["buy weed", "sell weed", "smoke weed", "drug dealer"]
        .iter()
        .any(|term| lower.contains(term))
    {
        findings.push((
            PrivacyRiskCategory::ControlledSubstances,
            68,
            "This may discuss controlled substances or drug use; review the context yourself.",
        ));
    }
    if [
        "stolen card",
        "commit fraud",
        "launder money",
        "evade police",
        "break into the",
        "how to hack",
        "shoplift from",
    ]
    .iter()
    .any(|term| lower.contains(term))
    {
        findings.push((
            PrivacyRiskCategory::PotentiallyUnlawfulConduct,
            66,
            "This may discuss potentially unlawful conduct; it is a review signal, not a legal conclusion.",
        ));
    }
    if [
        "confidential",
        "internal only",
        "do not share",
        "trade secret",
        "unreleased product",
        "customer list",
        "customer data",
        "contract terms",
        "pricing contract",
        "confidential file",
        "company credentials",
        "internal api key",
        "access details",
        "internal link",
        "private link",
        "private internal link",
        "internal roadmap",
    ]
    .iter()
    .any(|term| lower.contains(term))
    {
        findings.push((
            PrivacyRiskCategory::WorkSensitiveInformation,
            74,
            "This may contain work-sensitive information; review is suggested, not a legal determination.",
        ));
    }
    findings
}

fn contains_any_word(lower: &str, words: &[&str]) -> bool {
    lower
        .split(|character: char| !character.is_ascii_alphanumeric() && character != '\'')
        .any(|token| words.contains(&token))
}

fn contains_secret_assignment(lower: &str) -> bool {
    [
        "password", "passwd", "api key", "api_key", "secret", "token",
    ]
    .iter()
    .any(|label| {
        lower.find(label).is_some_and(|index| {
            let tail = &lower[index + label.len()..];
            let tail = tail.trim_start();
            tail.starts_with(':') || tail.starts_with('=') || tail.starts_with(" is ")
        })
    })
}

fn digit_runs(text: &str) -> Vec<Vec<u8>> {
    let mut runs = Vec::new();
    let mut run = Vec::new();
    for byte in text.bytes() {
        if byte.is_ascii_digit() {
            run.push(byte - b'0');
        } else if matches!(byte, b' ' | b'-') && !run.is_empty() {
            continue;
        } else {
            if (13..=19).contains(&run.len()) {
                runs.push(std::mem::take(&mut run));
            }
            run.clear();
        }
    }
    if (13..=19).contains(&run.len()) {
        runs.push(run);
    }
    runs
}

fn luhn_valid(digits: &[u8]) -> bool {
    if !(13..=19).contains(&digits.len()) || digits.iter().all(|digit| *digit == digits[0]) {
        return false;
    }
    let parity = digits.len() % 2;
    let sum: u32 = digits
        .iter()
        .enumerate()
        .map(|(index, digit)| {
            let mut value = u32::from(*digit);
            if index % 2 == parity {
                value *= 2;
                if value > 9 {
                    value -= 9;
                }
            }
            value
        })
        .sum();
    sum.is_multiple_of(10)
}

fn contains_ssn_shape(text: &str) -> bool {
    text.as_bytes().windows(11).any(|window| {
        window[0..3].iter().all(u8::is_ascii_digit)
            && window[3] == b'-'
            && window[4..6].iter().all(u8::is_ascii_digit)
            && window[6] == b'-'
            && window[7..11].iter().all(u8::is_ascii_digit)
            && window[0..3] != *b"000"
            && window[4..6] != *b"00"
            && window[7..11] != *b"0000"
    })
}

fn preview(text: &str) -> String {
    let mut output: String = text.chars().take(MAX_PREVIEW_CHARS).collect();
    if text.chars().count() > MAX_PREVIEW_CHARS {
        output.push('…');
    }
    output
}

fn result_message_locator(locator: &str) -> String {
    if contains_jump_reference(locator) {
        BLOCKED_JUMP_REFERENCE.to_owned()
    } else {
        locator.to_owned()
    }
}

fn result_attachment_path(path: &str) -> String {
    if contains_jump_reference(path) {
        BLOCKED_JUMP_REFERENCE.to_owned()
    } else {
        path.to_owned()
    }
}

fn result_preview(text: &str) -> String {
    preview(&redact_jump_reference_tokens(text))
}

fn redact_jump_reference_tokens(text: &str) -> String {
    let mut output = String::with_capacity(text.len().min(MAX_PREVIEW_CHARS));
    for (index, token) in text.split_whitespace().enumerate() {
        if index > 0 {
            output.push(' ');
        }
        if contains_jump_reference(token) {
            output.push_str("[blocked-reference]");
        } else {
            output.push_str(token);
        }
    }
    output
}

fn contains_jump_reference(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    contains_url_or_deep_link(&lower) || contains_open_action_reference(&lower)
}

fn contains_url_or_deep_link(lower: &str) -> bool {
    lower.contains("://")
        || lower.starts_with("mailto:")
        || lower.starts_with("tel:")
        || lower.starts_with("www.")
        || lower.contains(".com/")
        || lower.contains(".net/")
        || lower.contains(".org/")
        || lower.contains("deep-link")
        || lower.contains("deeplink")
}

fn contains_open_action_reference(lower: &str) -> bool {
    lower.contains("open_service")
        || lower.contains("open-service")
        || lower.contains("openservice")
        || lower.contains("open_action")
        || lower.contains("open-action")
        || lower.contains("openaction")
        || lower.contains("open action")
        || lower.contains("openserviceroute")
        || lower.contains("open_service_host")
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine as _;

    fn message(text: &str) -> LocalMessageCandidate {
        LocalMessageCandidate {
            service_id: "telegram".to_owned(),
            account_id: "qa-account".to_owned(),
            conversation_id: "conversation-1".to_owned(),
            message_locator: "opaque-message-1".to_owned(),
            authored_by_self: true,
            created_at_unix_ms: Some(1_700_000_000_000),
            text: text.to_owned(),
            reply_recipient: None,
            visible_recipients: Vec::new(),
            hidden_recipients: Vec::new(),
            email_thread_identity: None,
            email_folder_identity: None,
            attachments: Vec::new(),
        }
    }

    fn fixture_message(
        service_id: &str,
        account_id: &str,
        conversation_id: &str,
        created_at_unix_ms: i64,
        text: &str,
    ) -> LocalMessageCandidate {
        LocalMessageCandidate {
            service_id: service_id.to_owned(),
            account_id: account_id.to_owned(),
            conversation_id: conversation_id.to_owned(),
            message_locator: format!("{conversation_id}:message"),
            authored_by_self: true,
            created_at_unix_ms: Some(created_at_unix_ms),
            text: text.to_owned(),
            attachments: Vec::new(),
        }
    }

    #[test]
    fn flags_high_confidence_local_risks_without_persisting() {
        let result = scan_local_messages(vec![
            message("password: correct horse battery staple"),
            message("card 4242 4242 4242 4242"),
            message("my address is 123 Main Street"),
        ]);
        assert_eq!(result.messages_scanned, 3);
        assert_eq!(result.findings.len(), 3);
        assert_eq!(result.analysis_location, "this_device_only");
        assert!(!result.persisted);
    }

    #[test]
    fn task_1444_direct_results_output_groups_two_fixture_matches_with_all_location_fields() {
        let output = group_saved_matches_by_account(vec![
            fixture_message(
                "discord",
                "discord-account-alpha-1444",
                "dm:task-1444-alpha",
                1_786_008_600_000,
                "password: correct horse battery staple",
            ),
            fixture_message(
                "telegram",
                "telegram-account-beta-1444",
                "chat:task-1444-beta",
                1_786_013_100_000,
                "recovery phrase: maple bridge cloud midnight",
            ),
            fixture_message(
                "discord",
                "discord-account-alpha-1444",
                "dm:task-1444-alpha",
                1_786_008_600_000,
                "Want to get coffee tomorrow?",
            ),
        ]);
        println!(
            "TASK1444_DIRECT_RESULT command=group_saved_matches_by_account group_count={} total_matches={}",
            output.groups.len(),
            output.total_matches
        );
        for group in &output.groups {
            println!(
                "TASK1444_GROUP account_id={} match_count={}",
                group.account_id,
                group.matches.len()
            );
            for item in &group.matches {
                println!(
                    "TASK1444_MATCH account_id={} full_text=\"{}\" reason=\"{}\" service=\"{}\" place=\"{}\" date={} time={}",
                    group.account_id,
                    item.full_text,
                    item.reason,
                    item.service,
                    item.place,
                    item.date,
                    item.time
                );
            }
        }

        assert_eq!(output.groups.len(), 2);
        assert_eq!(output.total_matches, 2);
        assert_eq!(output.groups[0].account_id, "discord-account-alpha-1444");
        assert_eq!(output.groups[0].matches.len(), 1);
        assert_eq!(
            output.groups[0].matches[0],
            SavedReviewMatch {
                full_text: "password: correct horse battery staple".to_owned(),
                reason: "This looks like a password, API key, or access credential.",
                service: "Discord".to_owned(),
                place: "dm:task-1444-alpha".to_owned(),
                date: "2026-08-06".to_owned(),
                time: "09:30".to_owned(),
            }
        );
        assert_eq!(output.groups[1].account_id, "telegram-account-beta-1444");
        assert_eq!(output.groups[1].matches.len(), 1);
        assert_eq!(
            output.groups[1].matches[0],
            SavedReviewMatch {
                full_text: "recovery phrase: maple bridge cloud midnight".to_owned(),
                reason: "This may expose account or wallet recovery material.",
                service: "Telegram".to_owned(),
                place: "chat:task-1444-beta".to_owned(),
                date: "2026-08-06".to_owned(),
                time: "10:45".to_owned(),
            }
        );
    }

    #[test]
    fn task_3000_direct_command_names_found_message_record_shape_and_refuses_missing_sender() {
        let samples = vec![
            ServiceFoundMessageInput {
                service_id: "discord".to_owned(),
                sender: Some("Ari Discord".to_owned()),
                signed_in_account_sender: Some("Tessa Telegram".to_owned()),
                sent_at_unix_ms: 1_786_008_600_000,
                place: "discord:dm:task-3000-alpha:message-1".to_owned(),
                text: "Discord sample found message 3000".to_owned(),
            },
            ServiceFoundMessageInput {
                service_id: "telegram".to_owned(),
                sender: Some("Tessa Telegram".to_owned()),
                signed_in_account_sender: Some("Tessa Telegram".to_owned()),
                sent_at_unix_ms: 1_786_012_200_000,
                place: "telegram:chat:task-3000-beta:message-2".to_owned(),
                text: "Telegram sample found message 3000".to_owned(),
            },
            ServiceFoundMessageInput {
                service_id: "signal".to_owned(),
                sender: Some("Sam Signal".to_owned()),
                signed_in_account_sender: Some("Tessa Telegram".to_owned()),
                sent_at_unix_ms: 1_786_015_800_000,
                place: "signal:thread:task-3000-gamma:message-3".to_owned(),
                text: "Signal sample found message 3000".to_owned(),
            },
        ];
        let services: Vec<String> = samples
            .iter()
            .map(|sample| sample.service_id.clone())
            .collect();
        let records =
            name_found_message_records(samples.clone()).expect("sample records are valid");
        println!(
            "TASK3000_DIRECT_RESULT command=name_found_message_records service_count={} record_count={}",
            services.len(),
            records.len()
        );
        for (service, record) in services.iter().zip(&records) {
            let all_five_fields_set = !record.sender.is_empty()
                && record.sent_at_unix_ms > 0
                && !record.place.is_empty()
                && !record.text.is_empty();
            println!(
                "TASK3000_RECORD service={} sender=\"{}\" sent_at_unix_ms={} place=\"{}\" text=\"{}\" sent_by_signed_in_account={} all_five_fields_set={}",
                service,
                record.sender,
                record.sent_at_unix_ms,
                record.place,
                record.text,
                record.sent_by_signed_in_account,
                all_five_fields_set
            );
        }

        assert_eq!(records.len(), 3);
        assert_eq!(
            records,
            vec![
                FoundMessageRecord {
                    sender: "Ari Discord".to_owned(),
                    sent_at_unix_ms: 1_786_008_600_000,
                    place: "discord:dm:task-3000-alpha:message-1".to_owned(),
                    text: "Discord sample found message 3000".to_owned(),
                    sent_by_signed_in_account: false,
                },
                FoundMessageRecord {
                    sender: "Tessa Telegram".to_owned(),
                    sent_at_unix_ms: 1_786_012_200_000,
                    place: "telegram:chat:task-3000-beta:message-2".to_owned(),
                    text: "Telegram sample found message 3000".to_owned(),
                    sent_by_signed_in_account: true,
                },
                FoundMessageRecord {
                    sender: "Sam Signal".to_owned(),
                    sent_at_unix_ms: 1_786_015_800_000,
                    place: "signal:thread:task-3000-gamma:message-3".to_owned(),
                    text: "Signal sample found message 3000".to_owned(),
                    sent_by_signed_in_account: false,
                },
            ]
        );

        let refused = name_found_message_record(ServiceFoundMessageInput {
            service_id: "discord".to_owned(),
            sender: None,
            signed_in_account_sender: Some("Ari Discord".to_owned()),
            sent_at_unix_ms: 1_786_008_600_000,
            place: "discord:dm:task-3000-alpha:message-missing-sender".to_owned(),
            text: "Missing sender must be refused".to_owned(),
        })
        .expect_err("missing sender must be refused");
        println!(
            "TASK3000_REFUSAL command=name_found_message_record missing_sender_refused={} error=\"{}\"",
            refused == FoundMessageRecordError::MissingSender,
            refused
        );
        assert_eq!(refused, FoundMessageRecordError::MissingSender);
    }

    #[test]
    fn task_3001_direct_command_answers_owner_check_and_refuses_unknown_sender() {
        let signed_in = "task-3001-signed-in-account";
        let yes = did_signed_in_account_send_message(MessageOwnerCheckInput {
            signed_in_account_sender: Some(signed_in.to_owned()),
            message_sender: Some(signed_in.to_owned()),
        })
        .expect("known matching sender checks");
        println!(
            "TASK3001_OWNER_CHECK command=did_signed_in_account_send_message message=sent-by-account signed_in_sender=\"{}\" message_sender=\"{}\" result={}",
            signed_in,
            signed_in,
            if yes { "yes" } else { "no" }
        );

        let other_sender = "task-3001-other-account";
        let no = did_signed_in_account_send_message(MessageOwnerCheckInput {
            signed_in_account_sender: Some(signed_in.to_owned()),
            message_sender: Some(other_sender.to_owned()),
        })
        .expect("known non-matching sender checks");
        println!(
            "TASK3001_OWNER_CHECK command=did_signed_in_account_send_message message=sent-by-someone-else signed_in_sender=\"{}\" message_sender=\"{}\" result={}",
            signed_in,
            other_sender,
            if no { "yes" } else { "no" }
        );

        let refused = did_signed_in_account_send_message(MessageOwnerCheckInput {
            signed_in_account_sender: Some(signed_in.to_owned()),
            message_sender: None,
        })
        .expect_err("unknown sender must be refused");
        println!(
            "TASK3001_OWNER_CHECK_REFUSAL command=did_signed_in_account_send_message message=unknown-sender result=ERR error=\"{}\"",
            refused
        );

        assert!(yes);
        assert!(!no);
        assert_eq!(refused, MessageOwnerCheckError::UnknownMessageSender);
    }

    #[test]
    fn ordinary_conversation_is_not_flagged() {
        let result = scan_local_messages(vec![message("Want to get coffee tomorrow?")]);
        assert!(result.findings.is_empty());
    }

    #[test]
    fn invalid_and_oversized_messages_fail_closed() {
        let mut invalid = message("password: example");
        invalid.service_id = "Instagram!".to_owned();
        let mut oversized = message("secret: example");
        oversized.text = "x".repeat(MAX_TEXT_BYTES + 1);
        let result = scan_local_messages(vec![invalid, oversized]);
        assert_eq!(result.messages_scanned, 0);
        assert_eq!(result.messages_rejected, 2);
        assert!(result.findings.is_empty());
    }

    #[test]
    fn delete_is_not_offered_for_other_peoples_messages() {
        let mut candidate = message("recovery phrase: never share this");
        candidate.authored_by_self = false;
        let result = scan_local_messages(vec![candidate]);
        assert_eq!(result.findings.len(), 1);
        assert!(!result.findings[0].can_request_delete);
    }

    #[test]
    fn result_json_blocks_original_message_jump_links() {
        let mut candidate = message(
            "password: example https://discord.com/channels/@me/123/456?openAction=open_service_host",
        );
        candidate.message_locator =
            "https://discord.com/channels/@me/123/456?deepLink=osl://open&openAction=open_service_host"
                .to_owned();

        let result = scan_local_messages(vec![candidate]);
        let json = serde_json::to_string(&result).expect("serialize privacy scan result");

        assert_eq!(result.findings.len(), 1);
        assert_eq!(result.findings[0].message_locator, BLOCKED_JUMP_REFERENCE);
        assert!(!json.contains("https://"));
        assert!(!json.contains("discord.com/channels"));
        assert!(!json.contains("deepLink"));
        assert!(!json.contains("openAction"));
        assert!(!json.contains("open_service_host"));
    }

    #[test]
    fn payment_card_rule_rejects_non_luhn_numbers() {
        let result = scan_local_messages(vec![message("reference 1234 5678 9012 3456")]);
        assert!(result.findings.is_empty());
    }

    #[test]
    fn emits_bounded_context_review_signals_without_making_verdicts() {
        let result = scan_local_messages(vec![
            message("this is fucking frustrating"),
            message("that explicit photo should not be in chat"),
            message("my diagnosis is in the medical record"),
            message("we discussed cocaine use"),
            message("the lesson quotes how to hack an account"),
            message("internal only: unreleased product roadmap and customer data"),
        ]);
        assert_eq!(result.findings.len(), 6);
        assert!(result
            .findings
            .iter()
            .all(|finding| finding.confidence < 80));
        assert!(result.findings.iter().all(|finding| {
            let reason = finding.reason.to_ascii_lowercase();
            !reason.contains("is illegal") && !reason.contains("is guilty")
        }));
    }

    #[test]
    fn profanity_matching_uses_word_boundaries() {
        let result = scan_local_messages(vec![message("Scunthorpe and shitake mushrooms")]);
        assert!(result.findings.is_empty());
    }

    #[test]
    fn attachment_text_runs_through_existing_detectors_with_provenance() {
        let mut candidate = message("");
        candidate.attachments.push(LocalAttachmentCandidate {
            attachment_id: "invoice-1".into(),
            display_name: "photo.jpg".into(),
            content_base64: base64::engine::general_purpose::STANDARD
                .encode(b"password: attachment-secret"),
        });
        let result = scan_local_messages(vec![candidate]);
        assert_eq!(result.messages_scanned, 1);
        assert_eq!(result.attachments_scanned, 1);
        assert_eq!(result.attachment_types_scanned, ["plain_text"]);
        assert_eq!(result.findings.len(), 1);
        assert_eq!(
            result.findings[0].attachment_path.as_deref(),
            Some("photo.jpg")
        );
    }

    fn mib(value: u64) -> u64 {
        value * MAIL_ATTACHMENT_MIB
    }

    fn ordinary_set(prefix: &str, first_mb: u64, second_mb: u64) -> Vec<OrdinaryAttachmentSetItem> {
        vec![
            OrdinaryAttachmentSetItem {
                display_name: format!("{prefix}-a.bin"),
                size_bytes: mib(first_mb),
            },
            OrdinaryAttachmentSetItem {
                display_name: format!("{prefix}-b.bin"),
                size_bytes: mib(second_mb),
            },
        ]
    }

    fn set_limit_verdict(
        label: &str,
        profile: OrdinaryAttachmentLimitProfile,
        accepted_mb: u64,
        refused_mb: u64,
    ) -> String {
        let accepted = check_ordinary_attachment_set_limit(
            profile,
            &ordinary_set(
                &format!("task3761-{label}-under"),
                accepted_mb / 2,
                accepted_mb - (accepted_mb / 2),
            ),
        );
        let refused = check_ordinary_attachment_set_limit(
            profile,
            &ordinary_set(
                &format!("task3761-{label}-over"),
                refused_mb / 2,
                refused_mb - (refused_mb / 2),
            ),
        );
        let accepted_status = if accepted.accepted {
            "accepted"
        } else {
            "refused"
        };
        let refused_status = if refused.accepted {
            "accepted"
        } else {
            "refused"
        };
        format!(
            "TASK3761 {label} accepted_set_status={accepted_status} accepted_set_mb={} refused_set_status={refused_status} refused_set_mb={} refused_by_name={} limit_mb={} refusal=\"{}\"",
            accepted.total_mb,
            refused.total_mb,
            refused.rejected_attachment_name.as_deref().unwrap_or(""),
            refused.limit_mb,
            refused.refusal.as_deref().unwrap_or("")
        )
    }

    #[test]
    fn task3761_mailcom_and_exchange_attachment_limits_refuse_sets_by_name() {
        let lines = vec![
            set_limit_verdict(
                "mailcom_free",
                OrdinaryAttachmentLimitProfile::MailcomFree,
                29,
                31,
            ),
            set_limit_verdict(
                "mailcom_premium",
                OrdinaryAttachmentLimitProfile::MailcomPremium,
                99,
                101,
            ),
            set_limit_verdict(
                "exchange_default",
                OrdinaryAttachmentLimitProfile::Exchange {
                    company_limit_mb: None,
                },
                9,
                11,
            ),
            set_limit_verdict(
                "exchange_company_50",
                OrdinaryAttachmentLimitProfile::Exchange {
                    company_limit_mb: Some(50),
                },
                49,
                51,
            ),
        ];

        for line in &lines {
            println!("{line}");
        }

        assert_eq!(
            lines,
            vec![
                "TASK3761 mailcom_free accepted_set_status=accepted accepted_set_mb=29 refused_set_status=refused refused_set_mb=31 refused_by_name=task3761-mailcom_free-over-b.bin limit_mb=30 refusal=\"Mail.com Free refuses ordinary attachments over 30 MB: task3761-mailcom_free-over-b.bin makes the ordinary attachment set 31 MB\"",
                "TASK3761 mailcom_premium accepted_set_status=accepted accepted_set_mb=99 refused_set_status=refused refused_set_mb=101 refused_by_name=task3761-mailcom_premium-over-b.bin limit_mb=100 refusal=\"Mail.com Premium refuses ordinary attachments over 100 MB: task3761-mailcom_premium-over-b.bin makes the ordinary attachment set 101 MB\"",
                "TASK3761 exchange_default accepted_set_status=accepted accepted_set_mb=9 refused_set_status=refused refused_set_mb=11 refused_by_name=task3761-exchange_default-over-b.bin limit_mb=10 refusal=\"Exchange default refuses ordinary attachments over 10 MB: task3761-exchange_default-over-b.bin makes the ordinary attachment set 11 MB\"",
                "TASK3761 exchange_company_50 accepted_set_status=accepted accepted_set_mb=49 refused_set_status=refused refused_set_mb=51 refused_by_name=task3761-exchange_company_50-over-b.bin limit_mb=50 refusal=\"Exchange company 50 MB refuses ordinary attachments over 50 MB: task3761-exchange_company_50-over-b.bin makes the ordinary attachment set 51 MB\"",
            ]
        );
    }

    #[test]
    fn attachment_input_rejects_more_than_sixteen_items_before_scanning() {
        let mut candidate = message("");
        candidate.attachments = (0..=MAX_ATTACHMENTS_PER_MESSAGE)
            .map(|index| LocalAttachmentCandidate {
                attachment_id: format!("attachment-{index}"),
                display_name: format!("attachment-{index}.txt"),
                content_base64: "YQ==".into(),
            })
            .collect();

        assert!(validate_attachment_input_batch(std::slice::from_ref(&candidate)).is_err());
        let result = scan_local_messages(vec![candidate]);
        assert_eq!(result.messages_scanned, 0);
        assert_eq!(result.messages_rejected, 1);
        assert_eq!(result.attachments_scanned, 0);
    }

    #[test]
    fn attachment_input_bounds_metadata_and_base64_envelopes() {
        let mut candidate = message("");
        candidate.attachments.push(LocalAttachmentCandidate {
            attachment_id: "attachment\nspoof".into(),
            display_name: "document.txt".into(),
            content_base64: "not base64!".into(),
        });
        assert!(validate_attachment_input_batch(&[candidate]).is_err());

        let mut oversized = message("");
        oversized.attachments.push(LocalAttachmentCandidate {
            attachment_id: "a".repeat(MAX_ATTACHMENT_ID_BYTES + 1),
            display_name: "document.txt".into(),
            content_base64: "YQ==".into(),
        });
        assert!(validate_attachment_input_batch(&[oversized]).is_err());
    }

    #[test]
    fn attachment_input_rejects_aggregate_encoded_bytes_before_scan_or_clone() {
        let encoded = "A".repeat(MAX_ATTACHMENT_BATCH_ENCODED_BYTES / 2 + 4);
        let mut first = message("");
        first.attachments.push(LocalAttachmentCandidate {
            attachment_id: "first".into(),
            display_name: "first.bin".into(),
            content_base64: encoded.clone(),
        });
        let mut second = message("");
        second.attachments.push(LocalAttachmentCandidate {
            attachment_id: "second".into(),
            display_name: "second.bin".into(),
            content_base64: encoded,
        });

        assert!(validate_attachment_input_batch(&[first, second]).is_err());
    }

    #[test]
    fn unavailable_image_model_is_explicitly_uninspected() {
        let mut candidate = message("");
        candidate.attachments.push(LocalAttachmentCandidate {
            attachment_id: "image-1".into(),
            display_name: "actually-an-image.bin".into(),
            content_base64: base64::engine::general_purpose::STANDARD
                .encode(b"\x89PNG\r\n\x1a\nminimal"),
        });
        let result = scan_local_messages(vec![candidate]);
        assert!(!result.images_checked);
        assert_eq!(result.attachments_scanned, 0);
        assert_eq!(result.uninspected_attachments.len(), 1);
    }

    #[test]
    fn task1220_email_protection_check_counts_visible_and_hidden_recipients_but_hides_bcc() {
        let bcc_recipient = "bcc-task1220@oslprivacy.com";
        let mut candidate = message("password: email draft secret");
        candidate.service_id = "email".to_owned();
        candidate.visible_recipients = vec![
            "to-task1220@oslprivacy.com".to_owned(),
            "cc-task1220@oslprivacy.com".to_owned(),
        ];
        candidate.hidden_recipients = vec![bcc_recipient.to_owned()];
        let visible_sent = candidate.visible_recipients.len();
        let hidden_sent = candidate.hidden_recipients.len();

        let result = scan_local_messages(vec![candidate]);
        let check = result
            .email_protection_checks
            .first()
            .expect("email protection check is emitted for the email draft");
        let display_output = format!("visibleRecipients={}", check.visible_recipients.join(","));

        println!(
            "TASK1220 email_protection_check visible_sent={} hidden_sent={} distinct_recipients={} display_output=\"{}\" bcc_hidden_in_display={}",
            visible_sent,
            hidden_sent,
            check.distinct_recipient_count,
            display_output,
            !display_output.contains(bcc_recipient),
        );

        assert_eq!(check.visible_recipients.len(), visible_sent);
        assert_eq!(hidden_sent, 1);
        assert_eq!(check.distinct_recipient_count, 3);
        assert!(!display_output.contains(bcc_recipient));
        assert_eq!(result.findings.len(), 1);
    }

    #[test]
    fn task1291_direct_command_returns_reply_and_reply_all_without_bcc() {
        let reply_recipient = "from-task1291@oslprivacy.com";
        let to_recipient = "to-task1291@oslprivacy.com";
        let cc_recipient = "cc-task1291@oslprivacy.com";
        let bcc_recipient = "bcc-task1291@oslprivacy.com";
        let mut candidate = message("password: protected reply draft");
        candidate.service_id = "email".to_owned();
        candidate.reply_recipient = Some(reply_recipient.to_owned());
        candidate.visible_recipients = vec![to_recipient.to_owned(), cc_recipient.to_owned()];
        candidate.hidden_recipients = vec![bcc_recipient.to_owned()];

        let result = scan_local_messages(vec![candidate]);
        let check = result
            .email_protection_checks
            .first()
            .expect("email protection check returns protected reply recipients");
        let reply_output = check.reply_recipients.join(",");
        let reply_all_output = check.reply_all_recipients.join(",");
        let reply_all_excludes_bcc = !check
            .reply_all_recipients
            .iter()
            .any(|recipient| recipient == bcc_recipient);

        println!(
            "TASK1291 protected_email_replies direct_command=scan_local_messages Reply count={} recipients={} ReplyAll count={} recipients={} bcc_excluded={}",
            check.reply_recipients.len(),
            reply_output,
            check.reply_all_recipients.len(),
            reply_all_output,
            reply_all_excludes_bcc,
        );

        assert_eq!(check.reply_recipients, vec![reply_recipient.to_owned()]);
        assert_eq!(
            check.reply_all_recipients,
            vec![to_recipient.to_owned(), cc_recipient.to_owned()]
        );
        assert!(reply_all_excludes_bcc);
        assert!(!reply_output.contains(bcc_recipient));
    }

    #[test]
    fn task1292_fixture_commands_create_reply_and_reply_all_drafts_with_expected_recipients() {
        let reply_recipient = "from-task1292@oslprivacy.com";
        let to_recipient = "to-task1292@oslprivacy.com";
        let cc_recipient = "cc-task1292@oslprivacy.com";
        let bcc_recipient = "bcc-task1292@oslprivacy.com";
        let mut candidate = message("password: protected reply draft");
        candidate.service_id = "email".to_owned();
        candidate.message_locator = "task1292-message".to_owned();
        candidate.reply_recipient = Some(reply_recipient.to_owned());
        candidate.visible_recipients = vec![to_recipient.to_owned(), cc_recipient.to_owned()];
        candidate.hidden_recipients = vec![bcc_recipient.to_owned()];

        let result = scan_local_messages(vec![candidate]);
        let check = result
            .email_protection_checks
            .first()
            .expect("email protection check returns protected reply recipients");
        let reply_draft =
            create_protected_email_reply_draft(check, ProtectedEmailReplyAction::Reply)
                .expect("reply draft is created from the protected recipient list");
        let reply_all_draft =
            create_protected_email_reply_draft(check, ProtectedEmailReplyAction::ReplyAll)
                .expect("reply-all draft is created from the protected recipient list");

        println!(
            "TASK1292 protected_email_reply_drafts fixture_command=create_protected_email_reply_draft reply_drafts=1 reply_recipients_count={} reply_recipients={} reply_all_drafts=1 reply_all_recipients_count={} reply_all_recipients={} bcc_in_reply_all={}",
            reply_draft.recipients.len(),
            reply_draft.recipients.join(","),
            reply_all_draft.recipients.len(),
            reply_all_draft.recipients.join(","),
            reply_all_draft
                .recipients
                .iter()
                .any(|recipient| recipient == bcc_recipient),
        );

        assert!(reply_draft.protected);
        assert_eq!(reply_draft.action, ProtectedEmailReplyAction::Reply);
        assert_eq!(reply_draft.recipients, vec![reply_recipient.to_owned()]);
        assert!(reply_all_draft.protected);
        assert_eq!(reply_all_draft.action, ProtectedEmailReplyAction::ReplyAll);
        assert_eq!(
            reply_all_draft.recipients,
            vec![to_recipient.to_owned(), cc_recipient.to_owned()]
        );
        assert!(!reply_all_draft
            .recipients
            .iter()
            .any(|recipient| recipient == bcc_recipient));
    }

    #[test]
    fn task3760_gmail_counts_only_ordinary_attachments_against_twenty_five_mb() {
        const MB: u64 = 1024 * 1024;

        fn draft_attachment(
            display_name: &str,
            size_mb: u64,
            storage: EmailDraftAttachmentStorage,
        ) -> EmailDraftAttachment {
            EmailDraftAttachment {
                display_name: display_name.to_owned(),
                size_bytes: size_mb * MB,
                storage,
            }
        }

        let mut candidate = message("password: protected Gmail attachment draft");
        candidate.service_id = "email".to_owned();
        candidate.message_locator = "task3760-gmail-draft".to_owned();
        candidate.reply_recipient = Some("from-task3760@oslprivacy.com".to_owned());
        candidate.visible_recipients = vec!["to-task3760@oslprivacy.com".to_owned()];

        let result = scan_local_messages(vec![candidate]);
        let check = result
            .email_protection_checks
            .first()
            .expect("email protection check returns Gmail draft recipients");

        let ordinary_24 = [
            draft_attachment(
                "task3760-ordinary-12a.bin",
                12,
                EmailDraftAttachmentStorage::Ordinary,
            ),
            draft_attachment(
                "task3760-ordinary-12b.bin",
                12,
                EmailDraftAttachmentStorage::Ordinary,
            ),
        ];
        let accepted_ordinary_24 = create_protected_email_reply_draft_with_attachments(
            check,
            ProtectedEmailReplyAction::Reply,
            &ordinary_24,
        )
        .expect("24 MB of ordinary Gmail attachments is accepted");

        let ordinary_26 = [
            draft_attachment(
                "task3760-ordinary-13a.bin",
                13,
                EmailDraftAttachmentStorage::Ordinary,
            ),
            draft_attachment(
                "task3760-ordinary-13b.bin",
                13,
                EmailDraftAttachmentStorage::Ordinary,
            ),
        ];
        let refused_ordinary_26 = create_protected_email_reply_draft_with_attachments(
            check,
            ProtectedEmailReplyAction::Reply,
            &ordinary_26,
        )
        .expect_err("26 MB of ordinary Gmail attachments is refused by name");

        let osl_stored_26 = [
            draft_attachment(
                "task3760-osl-stored-13a.bin",
                13,
                EmailDraftAttachmentStorage::OslStored,
            ),
            draft_attachment(
                "task3760-osl-stored-13b.bin",
                13,
                EmailDraftAttachmentStorage::OslStored,
            ),
        ];
        let accepted_osl_stored_26 = create_protected_email_reply_draft_with_attachments(
            check,
            ProtectedEmailReplyAction::Reply,
            &osl_stored_26,
        )
        .expect("26 MB of OSL-stored Gmail files does not count as ordinary attachments");

        println!(
            "TASK3760 gmail_attachment_limit ordinary_24_status=accepted ordinary_24_mb={} ordinary_26_status=refused ordinary_26_mb=26 refusal=\"{}\" gmail_limit_mb={} osl_stored_26_status=accepted osl_stored_26_mb={} osl_stored_ordinary_counted_mb={}",
            bytes_to_whole_mb(accepted_ordinary_24.ordinary_attachment_bytes),
            refused_ordinary_26,
            GMAIL_ORDINARY_ATTACHMENT_LIMIT_MB,
            bytes_to_whole_mb(accepted_osl_stored_26.osl_stored_file_bytes),
            bytes_to_whole_mb(accepted_osl_stored_26.ordinary_attachment_bytes),
        );

        assert_eq!(accepted_ordinary_24.ordinary_attachment_bytes, 24 * MB);
        assert_eq!(accepted_ordinary_24.osl_stored_file_bytes, 0);
        assert!(refused_ordinary_26.contains("task3760-ordinary-13b.bin"));
        assert!(refused_ordinary_26.contains("25 MB"));
        assert!(refused_ordinary_26.contains("26 MB"));
        assert_eq!(accepted_osl_stored_26.osl_stored_file_bytes, 26 * MB);
        assert_eq!(accepted_osl_stored_26.ordinary_attachment_bytes, 0);
    }

    #[test]
    fn task3762_two_hundred_mb_osl_stored_files_ignore_mail_attachment_limits() {
        const MB: u64 = 1024 * 1024;
        const FILE_MB: u64 = 200;

        fn draft_attachment(
            display_name: &str,
            storage: EmailDraftAttachmentStorage,
        ) -> EmailDraftAttachment {
            EmailDraftAttachment {
                display_name: display_name.to_owned(),
                size_bytes: FILE_MB * MB,
                storage,
            }
        }

        let mut candidate = message("password: protected 200 MB mail limit draft");
        candidate.service_id = "email".to_owned();
        candidate.message_locator = "task3762-mail-limit-draft".to_owned();
        candidate.reply_recipient = Some("from-task3762@oslprivacy.com".to_owned());
        candidate.visible_recipients = vec!["to-task3762@oslprivacy.com".to_owned()];

        let result = scan_local_messages(vec![candidate]);
        let check = result
            .email_protection_checks
            .first()
            .expect("email protection check returns mail-limit draft recipients");

        let profiles = [
            (
                "gmail",
                EmailDraftMailLimitProfile::Gmail,
                GMAIL_ORDINARY_ATTACHMENT_LIMIT_MB,
            ),
            (
                "maildotcom-free",
                EmailDraftMailLimitProfile::MailDotComFree,
                MAIL_DOT_COM_FREE_ORDINARY_ATTACHMENT_LIMIT_MB,
            ),
            (
                "maildotcom-premium",
                EmailDraftMailLimitProfile::MailDotComPremium,
                MAIL_DOT_COM_PREMIUM_ORDINARY_ATTACHMENT_LIMIT_MB,
            ),
            (
                "exchange",
                EmailDraftMailLimitProfile::Exchange,
                EXCHANGE_ORDINARY_ATTACHMENT_LIMIT_MB,
            ),
        ];

        let mut accepted_profiles = Vec::new();
        let mut refused_by_name = Vec::new();
        let mut limit_numbers = Vec::new();

        for (slug, profile, expected_limit_mb) in profiles {
            let osl_stored_name = format!("task3762-{slug}-osl-stored-200mb.bin");
            let ordinary_name = format!("task3762-{slug}-ordinary-200mb.bin");
            let osl_stored = [draft_attachment(
                &osl_stored_name,
                EmailDraftAttachmentStorage::OslStored,
            )];
            let ordinary = [draft_attachment(
                &ordinary_name,
                EmailDraftAttachmentStorage::Ordinary,
            )];

            let accepted_osl_stored =
                create_protected_email_reply_draft_with_attachments_for_profile(
                    check,
                    ProtectedEmailReplyAction::Reply,
                    &osl_stored,
                    profile,
                )
                .expect("200 MB OSL-stored file is accepted regardless of mail limit");
            let refused_ordinary = create_protected_email_reply_draft_with_attachments_for_profile(
                check,
                ProtectedEmailReplyAction::Reply,
                &ordinary,
                profile,
            )
            .expect_err("200 MB ordinary attachment is refused by provider limit");

            assert_eq!(profile.ordinary_attachment_limit_mb(), expected_limit_mb);
            assert_eq!(accepted_osl_stored.osl_stored_file_bytes, FILE_MB * MB);
            assert_eq!(accepted_osl_stored.ordinary_attachment_bytes, 0);
            assert!(refused_ordinary.contains(profile.display_name()));
            assert!(refused_ordinary.contains(&ordinary_name));
            assert!(refused_ordinary.contains(&format!("{expected_limit_mb} MB")));
            assert!(refused_ordinary.contains("200 MB"));

            accepted_profiles.push(profile.display_name());
            refused_by_name.push(format!("{}:{ordinary_name}", profile.display_name()));
            limit_numbers.push(format!("{}={expected_limit_mb}", profile.display_name()));
        }

        println!(
            "TASK3762 mail_size_limits osl_stored_file_mb={FILE_MB} ordinary_attachment_mb={FILE_MB} osl_stored_acceptances={} ordinary_refusals_by_name={} accepted_profiles=\"{}\" refused_by_name=\"{}\" limits_mb=\"{}\"",
            accepted_profiles.len(),
            refused_by_name.len(),
            accepted_profiles.join("|"),
            refused_by_name.join("|"),
            limit_numbers.join("|"),
        );

        assert_eq!(
            accepted_profiles,
            vec!["Gmail", "Mail.com free", "Mail.com premium", "Exchange"]
        );
        assert_eq!(accepted_profiles.len(), 4);
        assert_eq!(refused_by_name.len(), 4);
    }

    #[test]
    fn task1226_direct_command_produces_separate_thread_and_folder_burn_target_lists() {
        let thread_identity = "email-thread-1225-stable";
        let folder_identity = "email-folder-1225-inbox";
        let mut open_message = message("password: task1226 burn scope seed");
        open_message.service_id = "email".to_owned();
        open_message.message_locator = "task1226-open-message".to_owned();
        open_message.email_thread_identity = Some(thread_identity.to_owned());
        open_message.email_folder_identity = Some(folder_identity.to_owned());

        let mut same_thread_message = message("secret: same thread, archived");
        same_thread_message.service_id = "email".to_owned();
        same_thread_message.message_locator = "task1226-same-thread-message".to_owned();
        same_thread_message.email_thread_identity = Some(thread_identity.to_owned());

        let mut same_folder_message = message("token: same folder, different thread");
        same_folder_message.service_id = "email".to_owned();
        same_folder_message.message_locator = "task1226-same-folder-message".to_owned();
        same_folder_message.email_folder_identity = Some(folder_identity.to_owned());

        let result =
            scan_local_messages(vec![open_message, same_thread_message, same_folder_message]);
        let thread_lists = result
            .email_burn_target_lists
            .iter()
            .filter(|list| list.scope == EmailBurnScope::Thread)
            .collect::<Vec<_>>();
        let folder_lists = result
            .email_burn_target_lists
            .iter()
            .filter(|list| list.scope == EmailBurnScope::Folder)
            .collect::<Vec<_>>();
        let thread_list = thread_lists
            .first()
            .expect("thread burn target list is emitted");
        let folder_list = folder_lists
            .first()
            .expect("folder burn target list is emitted");
        let target_lists_separate = thread_list.scope != folder_list.scope
            && thread_list.identity != folder_list.identity
            && thread_list.message_locators != folder_list.message_locators;

        println!(
            "TASK1226 email_burn_scopes direct_command=scan_local_messages thread_target_list_count={} thread_identity={} thread_targets_count={} thread_targets={} folder_target_list_count={} folder_identity={} folder_targets_count={} folder_targets={} target_lists_separate={}",
            thread_lists.len(),
            thread_list.identity,
            thread_list.message_locators.len(),
            thread_list.message_locators.join(","),
            folder_lists.len(),
            folder_list.identity,
            folder_list.message_locators.len(),
            folder_list.message_locators.join(","),
            target_lists_separate,
        );

        assert_eq!(thread_lists.len(), 1);
        assert_eq!(thread_list.identity, thread_identity);
        assert_eq!(
            thread_list.message_locators,
            vec![
                "task1226-open-message".to_owned(),
                "task1226-same-thread-message".to_owned()
            ]
        );
        assert_eq!(folder_lists.len(), 1);
        assert_eq!(folder_list.identity, folder_identity);
        assert_eq!(
            folder_list.message_locators,
            vec![
                "task1226-open-message".to_owned(),
                "task1226-same-folder-message".to_owned()
            ]
        );
        assert!(target_lists_separate);
    }
}
