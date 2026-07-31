//! Bounded, deterministic, local-only message risk scanning.
//!
//! This module deliberately has no HTTP/model dependency and performs no I/O.
//! A trusted caller may provide messages from an explicit local export or data
//! already visible to the signed-in user. The separate Scrub index can persist
//! those inputs encrypted and deterministically reproduce findings from disk.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

const MAX_MESSAGES: usize = 2_000;
const MAX_TEXT_BYTES: usize = 8 * 1024;
const MAX_LOCATOR_BYTES: usize = 256;
const MAX_PREVIEW_CHARS: usize = 120;
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
}

/// Scan bounded caller-provided text entirely in process memory.
///
/// Invalid or oversized records are rejected rather than partially scanned.
/// At most one finding per category is emitted for a message. The caller must
/// still confirm that a locator belongs to the active trusted service context
/// before offering jump/delete actions.
pub fn scan_local_messages(messages: Vec<LocalMessageCandidate>) -> LocalPrivacyScanResult {
    let mut findings = Vec::new();
    let mut messages_scanned = 0usize;
    let mut messages_rejected = messages.len().saturating_sub(MAX_MESSAGES);
    let mut truncated = messages.len() > MAX_MESSAGES;

    for message in messages.into_iter().take(MAX_MESSAGES) {
        if !valid_candidate(&message) {
            messages_rejected += 1;
            continue;
        }
        messages_scanned += 1;
        let mut categories = HashSet::new();
        for (category, confidence, reason) in classify(&message.text) {
            if !categories.insert(category) {
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
            });
        }
    }

    LocalPrivacyScanResult {
        findings,
        messages_scanned,
        messages_rejected,
        truncated,
        analysis_location: "this_device_only",
        persisted: false,
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
        && !message.text.is_empty()
        && message.text.len() <= MAX_TEXT_BYTES
        && !message.text.contains('\0')
}

fn valid_id_byte(byte: u8) -> bool {
    byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_')
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

    fn message(text: &str) -> LocalMessageCandidate {
        LocalMessageCandidate {
            service_id: "instagram".to_owned(),
            account_id: "qa-account".to_owned(),
            conversation_id: "conversation-1".to_owned(),
            message_locator: "opaque-message-1".to_owned(),
            authored_by_self: true,
            created_at_unix_ms: Some(1_700_000_000_000),
            text: text.to_owned(),
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
}
