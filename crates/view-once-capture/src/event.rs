//! The capture event: what was observed, what it is bound to, and its
//! signature.

use crate::sig::{self, PublicKey, SecretKey, SIGNATURE_SIZE};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Domain separator. Present in every signed body so a signature minted for
/// any other OSL structure can never be read as a capture accusation.
pub const EVENT_DOMAIN: &[u8] = b"OSL-VIEW-ONCE-CAPTURE-EVENT-v1";

/// A real desktop has texture. A fabricated "capture" — a solid rectangle, a
/// zeroed buffer, a test fixture — does not. Sampled distinct colours below
/// this floor is treated as evidence of a simulated event, not a screenshot.
pub const MIN_DISTINCT_SAMPLED_COLORS: usize = 64;

/// The captured bitmap has to actually look like the screen it claims to be a
/// capture of, measured against an independent readback taken at observation
/// time. Parts per million of sampled pixels that must agree.
pub const MIN_LIVE_MATCH_PPM: u32 = 900_000;

/// A Windows capture path OSL can observe.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SupportedCapturePath {
    /// PrintScreen or Alt+PrintScreen: the key was seen and the OS then put a
    /// full-screen bitmap on the clipboard.
    PrintScreenClipboard,
    /// A bitmap of the screen arrived on the clipboard with no PrintScreen key
    /// preceding it — the Windows snip (Win+Shift+S) and Game Bar behave this
    /// way.
    SnipToClipboard,
}

impl SupportedCapturePath {
    pub const fn label(self) -> &'static str {
        match self {
            Self::PrintScreenClipboard => "print-screen-clipboard",
            Self::SnipToClipboard => "snip-to-clipboard",
        }
    }

    /// Wording shown to the sender. Names the path, because "a screenshot was
    /// taken" is a stronger claim than we can make: what we know is that one
    /// specific OS path fired.
    pub const fn sender_wording(self) -> &'static str {
        match self {
            Self::PrintScreenClipboard => "the PrintScreen key",
            Self::SnipToClipboard => "the Windows snip",
        }
    }
}

/// A capture path OSL cannot observe. Present so the code can name what it is
/// blind to, and so an observation that lands here can be proven to produce no
/// notification.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum UnsupportedCapturePath {
    /// A camera pointed at the monitor.
    Camera,
    /// A capture card, a virtual display driver, a second machine.
    ExternalDevice,
    /// A tool that reads the desktop into its own process — a screen recorder,
    /// a remote-desktop client, anything using BitBlt or the Desktop
    /// Duplication API. Nothing reaches the clipboard and no key is pressed,
    /// so Windows tells us nothing.
    OutOfProcessGrab,
}

impl UnsupportedCapturePath {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Camera => "camera",
            Self::ExternalDevice => "external-device",
            Self::OutOfProcessGrab => "out-of-process-grab",
        }
    }
}

/// The exact viewer open session a capture is attributed to.
///
/// `open_nonce` is fresh per open. It is what makes two captures of two
/// different opens of the same message distinguishable, and it is the dedupe
/// key on the sender side.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewOnceOpenBinding {
    pub message_id: String,
    pub sender_osl_user_id: String,
    pub viewer_osl_user_id: String,
    pub viewer_device_id: String,
    pub open_nonce: [u8; 16],
}

impl ViewOnceOpenBinding {
    pub fn open_nonce_hex(&self) -> String {
        self.open_nonce.iter().map(|b| format!("{b:02x}")).collect()
    }
}

/// Measurements taken at the moment of observation. These are numbers about
/// the capture, never the captured pixels: the pixels are the recipient's
/// screen and are not ours to keep, hash-and-discard is the most we do.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureEvidence {
    /// `GetClipboardSequenceNumber` before and after. Equal means nothing
    /// arrived on the clipboard at all.
    pub clipboard_sequence_before: u32,
    pub clipboard_sequence_after: u32,
    /// Dimensions declared by the clipboard DIB header.
    pub dib_width: i32,
    pub dib_height: i32,
    pub dib_bit_count: u16,
    pub dib_byte_len: u64,
    /// Live `GetSystemMetrics(SM_C?VIRTUALSCREEN)` at observation time.
    pub screen_width: i32,
    pub screen_height: i32,
    /// Distinct colours over a fixed sample grid of the captured bitmap.
    pub distinct_sampled_colors: usize,
    /// Parts per million of sampled pixels where the clipboard bitmap agrees
    /// with an independent readback of the live screen.
    pub live_match_ppm: u32,
    /// Whether the PrintScreen key was seen by the low-level hook within the
    /// correlation window before the clipboard bitmap arrived.
    pub print_screen_key_seen: bool,
}

/// Why a capture was, or was not, accepted as real.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CaptureRealness {
    Real,
    Simulated(String),
}

impl CaptureEvidence {
    /// A stable digest of the measurements, bound into the signature so the
    /// numbers a sender is shown are the numbers the viewer device signed.
    pub fn digest(&self) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(b"OSL-VIEW-ONCE-CAPTURE-EVIDENCE-v1");
        hasher.update(self.clipboard_sequence_before.to_le_bytes());
        hasher.update(self.clipboard_sequence_after.to_le_bytes());
        hasher.update(self.dib_width.to_le_bytes());
        hasher.update(self.dib_height.to_le_bytes());
        hasher.update(self.dib_bit_count.to_le_bytes());
        hasher.update(self.dib_byte_len.to_le_bytes());
        hasher.update(self.screen_width.to_le_bytes());
        hasher.update(self.screen_height.to_le_bytes());
        hasher.update((self.distinct_sampled_colors as u64).to_le_bytes());
        hasher.update(self.live_match_ppm.to_le_bytes());
        hasher.update([u8::from(self.print_screen_key_seen)]);
        hasher.finalize().into()
    }

    /// Decide whether these measurements describe a capture that actually
    /// happened on this machine's screen.
    ///
    /// This is the gate that keeps a fabricated event out. Every clause is a
    /// property a real Windows capture has and a hand-written struct does not:
    /// the clipboard moved, the bitmap covers the whole virtual screen, it has
    /// real colour texture, and it agrees with what the screen actually showed.
    pub fn realness(&self) -> CaptureRealness {
        if self.clipboard_sequence_after == self.clipboard_sequence_before {
            return CaptureRealness::Simulated(
                "the clipboard sequence number never advanced".to_owned(),
            );
        }
        if self.screen_width <= 0 || self.screen_height <= 0 {
            return CaptureRealness::Simulated("there is no live screen to capture".to_owned());
        }
        // A bottom-up DIB reports a positive height, a top-down DIB a negative
        // one. Both are real; the magnitude is what must match the screen.
        if self.dib_width != self.screen_width || self.dib_height.abs() != self.screen_height {
            return CaptureRealness::Simulated(format!(
                "the bitmap is {}x{} but the live screen is {}x{}",
                self.dib_width,
                self.dib_height.abs(),
                self.screen_width,
                self.screen_height
            ));
        }
        if self.dib_bit_count < 16 {
            return CaptureRealness::Simulated(format!(
                "the bitmap is {} bits per pixel",
                self.dib_bit_count
            ));
        }
        let expected_min = (self.screen_width as u64)
            .saturating_mul(self.screen_height as u64)
            .saturating_mul(u64::from(self.dib_bit_count) / 8);
        if self.dib_byte_len < expected_min {
            return CaptureRealness::Simulated(format!(
                "the bitmap is {} bytes, short of the {expected_min} a {}x{} capture needs",
                self.dib_byte_len, self.screen_width, self.screen_height
            ));
        }
        if self.distinct_sampled_colors < MIN_DISTINCT_SAMPLED_COLORS {
            return CaptureRealness::Simulated(format!(
                "the bitmap has {} distinct sampled colours, below the {MIN_DISTINCT_SAMPLED_COLORS} a real desktop shows",
                self.distinct_sampled_colors
            ));
        }
        if self.live_match_ppm < MIN_LIVE_MATCH_PPM {
            return CaptureRealness::Simulated(format!(
                "the bitmap agrees with the live screen on only {} ppm of sampled pixels, below {MIN_LIVE_MATCH_PPM}",
                self.live_match_ppm
            ));
        }
        CaptureRealness::Real
    }
}

/// A signed accusation that one supported capture path fired during one
/// specific view-once open.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignedCaptureEvent {
    pub binding: ViewOnceOpenBinding,
    pub path: SupportedCapturePath,
    pub observed_at_ms: u64,
    pub evidence: CaptureEvidence,
    /// The viewer device's Ed25519 public key, carried so the sender can say
    /// which key it is checking against the one it already knows.
    pub viewer_public_key: [u8; 32],
    /// Hex on the wire: serde has no `Deserialize` for a 64-byte array, and a
    /// signature is read by humans in evidence files often enough that hex is
    /// the friendlier encoding anyway.
    #[serde(with = "hex_signature")]
    pub signature: [u8; SIGNATURE_SIZE],
}

mod hex_signature {
    use super::SIGNATURE_SIZE;
    use serde::{de::Error as _, Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(
        signature: &[u8; SIGNATURE_SIZE],
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        let hex: String = signature.iter().map(|byte| format!("{byte:02x}")).collect();
        serializer.serialize_str(&hex)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<[u8; SIGNATURE_SIZE], D::Error> {
        let hex = String::deserialize(deserializer)?;
        if hex.len() != SIGNATURE_SIZE * 2 {
            return Err(D::Error::custom("a capture signature is 64 bytes"));
        }
        let mut bytes = [0u8; SIGNATURE_SIZE];
        for (index, slot) in bytes.iter_mut().enumerate() {
            *slot = u8::from_str_radix(&hex[index * 2..index * 2 + 2], 16)
                .map_err(|_| D::Error::custom("a capture signature is hexadecimal"))?;
        }
        Ok(bytes)
    }
}

/// Length-prefixed, domain-separated encoding of everything the signature
/// covers.
///
/// Length prefixes rather than separators: with separators, a viewer id ending
/// in the separator could be slid into the device id field and produce the
/// same bytes for two different bindings.
fn signing_body(
    binding: &ViewOnceOpenBinding,
    path: SupportedCapturePath,
    observed_at_ms: u64,
    evidence: &CaptureEvidence,
) -> Vec<u8> {
    let mut body = Vec::new();
    let mut push = |bytes: &[u8]| {
        body.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        body.extend_from_slice(bytes);
    };
    push(EVENT_DOMAIN);
    push(binding.message_id.as_bytes());
    push(binding.sender_osl_user_id.as_bytes());
    push(binding.viewer_osl_user_id.as_bytes());
    push(binding.viewer_device_id.as_bytes());
    push(&binding.open_nonce);
    push(path.label().as_bytes());
    push(&observed_at_ms.to_le_bytes());
    push(&evidence.digest());
    body
}

/// Sign a capture event with the viewer device key.
///
/// Refuses to sign evidence that does not describe a real capture. A viewer
/// device that cannot prove the capture happened has nothing to accuse anyone
/// of, and an unsigned nothing is the correct output.
pub fn sign_capture_event(
    binding: ViewOnceOpenBinding,
    path: SupportedCapturePath,
    observed_at_ms: u64,
    evidence: CaptureEvidence,
    viewer_secret: &SecretKey,
) -> Result<SignedCaptureEvent, String> {
    if let CaptureRealness::Simulated(reason) = evidence.realness() {
        return Err(format!("refusing to accuse anyone: {reason}"));
    }
    let body = signing_body(&binding, path, observed_at_ms, &evidence);
    let signature = sig::sign(viewer_secret, &body);
    Ok(SignedCaptureEvent {
        binding,
        path,
        observed_at_ms,
        evidence,
        viewer_public_key: *sig::derive_public(viewer_secret).as_bytes(),
        signature,
    })
}

/// Verify the signature over a capture event against `expected_viewer_key`.
///
/// The key is supplied by the caller, never taken from the event: an attacker
/// who could nominate the key that checks their own signature has not been
/// checked at all.
pub fn verify_capture_event(
    event: &SignedCaptureEvent,
    expected_viewer_key: &PublicKey,
) -> Result<(), String> {
    if event.viewer_public_key != *expected_viewer_key.as_bytes() {
        return Err("the event names a different viewer device key".to_owned());
    }
    let body = signing_body(
        &event.binding,
        event.path,
        event.observed_at_ms,
        &event.evidence,
    );
    if sig::verify(expected_viewer_key, &body, &event.signature) {
        Ok(())
    } else {
        Err("the capture event signature does not verify".to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) fn real_evidence() -> CaptureEvidence {
        CaptureEvidence {
            clipboard_sequence_before: 8_188,
            clipboard_sequence_after: 8_192,
            dib_width: 5_760,
            dib_height: 1_200,
            dib_bit_count: 32,
            dib_byte_len: 27_648_052,
            screen_width: 5_760,
            screen_height: 1_200,
            distinct_sampled_colors: 855,
            live_match_ppm: 1_000_000,
            print_screen_key_seen: true,
        }
    }

    pub(crate) fn binding() -> ViewOnceOpenBinding {
        ViewOnceOpenBinding {
            message_id: "msg-6844-a".to_owned(),
            sender_osl_user_id: "sender-1".to_owned(),
            viewer_osl_user_id: "viewer-1".to_owned(),
            viewer_device_id: "viewer-device-1".to_owned(),
            open_nonce: [7u8; 16],
        }
    }

    #[test]
    fn a_real_capture_signs_and_verifies() {
        let (secret, public) = sig::generate_keypair();
        let event = sign_capture_event(
            binding(),
            SupportedCapturePath::PrintScreenClipboard,
            1,
            real_evidence(),
            &secret,
        )
        .expect("real evidence signs");
        assert_eq!(verify_capture_event(&event, &public), Ok(()));
    }

    #[test]
    fn a_flipped_signature_byte_does_not_verify() {
        let (secret, public) = sig::generate_keypair();
        let mut event = sign_capture_event(
            binding(),
            SupportedCapturePath::PrintScreenClipboard,
            1,
            real_evidence(),
            &secret,
        )
        .expect("real evidence signs");
        event.signature[0] ^= 0x01;
        assert!(verify_capture_event(&event, &public).is_err());
    }

    #[test]
    fn another_key_cannot_mint_an_event_for_this_viewer() {
        let (_, victim_public) = sig::generate_keypair();
        let (attacker_secret, _) = sig::generate_keypair();
        let event = sign_capture_event(
            binding(),
            SupportedCapturePath::PrintScreenClipboard,
            1,
            real_evidence(),
            &attacker_secret,
        )
        .expect("attacker can sign their own bytes");
        assert!(verify_capture_event(&event, &victim_public).is_err());
    }

    #[test]
    fn every_bound_field_is_covered_by_the_signature() {
        let (secret, public) = sig::generate_keypair();
        let base = sign_capture_event(
            binding(),
            SupportedCapturePath::PrintScreenClipboard,
            1_700_000_000_000,
            real_evidence(),
            &secret,
        )
        .expect("real evidence signs");

        let mutations: Vec<(&str, Box<dyn Fn(&mut SignedCaptureEvent)>)> = vec![
            (
                "messageId",
                Box::new(|e: &mut SignedCaptureEvent| {
                    e.binding.message_id = "msg-6844-b".to_owned()
                }),
            ),
            (
                "senderOslUserId",
                Box::new(|e: &mut SignedCaptureEvent| {
                    e.binding.sender_osl_user_id = "sender-2".to_owned()
                }),
            ),
            (
                "viewerOslUserId",
                Box::new(|e: &mut SignedCaptureEvent| {
                    e.binding.viewer_osl_user_id = "viewer-2".to_owned()
                }),
            ),
            (
                "viewerDeviceId",
                Box::new(|e: &mut SignedCaptureEvent| {
                    e.binding.viewer_device_id = "viewer-device-2".to_owned()
                }),
            ),
            (
                "openNonce",
                Box::new(|e: &mut SignedCaptureEvent| e.binding.open_nonce = [9u8; 16]),
            ),
            (
                "path",
                Box::new(|e: &mut SignedCaptureEvent| {
                    e.path = SupportedCapturePath::SnipToClipboard
                }),
            ),
            (
                "observedAtMs",
                Box::new(|e: &mut SignedCaptureEvent| e.observed_at_ms += 1),
            ),
            (
                "evidence",
                Box::new(|e: &mut SignedCaptureEvent| e.evidence.dib_byte_len += 1),
            ),
        ];
        for (field, mutate) in mutations {
            let mut mutated = base.clone();
            mutate(&mut mutated);
            assert!(
                verify_capture_event(&mutated, &public).is_err(),
                "changing {field} must break the signature"
            );
        }
    }

    #[test]
    fn field_boundaries_cannot_be_slid_between_ids() {
        let mut left = binding();
        left.viewer_osl_user_id = "viewer".to_owned();
        left.viewer_device_id = "1-device".to_owned();
        let mut right = binding();
        right.viewer_osl_user_id = "viewer1".to_owned();
        right.viewer_device_id = "-device".to_owned();
        assert_ne!(
            signing_body(
                &left,
                SupportedCapturePath::SnipToClipboard,
                1,
                &real_evidence()
            ),
            signing_body(
                &right,
                SupportedCapturePath::SnipToClipboard,
                1,
                &real_evidence()
            ),
        );
    }

    #[test]
    fn simulated_evidence_is_refused_before_it_can_be_signed() {
        let (secret, _) = sig::generate_keypair();
        let cases: Vec<(&str, CaptureEvidence)> = vec![
            (
                "clipboard never moved",
                CaptureEvidence {
                    clipboard_sequence_after: 8_188,
                    ..real_evidence()
                },
            ),
            (
                "wrong dimensions",
                CaptureEvidence {
                    dib_width: 800,
                    ..real_evidence()
                },
            ),
            (
                "flat colour",
                CaptureEvidence {
                    distinct_sampled_colors: 1,
                    ..real_evidence()
                },
            ),
            (
                "does not match the live screen",
                CaptureEvidence {
                    live_match_ppm: 10_000,
                    ..real_evidence()
                },
            ),
            (
                "too few bytes for the claimed screen",
                CaptureEvidence {
                    dib_byte_len: 1_024,
                    ..real_evidence()
                },
            ),
        ];
        for (name, evidence) in cases {
            assert!(
                matches!(evidence.realness(), CaptureRealness::Simulated(_)),
                "{name} must not read as real"
            );
            assert!(
                sign_capture_event(
                    binding(),
                    SupportedCapturePath::PrintScreenClipboard,
                    1,
                    evidence,
                    &secret
                )
                .is_err(),
                "{name} must not be signable"
            );
        }
    }

    #[test]
    fn a_top_down_dib_is_still_a_real_capture() {
        let evidence = CaptureEvidence {
            dib_height: -1_200,
            ..real_evidence()
        };
        assert_eq!(evidence.realness(), CaptureRealness::Real);
    }
}
