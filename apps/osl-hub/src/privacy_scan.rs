//! Bounded, deterministic, local-only message risk scanning.
//!
//! This module deliberately has no HTTP/model dependency and performs no I/O.
//! A trusted caller may provide messages from an explicit local export or data
//! already visible to the signed-in user. The separate Scrub index can persist
//! those inputs encrypted and deterministically reproduce findings from disk.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};

use crate::attachment_scan::{
    scan_attachments, AttachmentAnalyzers, LocalAttachmentCandidate, UninspectedAttachment,
    MAX_ATTACHMENTS_PER_MESSAGE, MAX_ATTACHMENT_BYTES,
};

const MAX_MESSAGES: usize = 2_000;
const MAX_TEXT_BYTES: usize = 8 * 1024;
const MAX_LOCATOR_BYTES: usize = 256;
const MAX_PREVIEW_CHARS: usize = 120;
const MAX_SENDER_BYTES: usize = 256;
const MAX_ATTACHMENT_ID_BYTES: usize = 128;
const MAX_ATTACHMENT_DISPLAY_NAME_BYTES: usize = 256;
const MAX_ATTACHMENT_ENCODED_BYTES: usize = MAX_ATTACHMENT_BYTES.div_ceil(3) * 4;
pub(crate) const MAX_ATTACHMENT_BATCH_ENCODED_BYTES: usize = 12 * 1024 * 1024;
pub(crate) const MAX_FINDINGS: usize = 1_000;

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
pub struct ReviewResultAccountGroup {
    pub account_id: String,
    pub matches: Vec<SavedReviewMatch>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewResultStoreOutput {
    pub groups: Vec<ReviewResultAccountGroup>,
    pub total_matches: usize,
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

/// Scan with explicitly supplied local media capabilities. The desktop build
/// supplies none until separately installed adapters and model packs have been
/// verified, so unavailable media is recorded rather than treated as clean.
pub fn scan_local_messages_with_analyzers(
    messages: Vec<LocalMessageCandidate>,
    analyzers: AttachmentAnalyzers<'_>,
) -> LocalPrivacyScanResult {
    let mut findings = Vec::new();
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
                message_locator: message.message_locator.clone(),
                authored_by_self: message.authored_by_self,
                created_at_unix_ms: message.created_at_unix_ms,
                category,
                confidence,
                reason,
                local_preview: preview(&message.text),
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
                    message_locator: message.message_locator.clone(),
                    authored_by_self: message.authored_by_self,
                    created_at_unix_ms: message.created_at_unix_ms,
                    category,
                    confidence,
                    reason,
                    local_preview: preview(&fragment.text),
                    can_request_delete: message.authored_by_self,
                    attachment_path: Some(fragment.path.clone()),
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
                message_locator: message.message_locator.clone(),
                authored_by_self: message.authored_by_self,
                created_at_unix_ms: message.created_at_unix_ms,
                category,
                confidence: signal.confidence,
                reason: signal.reason,
                local_preview: "Local media classification signal".into(),
                can_request_delete: message.authored_by_self,
                attachment_path: Some(signal.path),
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
}
