//! Manifest schema, canonical-bytes encoding, and Ed25519 sign /
//! verify.
//!
//! ## Wire format
//!
//! The signed wire object is JSON:
//!
//! ```json
//! {
//!   "version": 1,
//!   "manifest_b64": "<base64 of the canonical manifest bytes>",
//!   "signature_b64": "<base64 of Ed25519(canonical bytes)>",
//!   "signing_key_b64": "<base64 of the 32-byte signer pub>"
//! }
//! ```
//!
//! The client trusts a single hard-coded `signing_key_b64` value
//! (baked at build time). The field travels alongside the manifest
//! purely so the verifier can produce a clear error message when the
//! server-side key drifts from the expected one — the client always
//! re-checks against its own constant before accepting.
//!
//! ## Canonical bytes
//!
//! Both client and server reconstruct the same byte string for
//! signing / verification. The encoding is independent of any JSON
//! library quirks (key ordering, whitespace) and matches the
//! length-prefixed style used by the prekey + burn modules:
//!
//!   domain (LP, "discord-privacy-client/selector-manifest/v1")
//!   version (u32 BE)
//!   issued_at_unix_seconds (u64 BE)
//!   client_min_version (LP, semver string e.g. "0.1.0")
//!   selector_count (u32 BE)
//!   per selector (sorted by key):
//!     key (LP)
//!     value (LP)
//!
//! Selectors are sorted by key (lexicographic) before encoding.
//! Sorting is the only nondeterminism we eliminate at signing time;
//! the JSON layer is allowed to be lazy because we never sign a JSON
//! representation.

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use crypto::ed25519;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use thiserror::Error;

pub const MANIFEST_DOMAIN: &[u8] =
    b"discord-privacy-client/selector-manifest/v1";

/// 24-hour staleness window per design doc § "Selector resilience".
pub const MAX_MANIFEST_AGE_SECONDS: u64 = 24 * 60 * 60;

/// On-the-wire form. The signature covers
/// [`canonical_manifest_bytes`] of the inner [`SelectorManifest`].
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SignedManifest {
    /// Wire-envelope version. `1` for now; bump if the envelope
    /// changes (e.g. multi-sig in v2.5).
    pub version: u32,
    /// Base64-encoded canonical manifest bytes. Carrying the bytes
    /// directly (rather than re-serialising server-supplied JSON)
    /// guarantees signature stability across JSON-pretty-print
    /// differences.
    pub manifest_b64: String,
    /// Base64-encoded Ed25519 signature.
    pub signature_b64: String,
    /// Base64-encoded 32-byte Ed25519 public key the server claims
    /// signed this manifest. The client cross-checks against its own
    /// trusted constant before accepting; this field exists for
    /// human-readable error reporting on key rotation.
    pub signing_key_b64: String,
}

/// Decoded manifest content. `BTreeMap` keeps selectors in
/// deterministic order on serialise.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SelectorManifest {
    /// Manifest schema version. `1` for the initial format. Bump on
    /// breaking schema changes (adding optional fields is non-breaking
    /// because canonical bytes are explicitly enumerated).
    pub version: u32,
    /// When the publisher signed this. Used for the 24-hour
    /// staleness gate.
    pub issued_at_unix_seconds: u64,
    /// Minimum client version that understands the keys named in
    /// `selectors`. Display-only — client uses this in the banner.
    pub client_min_version: String,
    /// `module_role -> webpack_selector_string`. Roles are
    /// hard-coded constants on the client side
    /// (`MessageContent`, `MessageTextarea`, …); values are opaque
    /// strings the client passes to its webpack-resolver shim.
    pub selectors: BTreeMap<String, String>,
}

/// Compute the canonical byte string covered by the signature. Must
/// agree with the server-side implementation byte-for-byte.
pub fn canonical_manifest_bytes(m: &SelectorManifest) -> Vec<u8> {
    let mut buf = Vec::new();
    write_lp(&mut buf, MANIFEST_DOMAIN);
    buf.extend_from_slice(&m.version.to_be_bytes());
    buf.extend_from_slice(&m.issued_at_unix_seconds.to_be_bytes());
    write_lp(&mut buf, m.client_min_version.as_bytes());
    buf.extend_from_slice(&(m.selectors.len() as u32).to_be_bytes());
    for (k, v) in &m.selectors {
        write_lp(&mut buf, k.as_bytes());
        write_lp(&mut buf, v.as_bytes());
    }
    buf
}

/// Sign a manifest, producing the wire-shaped [`SignedManifest`].
/// The signing-key field is filled in from the public half of
/// `signer`.
pub fn sign_manifest(
    signer_secret: &ed25519::SecretKey,
    signer_public: &ed25519::PublicKey,
    manifest: &SelectorManifest,
) -> SignedManifest {
    let bytes = canonical_manifest_bytes(manifest);
    let sig = ed25519::sign(signer_secret, &bytes);
    SignedManifest {
        version: 1,
        manifest_b64: STANDARD.encode(&bytes),
        signature_b64: STANDARD.encode(sig.as_bytes()),
        signing_key_b64: STANDARD.encode(signer_public.as_bytes()),
    }
}

/// Errors verifying a signed manifest.
#[derive(Debug, Error)]
pub enum ManifestError {
    #[error("envelope version mismatch: got {got}, expected {expected}")]
    EnvelopeVersion { got: u32, expected: u32 },

    #[error("base64 decode error in field {field}: {source}")]
    Base64 {
        field: &'static str,
        #[source]
        source: base64::DecodeError,
    },

    #[error(
        "signing key mismatch: server presented {presented}, client trusts {trusted}"
    )]
    SigningKeyMismatch {
        presented: String,
        trusted: String,
    },

    #[error("signature length wrong: got {got}, expected 64")]
    SignatureLength { got: usize },

    #[error("public key length wrong: got {got}, expected 32")]
    PublicKeyLength { got: usize },

    #[error("Ed25519 signature verification failed")]
    BadSignature,

    #[error("crypto verification error: {0}")]
    CryptoVerify(String),

    #[error("manifest body did not parse as JSON: {0}")]
    BodyJson(#[from] serde_json::Error),

    #[error(
        "manifest is stale: signed at {issued_at_unix_seconds}, now {now_unix_seconds}, max age {max_age}s"
    )]
    Stale {
        issued_at_unix_seconds: u64,
        now_unix_seconds: u64,
        max_age: u64,
    },

    #[error("manifest issued in the future: signed at {issued_at_unix_seconds}, now {now_unix_seconds}")]
    Future {
        issued_at_unix_seconds: u64,
        now_unix_seconds: u64,
    },
}

/// Parse the wire JSON envelope.
pub fn parse_signed_manifest(json: &[u8]) -> Result<SignedManifest, ManifestError> {
    Ok(serde_json::from_slice(json)?)
}

/// Verify and decode a signed manifest. Performs, in order:
///
/// 1. Envelope version check (`version == 1`).
/// 2. Server-presented signing key MUST equal `trusted_pub_b64`. If
///    the strings disagree even once, the manifest is rejected with
///    [`ManifestError::SigningKeyMismatch`] — the trusted constant is
///    the client's anchor, not whatever the server claims.
/// 3. Decode signature + manifest bytes.
/// 4. Ed25519 verify with the *trusted* public key.
/// 5. Parse the canonical bytes as JSON canonical via the LP
///    encoding's reverse — actually the canonical bytes themselves are
///    not JSON, so we skip that and decode the manifest fields from
///    the trusted base64 of the LP-encoded bytes by simply parsing
///    them back. To keep this simple, the server also publishes the
///    JSON manifest alongside the canonical bytes; the verifier
///    reconstructs canonical bytes from the parsed manifest and
///    compares to confirm bit-for-bit equivalence.
///
/// `now_unix_seconds` is the caller's notion of "now"; tests pass a
/// fixed value, production passes wall-clock. The caller is the
/// trust root for time — there's no NTP coupling here.
pub fn verify_manifest(
    signed: &SignedManifest,
    trusted_pub_b64: &str,
    now_unix_seconds: u64,
) -> Result<SelectorManifest, ManifestError> {
    if signed.version != 1 {
        return Err(ManifestError::EnvelopeVersion {
            got: signed.version,
            expected: 1,
        });
    }
    if signed.signing_key_b64 != trusted_pub_b64 {
        return Err(ManifestError::SigningKeyMismatch {
            presented: signed.signing_key_b64.clone(),
            trusted: trusted_pub_b64.to_string(),
        });
    }
    let pub_bytes = STANDARD
        .decode(trusted_pub_b64)
        .map_err(|source| ManifestError::Base64 {
            field: "signing_key_b64",
            source,
        })?;
    if pub_bytes.len() != 32 {
        return Err(ManifestError::PublicKeyLength {
            got: pub_bytes.len(),
        });
    }
    let mut pub_arr = [0u8; 32];
    pub_arr.copy_from_slice(&pub_bytes);
    let pubkey = ed25519::PublicKey::from_bytes(pub_arr);

    let sig_bytes = STANDARD
        .decode(&signed.signature_b64)
        .map_err(|source| ManifestError::Base64 {
            field: "signature_b64",
            source,
        })?;
    if sig_bytes.len() != 64 {
        return Err(ManifestError::SignatureLength {
            got: sig_bytes.len(),
        });
    }
    let mut sig_arr = [0u8; 64];
    sig_arr.copy_from_slice(&sig_bytes);
    let sig = ed25519::Signature::from_bytes(sig_arr);

    let manifest_bytes = STANDARD
        .decode(&signed.manifest_b64)
        .map_err(|source| ManifestError::Base64 {
            field: "manifest_b64",
            source,
        })?;

    let ok = ed25519::verify(&pubkey, &manifest_bytes, &sig)
        .map_err(|e| ManifestError::CryptoVerify(format!("{e}")))?;
    if !ok {
        return Err(ManifestError::BadSignature);
    }

    let manifest = decode_canonical_bytes(&manifest_bytes)?;

    // Recompute canonical bytes from the parsed manifest and confirm
    // they round-trip exactly. Catches any mismatch between what the
    // signer put in `manifest_b64` and what the manifest actually
    // says.
    let recomputed = canonical_manifest_bytes(&manifest);
    if recomputed != manifest_bytes {
        return Err(ManifestError::BadSignature);
    }

    if manifest.issued_at_unix_seconds > now_unix_seconds {
        return Err(ManifestError::Future {
            issued_at_unix_seconds: manifest.issued_at_unix_seconds,
            now_unix_seconds,
        });
    }
    let age = now_unix_seconds.saturating_sub(manifest.issued_at_unix_seconds);
    if age > MAX_MANIFEST_AGE_SECONDS {
        return Err(ManifestError::Stale {
            issued_at_unix_seconds: manifest.issued_at_unix_seconds,
            now_unix_seconds,
            max_age: MAX_MANIFEST_AGE_SECONDS,
        });
    }
    Ok(manifest)
}

/// Decode the LP-encoded canonical bytes back into a
/// [`SelectorManifest`]. Strict: each LP-prefixed string must be
/// valid UTF-8.
fn decode_canonical_bytes(bytes: &[u8]) -> Result<SelectorManifest, ManifestError> {
    let mut r = ByteReader::new(bytes);
    let domain = r.read_lp().ok_or(ManifestError::BadSignature)?;
    if domain != MANIFEST_DOMAIN {
        return Err(ManifestError::BadSignature);
    }
    let version = r.read_u32_be().ok_or(ManifestError::BadSignature)?;
    let issued_at_unix_seconds =
        r.read_u64_be().ok_or(ManifestError::BadSignature)?;
    let client_min_version = r
        .read_lp_str()
        .ok_or(ManifestError::BadSignature)?;
    let count = r.read_u32_be().ok_or(ManifestError::BadSignature)?;
    let mut selectors = BTreeMap::new();
    for _ in 0..count {
        let k = r.read_lp_str().ok_or(ManifestError::BadSignature)?;
        let v = r.read_lp_str().ok_or(ManifestError::BadSignature)?;
        selectors.insert(k, v);
    }
    if !r.is_at_end() {
        return Err(ManifestError::BadSignature);
    }
    Ok(SelectorManifest {
        version,
        issued_at_unix_seconds,
        client_min_version,
        selectors,
    })
}

fn write_lp(buf: &mut Vec<u8>, bytes: &[u8]) {
    buf.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
    buf.extend_from_slice(bytes);
}

struct ByteReader<'a> {
    data: &'a [u8],
    cursor: usize,
}

impl<'a> ByteReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        ByteReader { data, cursor: 0 }
    }
    fn remaining(&self) -> usize {
        self.data.len() - self.cursor
    }
    fn is_at_end(&self) -> bool {
        self.cursor == self.data.len()
    }
    fn read_u32_be(&mut self) -> Option<u32> {
        if self.remaining() < 4 {
            return None;
        }
        let mut a = [0u8; 4];
        a.copy_from_slice(&self.data[self.cursor..self.cursor + 4]);
        self.cursor += 4;
        Some(u32::from_be_bytes(a))
    }
    fn read_u64_be(&mut self) -> Option<u64> {
        if self.remaining() < 8 {
            return None;
        }
        let mut a = [0u8; 8];
        a.copy_from_slice(&self.data[self.cursor..self.cursor + 8]);
        self.cursor += 8;
        Some(u64::from_be_bytes(a))
    }
    fn read_lp(&mut self) -> Option<&'a [u8]> {
        let len = self.read_u32_be()? as usize;
        if self.remaining() < len {
            return None;
        }
        let s = &self.data[self.cursor..self.cursor + len];
        self.cursor += len;
        Some(s)
    }
    fn read_lp_str(&mut self) -> Option<String> {
        let s = self.read_lp()?;
        std::str::from_utf8(s).ok().map(|s| s.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(now_unix: u64) -> SelectorManifest {
        let mut sel = BTreeMap::new();
        sel.insert("MessageContent".to_string(), "abcd".to_string());
        sel.insert("MessageTextarea".to_string(), "wxyz".to_string());
        SelectorManifest {
            version: 1,
            issued_at_unix_seconds: now_unix,
            client_min_version: "0.1.0".to_string(),
            selectors: sel,
        }
    }

    fn fresh_signer() -> (ed25519::SecretKey, ed25519::PublicKey, String) {
        let (s, p) = ed25519::generate_keypair();
        let pub_b64 = STANDARD.encode(p.as_bytes());
        (s, p, pub_b64)
    }

    #[test]
    fn round_trip_sign_and_verify() {
        let (s, p, pub_b64) = fresh_signer();
        let m = sample(1_700_000_000);
        let signed = sign_manifest(&s, &p, &m);
        let got = verify_manifest(&signed, &pub_b64, 1_700_000_000).unwrap();
        assert_eq!(got, m);
    }

    #[test]
    fn rejects_signature_with_wrong_key() {
        let (s, p, _real_pub_b64) = fresh_signer();
        let (_, _, evil_pub_b64) = fresh_signer();
        let m = sample(1_700_000_000);
        let signed = sign_manifest(&s, &p, &m);
        // Verifier holds the evil pub as trusted constant. Server
        // presented the real pub — mismatch.
        let err = verify_manifest(&signed, &evil_pub_b64, 1_700_000_000).unwrap_err();
        assert!(matches!(err, ManifestError::SigningKeyMismatch { .. }));
    }

    #[test]
    fn rejects_tampered_manifest_bytes() {
        let (s, p, pub_b64) = fresh_signer();
        let m = sample(1_700_000_000);
        let mut signed = sign_manifest(&s, &p, &m);
        // Flip one base64 char at a stable offset (the LP-encoded
        // first byte after the domain).
        let mut bytes = STANDARD.decode(&signed.manifest_b64).unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 0x01;
        signed.manifest_b64 = STANDARD.encode(&bytes);
        let err = verify_manifest(&signed, &pub_b64, 1_700_000_000).unwrap_err();
        assert!(matches!(err, ManifestError::BadSignature));
    }

    #[test]
    fn rejects_when_signature_does_not_cover_provided_bytes() {
        let (s, p, pub_b64) = fresh_signer();
        let m1 = sample(1_700_000_000);
        let m2 = {
            let mut m = m1.clone();
            m.selectors
                .insert("ExtraInjected".to_string(), "evil".to_string());
            m
        };
        // Sign m1, then ship m2's canonical bytes — verify must reject.
        let mut signed = sign_manifest(&s, &p, &m1);
        signed.manifest_b64 = STANDARD.encode(canonical_manifest_bytes(&m2));
        let err = verify_manifest(&signed, &pub_b64, 1_700_000_000).unwrap_err();
        assert!(matches!(err, ManifestError::BadSignature));
    }

    #[test]
    fn rejects_stale_manifest_past_24h() {
        let (s, p, pub_b64) = fresh_signer();
        let m = sample(1_700_000_000);
        let signed = sign_manifest(&s, &p, &m);
        let later = 1_700_000_000 + MAX_MANIFEST_AGE_SECONDS + 1;
        let err = verify_manifest(&signed, &pub_b64, later).unwrap_err();
        assert!(matches!(err, ManifestError::Stale { .. }));
    }

    #[test]
    fn accepts_manifest_at_exact_24h_boundary() {
        let (s, p, pub_b64) = fresh_signer();
        let m = sample(1_700_000_000);
        let signed = sign_manifest(&s, &p, &m);
        let boundary = 1_700_000_000 + MAX_MANIFEST_AGE_SECONDS;
        let got = verify_manifest(&signed, &pub_b64, boundary).unwrap();
        assert_eq!(got, m);
    }

    #[test]
    fn rejects_manifest_issued_in_the_future() {
        let (s, p, pub_b64) = fresh_signer();
        let m = sample(1_700_000_500);
        let signed = sign_manifest(&s, &p, &m);
        let err = verify_manifest(&signed, &pub_b64, 1_700_000_100).unwrap_err();
        assert!(matches!(err, ManifestError::Future { .. }));
    }

    #[test]
    fn rejects_envelope_version_other_than_one() {
        let (s, p, pub_b64) = fresh_signer();
        let m = sample(1_700_000_000);
        let mut signed = sign_manifest(&s, &p, &m);
        signed.version = 2;
        let err = verify_manifest(&signed, &pub_b64, 1_700_000_000).unwrap_err();
        assert!(matches!(err, ManifestError::EnvelopeVersion { .. }));
    }

    #[test]
    fn canonical_bytes_are_deterministic_under_insert_order() {
        let mut a = BTreeMap::new();
        a.insert("z".to_string(), "1".to_string());
        a.insert("a".to_string(), "2".to_string());

        let mut b = BTreeMap::new();
        b.insert("a".to_string(), "2".to_string());
        b.insert("z".to_string(), "1".to_string());

        let m_a = SelectorManifest {
            version: 1,
            issued_at_unix_seconds: 0,
            client_min_version: "0.0.1".to_string(),
            selectors: a,
        };
        let m_b = SelectorManifest {
            version: 1,
            issued_at_unix_seconds: 0,
            client_min_version: "0.0.1".to_string(),
            selectors: b,
        };
        assert_eq!(canonical_manifest_bytes(&m_a), canonical_manifest_bytes(&m_b));
    }

    #[test]
    fn parse_envelope_round_trip() {
        let (s, p, _) = fresh_signer();
        let m = sample(1_700_000_000);
        let signed = sign_manifest(&s, &p, &m);
        let json = serde_json::to_vec(&signed).unwrap();
        let parsed = parse_signed_manifest(&json).unwrap();
        assert_eq!(parsed, signed);
    }

    #[test]
    fn empty_selectors_map_round_trips() {
        let (s, p, pub_b64) = fresh_signer();
        let m = SelectorManifest {
            version: 1,
            issued_at_unix_seconds: 1_700_000_000,
            client_min_version: "0.1.0".to_string(),
            selectors: BTreeMap::new(),
        };
        let signed = sign_manifest(&s, &p, &m);
        assert_eq!(verify_manifest(&signed, &pub_b64, 1_700_000_000).unwrap(), m);
    }
}
