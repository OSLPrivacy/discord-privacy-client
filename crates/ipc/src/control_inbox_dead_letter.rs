//! Dead-letter ledger for undispatchable control-inbox rows.
//!
//! Tracks dispatch attempts and terminal rows by control-inbox row id.
//! Lives in a separate file so the server inbox remains the source of
//! undelivered rows while the local client records why a row is no longer
//! eligible for dispatch. Same on-disk encryption-at-rest path via
//! `main_password::maybe_encrypt` as the other scoped JSON state files.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

pub const MAX_CONTROL_INBOX_ATTEMPTS: u32 = 8;
pub const REASON_DISCORD_SNOWFLAKE_SENDER: &str = "discord_snowflake_sender";
pub const REASON_MAX_ATTEMPTS: &str = "max_dispatch_attempts";

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ControlInboxDeadLetterFile {
    #[serde(default)]
    pub version: u32,
    #[serde(default)]
    pub entries: BTreeMap<String, ControlInboxDeadLetterEntry>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ControlInboxDeadLetterEntry {
    #[serde(default)]
    pub attempts: u32,
    #[serde(default)]
    pub next_attempt_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal_reason: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DispatchFailureOutcome {
    RetryScheduled { attempts: u32, next_attempt_at: i64 },
    Terminal { attempts: u32 },
}

impl ControlInboxDeadLetterFile {
    pub fn is_terminal(&self, row_id: &str) -> bool {
        self.entries
            .get(row_id)
            .and_then(|entry| entry.terminal_reason.as_ref())
            .is_some()
    }

    pub fn should_skip_for_backoff(&self, row_id: &str, now_unix_secs: i64) -> bool {
        self.entries
            .get(row_id)
            .map(|entry| entry.terminal_reason.is_none() && entry.next_attempt_at > now_unix_secs)
            .unwrap_or(false)
    }

    pub fn terminal_count(&self) -> u32 {
        self.entries
            .values()
            .filter(|entry| entry.terminal_reason.is_some())
            .count()
            .try_into()
            .unwrap_or(u32::MAX)
    }

    pub fn mark_terminal(&mut self, row_id: &str, reason: &'static str) {
        let entry = self.entries.entry(row_id.to_string()).or_default();
        entry.terminal_reason = Some(reason.to_string());
        if reason == REASON_DISCORD_SNOWFLAKE_SENDER {
            entry.next_attempt_at = 0;
        }
        self.version = 1;
    }

    pub fn record_dispatch_failure(
        &mut self,
        row_id: &str,
        now_unix_secs: i64,
    ) -> DispatchFailureOutcome {
        let entry = self.entries.entry(row_id.to_string()).or_default();
        entry.attempts = entry.attempts.saturating_add(1);
        self.version = 1;

        if entry.attempts >= MAX_CONTROL_INBOX_ATTEMPTS {
            entry.terminal_reason = Some(REASON_MAX_ATTEMPTS.to_string());
            entry.next_attempt_at = 0;
            return DispatchFailureOutcome::Terminal {
                attempts: entry.attempts,
            };
        }

        let delay_secs = retry_delay_secs(entry.attempts);
        entry.next_attempt_at = now_unix_secs.saturating_add(delay_secs);
        DispatchFailureOutcome::RetryScheduled {
            attempts: entry.attempts,
            next_attempt_at: entry.next_attempt_at,
        }
    }
}

fn retry_delay_secs(attempts: u32) -> i64 {
    let shift = attempts.saturating_sub(1).min(30);
    60_i64.saturating_mul(1_i64 << shift)
}

pub fn load_control_inbox_dead_letter(path: &Path) -> ControlInboxDeadLetterFile {
    let Ok(blob) = std::fs::read(path) else {
        return ControlInboxDeadLetterFile::default();
    };
    let plain = match crate::main_password::maybe_decrypt(&blob) {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!(error = %e, "OSL: load control_inbox_dead_letter.json decrypt failed");
            return ControlInboxDeadLetterFile::default();
        }
    };
    serde_json::from_slice(&plain).unwrap_or_default()
}

pub fn write_control_inbox_dead_letter(
    path: &Path,
    file: &ControlInboxDeadLetterFile,
) -> Result<(), String> {
    let body = serde_json::to_vec_pretty(file)
        .map_err(|e| format!("OSL: serialize control_inbox_dead_letter: {e}"))?;
    let out = crate::main_password::maybe_encrypt(&body)
        .map_err(|e| format!("OSL: encrypt control_inbox_dead_letter: {e}"))?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, &out).map_err(|e| format!("OSL: write {}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("OSL: rename {}: {e}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        load_control_inbox_dead_letter, write_control_inbox_dead_letter,
        ControlInboxDeadLetterEntry, ControlInboxDeadLetterFile, DispatchFailureOutcome,
        MAX_CONTROL_INBOX_ATTEMPTS, REASON_DISCORD_SNOWFLAKE_SENDER, REASON_MAX_ATTEMPTS,
    };
    use tempfile::tempdir;

    struct FileKeyReset;

    impl Drop for FileKeyReset {
        fn drop(&mut self) {
            crate::main_password::set_file_storage_key(None);
        }
    }

    #[test]
    fn transient_failure_is_retried_and_next_attempt_grows_between_attempts() {
        let mut file = ControlInboxDeadLetterFile::default();
        let first = file.record_dispatch_failure("row-a", 1_000);
        let second = file.record_dispatch_failure("row-a", 1_000);

        let (
            DispatchFailureOutcome::RetryScheduled {
                attempts: first_attempts,
                next_attempt_at: first_next,
            },
            DispatchFailureOutcome::RetryScheduled {
                attempts: second_attempts,
                next_attempt_at: second_next,
            },
        ) = (first, second)
        else {
            panic!("first two failures must be retryable");
        };

        assert_eq!(first_attempts, 1);
        assert_eq!(second_attempts, 2);
        assert!(second_next > first_next);
        assert!(file.should_skip_for_backoff("row-a", first_next - 1));
        assert!(!file.should_skip_for_backoff("row-a", second_next));
    }

    #[test]
    fn row_is_not_terminal_before_max_control_inbox_attempts() {
        let mut file = ControlInboxDeadLetterFile::default();
        for _ in 0..(MAX_CONTROL_INBOX_ATTEMPTS - 1) {
            file.record_dispatch_failure("row-a", 1_000);
        }

        let entry = file.entries.get("row-a").expect("entry");
        assert_eq!(entry.attempts, MAX_CONTROL_INBOX_ATTEMPTS - 1);
        assert_eq!(entry.terminal_reason, None);
        assert!(!file.is_terminal("row-a"));
    }

    #[test]
    fn at_max_control_inbox_attempts_row_becomes_terminal_and_is_skipped() {
        let mut file = ControlInboxDeadLetterFile::default();
        for _ in 0..(MAX_CONTROL_INBOX_ATTEMPTS - 1) {
            file.record_dispatch_failure("row-a", 1_000);
        }

        let outcome = file.record_dispatch_failure("row-a", 1_000);
        assert_eq!(
            outcome,
            DispatchFailureOutcome::Terminal {
                attempts: MAX_CONTROL_INBOX_ATTEMPTS
            }
        );

        let entry = file.entries.get("row-a").expect("entry");
        assert_eq!(entry.attempts, MAX_CONTROL_INBOX_ATTEMPTS);
        assert_eq!(entry.terminal_reason.as_deref(), Some(REASON_MAX_ATTEMPTS));
        assert!(file.is_terminal("row-a"));
    }

    #[test]
    fn surfaced_terminal_count_reflects_entries_and_zero_when_none() {
        let mut file = ControlInboxDeadLetterFile::default();
        file.entries.insert(
            "retrying".to_string(),
            ControlInboxDeadLetterEntry {
                attempts: 3,
                next_attempt_at: 1_200,
                terminal_reason: None,
            },
        );
        assert_eq!(file.terminal_count(), 0);

        file.mark_terminal("dead-a", REASON_DISCORD_SNOWFLAKE_SENDER);
        file.mark_terminal("dead-b", REASON_MAX_ATTEMPTS);
        assert_eq!(file.terminal_count(), 2);
    }

    #[test]
    fn control_inbox_dead_letter_round_trips_through_encryption_path_unchanged() {
        use crate::main_password::{has_enc_magic, set_file_storage_key};

        let _reset = FileKeyReset;
        set_file_storage_key(Some([0x55u8; 32]));
        let dir = tempdir().unwrap();
        let path = dir.path().join("control_inbox_dead_letter.json");

        let mut file = ControlInboxDeadLetterFile {
            version: 1,
            ..Default::default()
        };
        file.entries.insert(
            "row-a".to_string(),
            ControlInboxDeadLetterEntry {
                attempts: 2,
                next_attempt_at: 1_240,
                terminal_reason: None,
            },
        );
        file.mark_terminal("row-b", REASON_MAX_ATTEMPTS);

        write_control_inbox_dead_letter(&path, &file).expect("write encrypted");
        let raw = std::fs::read(&path).expect("read encrypted");
        assert!(has_enc_magic(&raw));
        assert_eq!(load_control_inbox_dead_letter(&path), file);

        set_file_storage_key(None);
    }

    #[test]
    fn control_inbox_dead_letter_reason_strings_contain_no_identifiers() {
        let identifiers = [
            "123456789012345678",
            "dm:123456789012345678",
            "message-123",
            "scope:secret",
            "sender:alice",
            "row-a",
        ];

        for reason in [REASON_DISCORD_SNOWFLAKE_SENDER, REASON_MAX_ATTEMPTS] {
            for identifier in identifiers {
                assert!(
                    !reason.contains(identifier),
                    "reason {reason:?} must not contain identifier {identifier:?}"
                );
            }
        }
    }
}
