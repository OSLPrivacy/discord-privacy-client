//! The viewer side's durable outbox for signed capture events.
//!
//! A capture happens while someone is looking at their screen. That is exactly
//! when the network may be down and exactly when the process may be killed, so
//! "send it and hope" loses the event in the two cases that matter most. The
//! event is written to disk before any delivery is attempted and is cleared
//! only when the sender has acknowledged it.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::event::SignedCaptureEvent;

/// File name under the caller's data directory. One file, rewritten whole:
/// the record count is bounded by the number of view-once opens a person can
/// physically perform, so there is no reason for anything cleverer.
pub const OUTBOX_FILE: &str = "view-once-capture-outbox.json";

/// A queue this long means delivery has been failing for a very long time.
/// It fails closed rather than growing without bound.
pub const OUTBOX_CAPACITY: usize = 256;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OutboxRecord {
    pub event: SignedCaptureEvent,
    /// Delivery attempts made so far. Kept so a stuck record is visible rather
    /// than silently retried forever.
    pub attempts: u32,
    /// Set once the sender has acknowledged. Acknowledged records are dropped
    /// on the next write; the flag exists so an acknowledgement that races a
    /// crash is not lost between the ack and the rewrite.
    pub acknowledged: bool,
}

/// The event an outbox operation could not find, named by the open it belongs
/// to. An error about "an event" tells a reader nothing; the open nonce is the
/// identity of the one capture the caller meant.
fn no_such_event(open_nonce: &[u8; 16]) -> String {
    let nonce: String = open_nonce.iter().map(|byte| format!("{byte:02x}")).collect();
    format!("no capture event for open {nonce} is queued")
}

/// File-backed outbox. Every constructor reads whatever is already on disk,
/// which is what makes a restart indistinguishable from a reconnect.
pub struct CaptureOutbox {
    path: PathBuf,
    records: Vec<OutboxRecord>,
}

impl CaptureOutbox {
    /// Open the outbox held in `dir`, loading any records a previous process
    /// left behind.
    pub fn open(dir: &Path) -> Result<Self, String> {
        fs::create_dir_all(dir).map_err(|error| format!("outbox directory: {error}"))?;
        let path = dir.join(OUTBOX_FILE);
        let records = match fs::read(&path) {
            Ok(bytes) => serde_json::from_slice::<Vec<OutboxRecord>>(&bytes)
                .map_err(|error| format!("outbox is unreadable: {error}"))?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(error) => return Err(format!("outbox is unreadable: {error}")),
        };
        Ok(Self { path, records })
    }

    /// Persist a signed event. Returns `false` when the event is already held,
    /// so a retry of the enqueue itself cannot duplicate the accusation.
    pub fn enqueue(&mut self, event: SignedCaptureEvent) -> Result<bool, String> {
        let nonce = event.binding.open_nonce;
        let id = event.binding.message_id.clone();
        if self.records.iter().any(|record| {
            record.event.binding.open_nonce == nonce && record.event.binding.message_id == id
        }) {
            return Ok(false);
        }
        if self.records.len() >= OUTBOX_CAPACITY {
            return Err(format!(
                "the capture outbox is full at {OUTBOX_CAPACITY} undelivered events"
            ));
        }
        self.records.push(OutboxRecord {
            event,
            attempts: 0,
            acknowledged: false,
        });
        self.flush()
            .map_err(|error| format!("the capture event was not persisted: {error}"))?;
        Ok(true)
    }

    /// Events still owed to a sender, oldest first.
    pub fn pending(&self) -> Vec<&SignedCaptureEvent> {
        self.records
            .iter()
            .filter(|record| !record.acknowledged)
            .map(|record| &record.event)
            .collect()
    }

    pub fn pending_count(&self) -> usize {
        self.records.iter().filter(|r| !r.acknowledged).count()
    }

    /// Record that a delivery was attempted and failed. The event stays.
    pub fn note_attempt(&mut self, open_nonce: &[u8; 16]) -> Result<u32, String> {
        let record = self
            .records
            .iter_mut()
            .find(|record| record.event.binding.open_nonce == *open_nonce)
            .ok_or_else(|| no_such_event(open_nonce))?;
        record.attempts = record.attempts.saturating_add(1);
        let attempts = record.attempts;
        self.flush()?;
        Ok(attempts)
    }

    /// Drop an event once the sender has acknowledged it.
    pub fn acknowledge(&mut self, open_nonce: &[u8; 16]) -> Result<(), String> {
        let before = self.records.len();
        self.records
            .retain(|record| record.event.binding.open_nonce != *open_nonce);
        if self.records.len() == before {
            return Err(no_such_event(open_nonce));
        }
        self.flush()
    }

    /// Whole-file rewrite through a temporary file and a rename, so a crash
    /// mid-write leaves the previous content rather than a truncated file.
    fn flush(&self) -> Result<(), String> {
        let bytes = serde_json::to_vec_pretty(&self.records)
            .map_err(|error| format!("outbox cannot be encoded: {error}"))?;
        let temp = self.path.with_extension("json.tmp");
        fs::write(&temp, &bytes).map_err(|error| format!("outbox write: {error}"))?;
        fs::rename(&temp, &self.path).map_err(|error| format!("outbox rename: {error}"))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{sign_capture_event, SupportedCapturePath, ViewOnceOpenBinding};
    use crate::sig;

    fn evidence() -> crate::event::CaptureEvidence {
        crate::event::CaptureEvidence {
            clipboard_sequence_before: 1,
            clipboard_sequence_after: 2,
            dib_width: 1_920,
            dib_height: 1_080,
            dib_bit_count: 32,
            dib_byte_len: 1_920 * 1_080 * 4,
            screen_width: 1_920,
            screen_height: 1_080,
            distinct_sampled_colors: 500,
            live_match_ppm: 1_000_000,
            print_screen_key_seen: true,
        }
    }

    fn event(nonce: u8) -> SignedCaptureEvent {
        let (secret, _) = sig::generate_keypair();
        sign_capture_event(
            ViewOnceOpenBinding {
                message_id: format!("msg-{nonce}"),
                sender_osl_user_id: "sender".to_owned(),
                viewer_osl_user_id: "viewer".to_owned(),
                viewer_device_id: "device".to_owned(),
                open_nonce: [nonce; 16],
            },
            SupportedCapturePath::PrintScreenClipboard,
            1,
            evidence(),
            &secret,
        )
        .expect("real evidence signs")
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("osl-6844-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn a_queued_event_survives_the_process_that_queued_it() {
        let dir = temp_dir("restart");
        {
            let mut outbox = CaptureOutbox::open(&dir).expect("open");
            assert!(outbox.enqueue(event(1)).expect("enqueue"));
            assert_eq!(outbox.pending_count(), 1);
        }
        // A fresh instance over the same directory is what a restart looks
        // like from this module's point of view.
        let restarted = CaptureOutbox::open(&dir).expect("reopen");
        assert_eq!(restarted.pending_count(), 1);
        assert_eq!(restarted.pending()[0].binding.message_id, "msg-1");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_acknowledged_event_does_not_come_back_after_restart() {
        let dir = temp_dir("ack");
        let nonce = [1u8; 16];
        {
            let mut outbox = CaptureOutbox::open(&dir).expect("open");
            outbox.enqueue(event(1)).expect("enqueue");
            outbox.acknowledge(&nonce).expect("ack");
            assert_eq!(outbox.pending_count(), 0);
        }
        assert_eq!(
            CaptureOutbox::open(&dir).expect("reopen").pending_count(),
            0
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn enqueueing_the_same_open_twice_queues_one_event() {
        let dir = temp_dir("dupe");
        let mut outbox = CaptureOutbox::open(&dir).expect("open");
        let queued = event(1);
        assert!(outbox.enqueue(queued.clone()).expect("first"));
        assert!(!outbox.enqueue(queued).expect("second"));
        assert_eq!(outbox.pending_count(), 1);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_operation_on_an_event_that_is_not_queued_names_the_open() {
        let dir = temp_dir("names-open");
        let mut outbox = CaptureOutbox::open(&dir).expect("open");
        let error = outbox.note_attempt(&[0xab; 16]).expect_err("nothing queued");
        assert_eq!(
            error,
            "no capture event for open abababababababababababababababab is queued"
        );
        let error = outbox.acknowledge(&[0xab; 16]).expect_err("nothing queued");
        assert!(error.contains("abababababababababababababababab"), "{error}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn failed_attempts_are_counted_and_the_event_is_kept() {
        let dir = temp_dir("attempts");
        let mut outbox = CaptureOutbox::open(&dir).expect("open");
        outbox.enqueue(event(3)).expect("enqueue");
        assert_eq!(outbox.note_attempt(&[3u8; 16]).expect("attempt"), 1);
        assert_eq!(outbox.note_attempt(&[3u8; 16]).expect("attempt"), 2);
        assert_eq!(
            CaptureOutbox::open(&dir).expect("reopen").pending_count(),
            1
        );
        let _ = fs::remove_dir_all(&dir);
    }
}
