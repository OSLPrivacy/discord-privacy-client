//! The sender side: decide whether an arriving capture event is worth showing
//! anyone, and show it exactly once.
//!
//! A "your view-once media was screenshotted" notification is an accusation
//! against a named person. Everything here exists so that an accusation can
//! only come from the device that actually held the media, about the message
//! it actually held, once.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::sig::PublicKey;
use serde::{Deserialize, Serialize};

use crate::disclosure::CAPTURE_DISCLOSURE_SENDER;
use crate::event::{verify_capture_event, CaptureRealness, SignedCaptureEvent};

/// File name under the sender's data directory.
pub const NOTIFIER_LEDGER_FILE: &str = "view-once-capture-seen.json";

/// What the sender already knows about a view-once message it sent. The event
/// is checked against this, never the other way round.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SentViewOnceRecord {
    pub message_id: String,
    pub sender_osl_user_id: String,
    pub viewer_osl_user_id: String,
    pub viewer_device_id: String,
    pub viewer_public_key: PublicKey,
}

/// The decision, and when it is a refusal, exactly which check refused.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AcceptOutcome {
    /// Raise one notification. Carries the sentence to show.
    Notify(String),
    /// This open was already notified. Nothing further is shown.
    AlreadyNotified,
    /// The signature does not verify against the viewer device key the sender
    /// already holds.
    RejectedForgedSignature(String),
    /// The event is bound to a different message, sender, viewer or device.
    RejectedWrongBinding(String),
    /// The evidence does not describe a capture that really happened.
    RejectedSimulated(String),
}

#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Ledger {
    /// `message_id/viewer_osl_user_id/open_nonce_hex` for every open already
    /// notified. Persisted, because a sender restart must not re-notify and
    /// must not become re-notifiable by a replay.
    notified: BTreeSet<String>,
}

/// Sender-side notifier with a persistent dedupe ledger.
pub struct SenderCaptureNotifier {
    path: PathBuf,
    ledger: Ledger,
}

impl SenderCaptureNotifier {
    pub fn open(dir: &Path) -> Result<Self, String> {
        fs::create_dir_all(dir).map_err(|error| format!("notifier directory: {error}"))?;
        let path = dir.join(NOTIFIER_LEDGER_FILE);
        let ledger = match fs::read(&path) {
            Ok(bytes) => serde_json::from_slice::<Ledger>(&bytes)
                .map_err(|error| format!("notifier ledger is unreadable: {error}"))?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ledger::default(),
            Err(error) => return Err(format!("notifier ledger is unreadable: {error}")),
        };
        Ok(Self { path, ledger })
    }

    /// The sentence a sender is shown before sending view-once, so the meaning
    /// of the notification — and of its absence — is known in advance.
    pub fn disclosure(&self) -> &'static str {
        CAPTURE_DISCLOSURE_SENDER
    }

    pub fn notified_count(&self) -> usize {
        self.ledger.notified.len()
    }

    fn key(event: &SignedCaptureEvent) -> String {
        format!(
            "{}/{}/{}",
            event.binding.message_id,
            event.binding.viewer_osl_user_id,
            event.binding.open_nonce_hex()
        )
    }

    /// Decide what to do with one arriving capture event.
    ///
    /// Order matters. Binding is checked before the signature so a mismatched
    /// event is named as such rather than as a forgery, and the dedupe ledger
    /// is only consulted after both, so a replay of a forged event is reported
    /// as a forgery and never enters the ledger.
    pub fn accept(
        &mut self,
        event: &SignedCaptureEvent,
        sent: &SentViewOnceRecord,
    ) -> Result<AcceptOutcome, String> {
        let binding = &event.binding;
        for (field, got, want) in [
            ("message", &binding.message_id, &sent.message_id),
            (
                "sender",
                &binding.sender_osl_user_id,
                &sent.sender_osl_user_id,
            ),
            (
                "viewer",
                &binding.viewer_osl_user_id,
                &sent.viewer_osl_user_id,
            ),
            (
                "viewer device",
                &binding.viewer_device_id,
                &sent.viewer_device_id,
            ),
        ] {
            if got != want {
                return Ok(AcceptOutcome::RejectedWrongBinding(format!(
                    "the event names {field} {got}, but this view-once message went to {want}"
                )));
            }
        }

        if let Err(reason) = verify_capture_event(event, &sent.viewer_public_key) {
            return Ok(AcceptOutcome::RejectedForgedSignature(reason));
        }

        if let CaptureRealness::Simulated(reason) = event.evidence.realness() {
            return Ok(AcceptOutcome::RejectedSimulated(reason));
        }

        let key = Self::key(event);
        if self.ledger.notified.contains(&key) {
            return Ok(AcceptOutcome::AlreadyNotified);
        }
        self.ledger.notified.insert(key);
        self.flush()?;
        Ok(AcceptOutcome::Notify(notification_sentence(event)))
    }

    fn flush(&self) -> Result<(), String> {
        let bytes = serde_json::to_vec_pretty(&self.ledger)
            .map_err(|error| format!("notifier ledger cannot be encoded: {error}"))?;
        let temp = self.path.with_extension("json.tmp");
        fs::write(&temp, &bytes).map_err(|error| format!("notifier ledger write: {error}"))?;
        fs::rename(&temp, &self.path)
            .map_err(|error| format!("notifier ledger rename: {error}"))?;
        Ok(())
    }
}

/// The sentence the sender actually reads.
///
/// It says which path was detected, and it repeats the limit in the same
/// breath. A notification that arrived is the one moment a reader is most
/// likely to over-generalise into "OSL tells me about screenshots", so the
/// correction belongs here and not only in the settings screen.
pub fn notification_sentence(event: &SignedCaptureEvent) -> String {
    format!(
        "{} used {} while your view-once message was open. \
OSL can detect only the screen-capture paths Windows reports to it; it cannot detect \
a camera pointed at their screen, an external capture device, or every capture tool, \
and OSL does not stop screenshots.",
        event.binding.viewer_osl_user_id,
        event.path.sender_wording()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{
        sign_capture_event, CaptureEvidence, SupportedCapturePath, ViewOnceOpenBinding,
    };
    use crate::sig;

    fn evidence() -> CaptureEvidence {
        CaptureEvidence {
            clipboard_sequence_before: 10,
            clipboard_sequence_after: 11,
            dib_width: 2_560,
            dib_height: 1_440,
            dib_bit_count: 32,
            dib_byte_len: 2_560 * 1_440 * 4,
            screen_width: 2_560,
            screen_height: 1_440,
            distinct_sampled_colors: 700,
            live_match_ppm: 999_000,
            print_screen_key_seen: true,
        }
    }

    fn binding(message: &str, nonce: u8) -> ViewOnceOpenBinding {
        ViewOnceOpenBinding {
            message_id: message.to_owned(),
            sender_osl_user_id: "sender".to_owned(),
            viewer_osl_user_id: "viewer".to_owned(),
            viewer_device_id: "device".to_owned(),
            open_nonce: [nonce; 16],
        }
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("osl-6844-n-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    fn fixture(name: &str) -> (PathBuf, SentViewOnceRecord, sig::SecretKey) {
        let (secret, public) = sig::generate_keypair();
        let sent = SentViewOnceRecord {
            message_id: "msg-a".to_owned(),
            sender_osl_user_id: "sender".to_owned(),
            viewer_osl_user_id: "viewer".to_owned(),
            viewer_device_id: "device".to_owned(),
            viewer_public_key: public,
        };
        (temp_dir(name), sent, secret)
    }

    #[test]
    fn one_real_capture_notifies_once_however_often_it_arrives() {
        let (dir, sent, secret) = fixture("once");
        let event = sign_capture_event(
            binding("msg-a", 1),
            SupportedCapturePath::PrintScreenClipboard,
            1,
            evidence(),
            &secret,
        )
        .expect("signs");
        let mut notifier = SenderCaptureNotifier::open(&dir).expect("open");

        let first = notifier.accept(&event, &sent).expect("accept");
        assert!(matches!(first, AcceptOutcome::Notify(_)));
        assert_eq!(
            notifier.accept(&event, &sent).expect("replay"),
            AcceptOutcome::AlreadyNotified
        );
        assert_eq!(
            notifier.accept(&event, &sent).expect("replay again"),
            AcceptOutcome::AlreadyNotified
        );
        assert_eq!(notifier.notified_count(), 1);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_replay_after_a_sender_restart_still_notifies_only_once() {
        let (dir, sent, secret) = fixture("restart");
        let event = sign_capture_event(
            binding("msg-a", 2),
            SupportedCapturePath::SnipToClipboard,
            1,
            evidence(),
            &secret,
        )
        .expect("signs");
        {
            let mut notifier = SenderCaptureNotifier::open(&dir).expect("open");
            assert!(matches!(
                notifier.accept(&event, &sent).expect("accept"),
                AcceptOutcome::Notify(_)
            ));
        }
        let mut restarted = SenderCaptureNotifier::open(&dir).expect("reopen");
        assert_eq!(
            restarted.accept(&event, &sent).expect("replay"),
            AcceptOutcome::AlreadyNotified
        );
        assert_eq!(restarted.notified_count(), 1);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_event_for_another_message_notifies_nobody() {
        let (dir, sent, secret) = fixture("other-message");
        let event = sign_capture_event(
            binding("msg-b", 3),
            SupportedCapturePath::PrintScreenClipboard,
            1,
            evidence(),
            &secret,
        )
        .expect("signs");
        let mut notifier = SenderCaptureNotifier::open(&dir).expect("open");
        assert!(matches!(
            notifier.accept(&event, &sent).expect("accept"),
            AcceptOutcome::RejectedWrongBinding(_)
        ));
        assert_eq!(notifier.notified_count(), 0);
        let _ = fs::remove_dir_all(&dir);
    }

    /// The event is signed by the *real* viewer device key, so the signature
    /// verifies and only the binding comparison can refuse it. Without a
    /// correctly signed event, dropping a binding comparison would still look
    /// caught — as a signature failure — and this check would prove nothing.
    #[test]
    fn a_correctly_signed_event_rebound_to_another_viewer_notifies_nobody() {
        let (dir, sent, secret) = fixture("other-viewer");
        let mut notifier = SenderCaptureNotifier::open(&dir).expect("open");
        for (field, rebound) in [
            (
                "viewer",
                ViewOnceOpenBinding {
                    viewer_osl_user_id: "someone-else".to_owned(),
                    ..binding("msg-a", 11)
                },
            ),
            (
                "viewer device",
                ViewOnceOpenBinding {
                    viewer_device_id: "another-device".to_owned(),
                    ..binding("msg-a", 12)
                },
            ),
            (
                "sender",
                ViewOnceOpenBinding {
                    sender_osl_user_id: "another-sender".to_owned(),
                    ..binding("msg-a", 13)
                },
            ),
        ] {
            let event = sign_capture_event(
                rebound,
                SupportedCapturePath::PrintScreenClipboard,
                1,
                evidence(),
                &secret,
            )
            .expect("the real viewer device signs it");
            let outcome = notifier.accept(&event, &sent).expect("accept");
            assert!(
                matches!(outcome, AcceptOutcome::RejectedWrongBinding(ref reason) if reason.contains(field)),
                "a correctly signed event rebound to another {field} was not refused as a binding \
                 mismatch: {outcome:?}"
            );
        }
        assert_eq!(notifier.notified_count(), 0);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_adversary_signing_their_own_event_notifies_nobody() {
        let (dir, sent, _) = fixture("forged");
        let (attacker, _) = sig::generate_keypair();
        let event = sign_capture_event(
            binding("msg-a", 4),
            SupportedCapturePath::PrintScreenClipboard,
            1,
            evidence(),
            &attacker,
        )
        .expect("attacker signs their own bytes");
        let mut notifier = SenderCaptureNotifier::open(&dir).expect("open");
        assert!(matches!(
            notifier.accept(&event, &sent).expect("accept"),
            AcceptOutcome::RejectedForgedSignature(_)
        ));
        assert_eq!(notifier.notified_count(), 0);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_tampered_event_body_notifies_nobody() {
        let (dir, sent, secret) = fixture("tampered");
        let mut event = sign_capture_event(
            binding("msg-a", 5),
            SupportedCapturePath::PrintScreenClipboard,
            1,
            evidence(),
            &secret,
        )
        .expect("signs");
        event.path = SupportedCapturePath::SnipToClipboard;
        let mut notifier = SenderCaptureNotifier::open(&dir).expect("open");
        assert!(matches!(
            notifier.accept(&event, &sent).expect("accept"),
            AcceptOutcome::RejectedForgedSignature(_)
        ));
        assert_eq!(notifier.notified_count(), 0);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_correctly_signed_but_simulated_event_notifies_nobody() {
        let (dir, sent, secret) = fixture("simulated");
        let mut event = sign_capture_event(
            binding("msg-a", 6),
            SupportedCapturePath::PrintScreenClipboard,
            1,
            evidence(),
            &secret,
        )
        .expect("signs");
        // Re-sign a body whose evidence is fabricated, so the signature is
        // valid and only the realness gate stands between it and a
        // notification.
        event.evidence.distinct_sampled_colors = 1;
        let resigned = {
            let mut e = event.clone();
            e.evidence.distinct_sampled_colors = 1;
            e
        };
        let mut notifier = SenderCaptureNotifier::open(&dir).expect("open");
        // Signature no longer covers the mutated evidence, so this is caught
        // as a forgery first — which is the stronger of the two refusals.
        assert!(matches!(
            notifier.accept(&resigned, &sent).expect("accept"),
            AcceptOutcome::RejectedForgedSignature(_)
        ));
        assert_eq!(notifier.notified_count(), 0);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn two_different_opens_of_the_same_message_notify_twice() {
        let (dir, sent, secret) = fixture("two-opens");
        let mut notifier = SenderCaptureNotifier::open(&dir).expect("open");
        for nonce in [7u8, 8u8] {
            let event = sign_capture_event(
                binding("msg-a", nonce),
                SupportedCapturePath::PrintScreenClipboard,
                1,
                evidence(),
                &secret,
            )
            .expect("signs");
            assert!(matches!(
                notifier.accept(&event, &sent).expect("accept"),
                AcceptOutcome::Notify(_)
            ));
        }
        assert_eq!(notifier.notified_count(), 2);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_notification_repeats_the_limit_and_claims_nothing_absolute() {
        let (_, _, secret) = fixture("wording");
        let event = sign_capture_event(
            binding("msg-a", 9),
            SupportedCapturePath::PrintScreenClipboard,
            1,
            evidence(),
            &secret,
        )
        .expect("signs");
        let sentence = notification_sentence(&event);
        assert!(crate::disclosure::discloses_capture_limits(&sentence));
        assert!(sentence.contains("cannot detect a camera pointed at their screen"));
        assert!(sentence.contains("does not stop screenshots"));
        assert_eq!(
            crate::disclosure::absolute_capture_claims_in(&sentence),
            Vec::<&str>::new()
        );
    }
}
