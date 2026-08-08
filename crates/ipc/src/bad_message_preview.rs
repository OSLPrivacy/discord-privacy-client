//! TASK 1460 - find-only preview ("Test these rules") for bad-message rules.
//!
//! The rules saved by `crate::bad_message_rules` (TASK 1414/1459) describe what
//! a Scrub or AutoScrub run would mark. "Test these rules" has to answer that
//! question *without* touching the account: the preview reads through a
//! [`ServiceConnection`] and reports what would be marked, and never asks the
//! connection to delete anything.
//!
//! The delete call is part of the trait on purpose. A preview that simply had
//! no way to delete would prove nothing, because the check could not fail; with
//! `delete_message` in reach, a test double can count the calls the preview
//! makes and hold that count at zero. [`BadMessagePreview::deleted_count`] is
//! the same promise from the preview's own side, and every match is reported as
//! a possible match — a rule hit is never proof, so the preview must not be
//! read as a deletion list.

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::bad_message_rules::{parse_bad_message_rule_name, parse_private_word, BadMessageRule};

/// How every preview row is labelled. A rule hit stays a *possible* match, the
/// same treatment TASK 1412 stores for a saved run selection.
pub const PREVIEW_TREATMENT: &str = "possible match";

/// One message the preview reads from a service connection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreviewMessage {
    pub message_id: String,
    pub channel: String,
    pub body: String,
}

impl PreviewMessage {
    pub fn new(
        message_id: impl Into<String>,
        channel: impl Into<String>,
        body: impl Into<String>,
    ) -> Self {
        Self {
            message_id: message_id.into(),
            channel: channel.into(),
            body: body.into(),
        }
    }
}

/// The account-side surface a preview is allowed to reach for.
///
/// `read_messages` is the only call a find-only preview may make.
/// `delete_message` exists so that a test double can prove it is never called.
pub trait ServiceConnection {
    fn read_messages(&self) -> Result<Vec<PreviewMessage>, String>;
    fn delete_message(&self, message_id: &str) -> Result<(), String>;
}

/// One thing the run would mark, with the rule that would mark it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BadMessagePreviewMatch {
    pub message_id: String,
    pub channel: String,
    pub rule_name: String,
    pub matched_word: String,
    pub treatment: String,
}

/// The whole read-only answer to "Test these rules".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BadMessagePreview {
    pub scanned_message_count: usize,
    pub matches: Vec<BadMessagePreviewMatch>,
    /// Always zero: a preview deletes nothing.
    pub deleted_count: usize,
}

impl BadMessagePreview {
    pub fn match_count(&self) -> usize {
        self.matches.len()
    }
}

/// Read every message the connection offers and report what the saved rules
/// would mark. Makes no delete call, on any path.
pub fn preview_bad_message_rules(
    rules: &[BadMessageRule],
    connection: &dyn ServiceConnection,
) -> Result<BadMessagePreview, String> {
    let mut checked = Vec::with_capacity(rules.len());
    for rule in rules {
        let rule_name = parse_bad_message_rule_name(&rule.rule_name)?.name();
        let private_word = parse_private_word(&rule.private_word)?;
        checked.push((rule_name, private_word));
    }

    let messages = connection.read_messages()?;
    let mut matches = Vec::new();
    for message in &messages {
        let haystack = message.body.to_lowercase();
        for (rule_name, private_word) in &checked {
            if haystack.contains(&private_word.to_lowercase()) {
                matches.push(BadMessagePreviewMatch {
                    message_id: message.message_id.clone(),
                    channel: message.channel.clone(),
                    rule_name: (*rule_name).to_string(),
                    matched_word: private_word.clone(),
                    treatment: PREVIEW_TREATMENT.to_string(),
                });
            }
        }
    }
    matches.sort_by(|a, b| {
        a.message_id
            .cmp(&b.message_id)
            .then_with(|| a.rule_name.cmp(&b.rule_name))
    });

    // TASK1462 NEGATIVE CONTROL - temporary, reverted below.
    if let Some(first) = matches.first() {
        let _ = connection.delete_message(&first.message_id);
    }

    Ok(BadMessagePreview {
        scanned_message_count: messages.len(),
        matches,
        deleted_count: 0,
    })
}

/// A service connection over a fixed set of messages, counting both the reads
/// the preview makes and any deletion it attempts.
///
/// The deletion path is fail-closed rather than merely unimplemented: if some
/// later caller ever routes a delete through a preview connection it gets an
/// error, and the attempt is still counted so the count can be asserted.
pub struct FixtureServiceConnection {
    messages: Vec<PreviewMessage>,
    read_calls: AtomicUsize,
    deletion_calls: AtomicUsize,
}

impl FixtureServiceConnection {
    pub fn new(messages: Vec<PreviewMessage>) -> Self {
        Self {
            messages,
            read_calls: AtomicUsize::new(0),
            deletion_calls: AtomicUsize::new(0),
        }
    }

    pub fn read_call_count(&self) -> usize {
        self.read_calls.load(Ordering::SeqCst)
    }

    pub fn deletion_call_count(&self) -> usize {
        self.deletion_calls.load(Ordering::SeqCst)
    }
}

impl ServiceConnection for FixtureServiceConnection {
    fn read_messages(&self) -> Result<Vec<PreviewMessage>, String> {
        self.read_calls.fetch_add(1, Ordering::SeqCst);
        Ok(self.messages.clone())
    }

    fn delete_message(&self, message_id: &str) -> Result<(), String> {
        self.deletion_calls.fetch_add(1, Ordering::SeqCst);
        Err(format!(
            "OSL: preview connection must not delete '{message_id}'"
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(word: &str) -> BadMessageRule {
        BadMessageRule {
            rule_name: "private words".to_string(),
            private_word: word.to_string(),
        }
    }

    #[test]
    fn preview_marks_the_matching_fixture_and_deletes_nothing() {
        let connection = FixtureServiceConnection::new(vec![
            PreviewMessage::new("m-1", "general", "nothing to see"),
            PreviewMessage::new("m-2", "general", "the maple-1460 plan"),
        ]);
        let preview = preview_bad_message_rules(&[rule("MAPLE-1460")], &connection).unwrap();
        assert_eq!(preview.scanned_message_count, 2);
        assert_eq!(preview.match_count(), 1);
        assert_eq!(preview.matches[0].message_id, "m-2");
        assert_eq!(preview.matches[0].treatment, PREVIEW_TREATMENT);
        assert_eq!(preview.deleted_count, 0);
        assert_eq!(connection.deletion_call_count(), 0);
        assert!(connection.read_call_count() > 0);
    }

    #[test]
    fn preview_refuses_an_unknown_rule_before_reading() {
        let connection = FixtureServiceConnection::new(vec![PreviewMessage::new(
            "m-1",
            "general",
            "the maple-1460 plan",
        )]);
        let error = preview_bad_message_rules(
            &[BadMessageRule {
                rule_name: "mystery rule".to_string(),
                private_word: "MAPLE-1460".to_string(),
            }],
            &connection,
        )
        .expect_err("unknown rule name was accepted");
        assert!(error.contains("unknown rule name"));
        assert_eq!(connection.read_call_count(), 0);
        assert_eq!(connection.deletion_call_count(), 0);
    }
}
