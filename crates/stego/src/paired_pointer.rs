//! Context-bound, pair-keyed shipping pointers.
//!
//! This is deliberately a separate shipping path from the historical
//! `encode_shrunk_token` compatibility API.  The historical compact token is
//! reversibly readable as an 80-bit handle; this protocol is not.  Its cover
//! choices carry only a keyed permutation of an 80-bit pointer.  The pointer,
//! record address, cover seed and record key are all derived from a pair key
//! and authenticated message context, and are never serialized in the cover.

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{Key, XChaCha20Poly1305, XNonce};
use hkdf::Hkdf;
use sha2::Sha256;
use thiserror::Error;

/// The private pointer is exactly 80 bits.
pub const POINTER_BYTES: usize = 10;
/// No detector, seed or clear pointer is appended to the shipping cover.
pub const SHIPPING_CARRIED_BITS: u32 = 80;

const COVER_MASK_DOMAIN: &[u8] = b"osl/task-0071/cover-observable-mask/v2";
const COVER_SEED_DOMAIN: &[u8] = b"osl/task-0071/cover-seed/v2";
const RECORD_KEY_DOMAIN: &[u8] = b"osl/task-0071/protected-record-key/v2";
const RECORD_NONCE_DOMAIN: &[u8] = b"osl/task-0071/protected-record-nonce/v2";
const RECORD_ADDRESS_DOMAIN: &[u8] = b"osl/task-0071/protected-record-address/v2";
const RECORD_AAD_DOMAIN: &[u8] = b"osl/task-0071/protected-record-aad/v2";

#[derive(Debug, Error, PartialEq, Eq)]
pub enum PairPointerError {
    #[error("shipping carrier is not the exact canonical 80-bit cover")]
    InvalidCarrier,
    #[error("protected record authentication failed")]
    AuthenticationFailed,
}

/// An opaque record address.  It is derived locally and is not present in a
/// [`CarrierCapture`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RecordAddress(pub [u8; 32]);

/// Encrypted, authenticated material stored at a locally-derived address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtectedRecord {
    pub address: RecordAddress,
    pub ciphertext: Vec<u8>,
}

/// The exact unmodified text captured from the shipping carrier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CarrierCapture {
    pub cover_text: String,
}

/// Pair-key protocol state.  `authenticated_context` must be held by both
/// peers (for example, the authenticated conversation/message binding).
#[derive(Clone)]
pub struct PairPointerProtocol {
    pair_key: Vec<u8>,
    authenticated_context: Vec<u8>,
}

impl PairPointerProtocol {
    pub fn new(pair_key: &[u8], authenticated_context: &[u8]) -> Self {
        Self {
            pair_key: pair_key.to_vec(),
            authenticated_context: authenticated_context.to_vec(),
        }
    }

    fn derive<const N: usize>(
        &self,
        domain: &[u8],
        pointer: Option<&[u8; POINTER_BYTES]>,
    ) -> [u8; N] {
        let hk = Hkdf::<Sha256>::new(Some(&self.authenticated_context), &self.pair_key);
        let mut info = Vec::with_capacity(domain.len() + POINTER_BYTES);
        info.extend_from_slice(domain);
        if let Some(pointer) = pointer {
            info.extend_from_slice(pointer);
        }
        let mut out = [0u8; N];
        hk.expand(&info, &mut out)
            .expect("fixed HKDF output length");
        out
    }

    /// A derived cover seed for callers that need deterministic cover styling.
    /// It is intentionally not sent in the capture.
    pub fn cover_seed(&self) -> [u8; 32] {
        self.derive(COVER_SEED_DOMAIN, None)
    }

    fn observable_bytes(&self, pointer: &[u8; POINTER_BYTES]) -> [u8; POINTER_BYTES] {
        let cover_seed = self.cover_seed();
        let hk = Hkdf::<Sha256>::new(Some(&self.authenticated_context), &cover_seed);
        let mut mask = [0u8; POINTER_BYTES];
        hk.expand(COVER_MASK_DOMAIN, &mut mask)
            .expect("fixed HKDF output length");
        std::array::from_fn(|i| pointer[i] ^ mask[i])
    }

    fn pointer_from_observable(&self, observable: &[u8; POINTER_BYTES]) -> [u8; POINTER_BYTES] {
        // XOR with an HKDF-derived pad is a keyed permutation.  In particular,
        // replacing it with identity makes the public codec reversible, which
        // the task proof deliberately detects.
        self.observable_bytes(observable)
    }

    pub fn record_address(&self, pointer: &[u8; POINTER_BYTES]) -> RecordAddress {
        RecordAddress(self.derive(RECORD_ADDRESS_DOMAIN, Some(pointer)))
    }

    fn record_aad(&self, pointer: &[u8; POINTER_BYTES]) -> Vec<u8> {
        let mut aad = Vec::with_capacity(
            RECORD_AAD_DOMAIN.len() + self.authenticated_context.len() + POINTER_BYTES,
        );
        aad.extend_from_slice(RECORD_AAD_DOMAIN);
        aad.extend_from_slice(&self.authenticated_context);
        aad.extend_from_slice(pointer);
        aad
    }

    /// Produce the shipping cover and the encrypted addressed record.  The
    /// cover has exactly 80 meaningful bits: the keyed observable mapping.
    pub fn seal(
        &self,
        pointer: [u8; POINTER_BYTES],
        plaintext: &[u8],
    ) -> (CarrierCapture, ProtectedRecord) {
        let observable = self.observable_bytes(&pointer);
        let cover_text = encode_observable_cover(&observable);
        let key = self.derive::<32>(RECORD_KEY_DOMAIN, Some(&pointer));
        let nonce = self.derive::<24>(RECORD_NONCE_DOMAIN, Some(&pointer));
        let cipher = XChaCha20Poly1305::new(Key::from_slice(&key));
        let ciphertext = cipher
            .encrypt(
                XNonce::from_slice(&nonce),
                Payload {
                    msg: plaintext,
                    aad: &self.record_aad(&pointer),
                },
            )
            .expect("fixed key and nonce lengths");
        (
            CarrierCapture { cover_text },
            ProtectedRecord {
                address: self.record_address(&pointer),
                ciphertext,
            },
        )
    }

    /// Decode only a canonical capture.  It returns the pointer only to a peer
    /// holding this pair key and authenticated context; a failed record open
    /// releases no plaintext.
    pub fn open(
        &self,
        capture: &CarrierCapture,
        record: &ProtectedRecord,
    ) -> Result<([u8; POINTER_BYTES], Vec<u8>), PairPointerError> {
        let observable = decode_observable_cover(&capture.cover_text)?;
        let pointer = self.pointer_from_observable(&observable);
        if record.address != self.record_address(&pointer) {
            return Err(PairPointerError::AuthenticationFailed);
        }
        let key = self.derive::<32>(RECORD_KEY_DOMAIN, Some(&pointer));
        let nonce = self.derive::<24>(RECORD_NONCE_DOMAIN, Some(&pointer));
        let cipher = XChaCha20Poly1305::new(Key::from_slice(&key));
        let plaintext = cipher
            .decrypt(
                XNonce::from_slice(&nonce),
                Payload {
                    msg: &record.ciphertext,
                    aad: &self.record_aad(&pointer),
                },
            )
            .map_err(|_| PairPointerError::AuthenticationFailed)?;
        Ok((pointer, plaintext))
    }
}

/// Public fixed word-bank rendering of the already-keyed observable bits.
/// This function intentionally has no key parameter; secrecy comes from the
/// mapping before these public cover choices are made.
pub fn encode_observable_cover(observable: &[u8; POINTER_BYTES]) -> String {
    let bits = bytes_to_bits(observable);
    let words = crate::bigram::legacy_wide_decode_bits(&bits, SHIPPING_CARRIED_BITS);
    crate::bigram::render_words(&words)
}

/// Public structural decode used by auditing tools.  It exposes observable
/// cover choices, never the pointer; only `PairPointerProtocol::open` applies
/// the keyed inverse mapping and authenticates the protected record.
pub fn decode_observable_cover(cover_text: &str) -> Result<[u8; POINTER_BYTES], PairPointerError> {
    let words = crate::bigram::parse_words(cover_text).ok_or(PairPointerError::InvalidCarrier)?;
    let bits = crate::bigram::legacy_wide_encode_words(&words, SHIPPING_CARRIED_BITS);
    let observable = bits_to_bytes(&bits).ok_or(PairPointerError::InvalidCarrier)?;
    if encode_observable_cover(&observable) != cover_text {
        return Err(PairPointerError::InvalidCarrier);
    }
    Ok(observable)
}

fn bytes_to_bits(bytes: &[u8; POINTER_BYTES]) -> Vec<bool> {
    bytes
        .iter()
        .flat_map(|byte| (0..8).rev().map(move |shift| byte & (1 << shift) != 0))
        .collect()
}

fn bits_to_bytes(bits: &[bool]) -> Option<[u8; POINTER_BYTES]> {
    if bits.len() != SHIPPING_CARRIED_BITS as usize {
        return None;
    }
    let mut out = [0u8; POINTER_BYTES];
    for (index, bit) in bits.iter().enumerate() {
        out[index / 8] = (out[index / 8] << 1) | *bit as u8;
    }
    Some(out)
}
