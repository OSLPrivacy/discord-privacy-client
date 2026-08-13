//! TASK 5402 — the two at-rest envelopes every recoverable secret must use.
//!
//! Before this module three shipping writers put long-term secret material on
//! disk as raw bytes:
//!
//! - [`crate::account_recovery::RecoveryKit`] wrote the independent recovery
//!   authority's Ed25519 seed with `fs::write`,
//! - [`crate::lost_device_recovery::LostDeviceRecoveryKit`] wrote its own
//!   authority seed with `File::write_all`,
//! - [`crate::identity::DevicePrivateKeys`] wrote the 64 raw bytes of a
//!   device's X25519 + Ed25519 secrets to `device-private-keys.bin`.
//!
//! All three are named secret classes in TASK 5402 (recovery authorities,
//! recovery packages/kits, restored identity private material) and all three
//! are recoverable — the file IS the secret, so "hash it" is not available.
//! They therefore have to be authenticated ciphertext whose unlock key is
//! user-derived or OS/hardware-bound, and never persisted in plaintext beside
//! the ciphertext.
//!
//! Two envelopes, because the two cases genuinely differ:
//!
//! - [`seal_with_passphrase`] — Argon2id(passphrase) → XChaCha20-Poly1305.
//!   For artifacts that must survive the device: a recovery kit exists exactly
//!   for the case where every device is gone, so binding it to the device
//!   sealer would make it useless at the only moment it is needed. The unlock
//!   key is user-derived and no part of it is stored in the file — only the
//!   salt and the memory-hard parameters, which are public by construction.
//! - [`seal_with_device_sealer`] — the platform [`Sealer`] (TPM → OS
//!   credential store → encrypted process-ephemeral). For artifacts that are
//!   meaningful only on this device, such as a replacement profile's own
//!   device private keys.
//!
//! Both layouts are self-describing and fully authenticated: the magic, the
//! version, the KDF parameters, the salt and the caller's domain string are all
//! AEAD associated data, so flipping any of them fails the tag rather than
//! silently selecting different key material.

use crate::sealer::Sealer;
use argon2::{Algorithm, Argon2, Params, Version};
use crypto::{aead, random};
use std::fmt;
use zeroize::Zeroizing;

/// `seal_with_passphrase` output prefix.
pub const PASSPHRASE_ENVELOPE_MAGIC: &[u8; 8] = b"OSLKITE1";
/// `seal_with_device_sealer` output prefix.
pub const DEVICE_ENVELOPE_MAGIC: &[u8; 8] = b"OSLDEVE1";
pub const ENVELOPE_VERSION: u8 = 1;

/// Minimum passphrase for a portable kit envelope.
///
/// A kit passphrase is not the six-character unlock PIN: the unlock PIN is
/// rate-limited by a live process that can lock out, and a kit file that has
/// left the device has no rate limit at all. Argon2id at 64 MiB is the only
/// brake, so the floor here is higher than [`crate::MIN_PASSWORD_LENGTH`].
pub const MIN_KIT_PASSPHRASE_LEN: usize = 12;

/// Argon2id floor — identical to the production unlock parameters in
/// [`crate::password::Argon2Params::production`]. Do not lower `m_cost`; that
/// is the GPU-resistance property.
pub const ARGON_M_COST: u32 = 65_536;
pub const ARGON_T_COST: u32 = 3;
pub const ARGON_P_COST: u32 = 1;

const SALT_LEN: usize = 16;
/// magic(8) + version(1) + m_cost(4) + t_cost(4) + p_cost(4) + salt(16)
const PASSPHRASE_HEADER_LEN: usize = 8 + 1 + 4 + 4 + 4 + SALT_LEN;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SecretEnvelopeError {
    PassphraseTooShort { got: usize, min: usize },
    Malformed(String),
    Kdf(String),
    Crypto(String),
    Sealer(String),
}

impl fmt::Display for SecretEnvelopeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PassphraseTooShort { got, min } => write!(
                f,
                "recovery-kit passphrase too short: {got} characters, minimum {min}"
            ),
            Self::Malformed(detail) => write!(f, "at-rest envelope malformed: {detail}"),
            Self::Kdf(detail) => write!(f, "at-rest envelope key derivation failed: {detail}"),
            Self::Crypto(detail) => write!(f, "at-rest envelope AEAD failed: {detail}"),
            Self::Sealer(detail) => write!(f, "at-rest envelope device sealer failed: {detail}"),
        }
    }
}

impl std::error::Error for SecretEnvelopeError {}

pub type Result<T> = core::result::Result<T, SecretEnvelopeError>;

/// Which envelope, if any, these bytes carry. The independent at-rest
/// inventory uses this to classify a file without holding any key.
pub fn envelope_kind(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(PASSPHRASE_ENVELOPE_MAGIC) {
        return Some("passphrase-derived-aead");
    }
    if bytes.starts_with(DEVICE_ENVELOPE_MAGIC) {
        return Some("device-sealed-aead");
    }
    None
}

pub fn validate_kit_passphrase(passphrase: &str) -> Result<()> {
    let got = passphrase.chars().count();
    if got < MIN_KIT_PASSPHRASE_LEN {
        return Err(SecretEnvelopeError::PassphraseTooShort {
            got,
            min: MIN_KIT_PASSPHRASE_LEN,
        });
    }
    Ok(())
}

fn argon_key(
    passphrase: &str,
    salt: &[u8],
    m_cost: u32,
    t_cost: u32,
    p_cost: u32,
) -> Result<Zeroizing<[u8; aead::KEY_SIZE]>> {
    let params = Params::new(m_cost, t_cost, p_cost, Some(aead::KEY_SIZE))
        .map_err(|e| SecretEnvelopeError::Kdf(format!("params: {e}")))?;
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut out = Zeroizing::new([0u8; aead::KEY_SIZE]);
    argon
        .hash_password_into(passphrase.as_bytes(), salt, &mut out[..])
        .map_err(|e| SecretEnvelopeError::Kdf(format!("hash_password_into: {e}")))?;
    Ok(out)
}

fn passphrase_header(salt: &[u8; SALT_LEN]) -> Vec<u8> {
    let mut header = Vec::with_capacity(PASSPHRASE_HEADER_LEN);
    header.extend_from_slice(PASSPHRASE_ENVELOPE_MAGIC);
    header.push(ENVELOPE_VERSION);
    header.extend_from_slice(&ARGON_M_COST.to_be_bytes());
    header.extend_from_slice(&ARGON_T_COST.to_be_bytes());
    header.extend_from_slice(&ARGON_P_COST.to_be_bytes());
    header.extend_from_slice(salt);
    header
}

fn aad(header: &[u8], domain: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(header.len() + 8 + domain.len());
    out.extend_from_slice(header);
    out.extend_from_slice(&(domain.len() as u64).to_be_bytes());
    out.extend_from_slice(domain);
    out
}

/// Seal `plaintext` under a key derived from `passphrase` alone.
///
/// Layout: `magic(8) || version(1) || m_cost(4) || t_cost(4) || p_cost(4) ||
/// salt(16) || nonce(24) || ciphertext||tag`. Everything before the nonce, plus
/// `domain`, is AEAD associated data.
pub fn seal_with_passphrase(domain: &[u8], passphrase: &str, plaintext: &[u8]) -> Result<Vec<u8>> {
    validate_kit_passphrase(passphrase)?;
    let mut salt = [0u8; SALT_LEN];
    salt.copy_from_slice(&random::random_bytes(SALT_LEN));
    let header = passphrase_header(&salt);
    let key = argon_key(passphrase, &salt, ARGON_M_COST, ARGON_T_COST, ARGON_P_COST)?;
    let key = aead::Key::from_bytes(*key);
    let nonce = random::random_nonce();
    let ciphertext = aead::seal(&key, &nonce, &aad(&header, domain), plaintext)
        .map_err(|e| SecretEnvelopeError::Crypto(e.to_string()))?;
    let mut out = Vec::with_capacity(header.len() + aead::NONCE_SIZE + ciphertext.len());
    out.extend_from_slice(&header);
    out.extend_from_slice(nonce.as_bytes());
    out.extend_from_slice(&ciphertext);
    Ok(out)
}

/// Inverse of [`seal_with_passphrase`]. A wrong passphrase, a wrong domain, or
/// a single flipped byte anywhere in the file fails the AEAD tag and returns
/// zero plaintext bytes.
pub fn open_with_passphrase(
    domain: &[u8],
    passphrase: &str,
    bytes: &[u8],
) -> Result<Zeroizing<Vec<u8>>> {
    if bytes.len() < PASSPHRASE_HEADER_LEN + aead::NONCE_SIZE + aead::TAG_SIZE {
        return Err(SecretEnvelopeError::Malformed(
            "passphrase envelope shorter than its own header".into(),
        ));
    }
    if !bytes.starts_with(PASSPHRASE_ENVELOPE_MAGIC) {
        return Err(SecretEnvelopeError::Malformed(
            "passphrase envelope magic missing".into(),
        ));
    }
    if bytes[8] != ENVELOPE_VERSION {
        return Err(SecretEnvelopeError::Malformed(format!(
            "passphrase envelope version {} != {ENVELOPE_VERSION}",
            bytes[8]
        )));
    }
    let m_cost = u32::from_be_bytes(bytes[9..13].try_into().expect("4 bytes"));
    let t_cost = u32::from_be_bytes(bytes[13..17].try_into().expect("4 bytes"));
    let p_cost = u32::from_be_bytes(bytes[17..21].try_into().expect("4 bytes"));
    if m_cost < ARGON_M_COST || t_cost < ARGON_T_COST {
        return Err(SecretEnvelopeError::Malformed(format!(
            "passphrase envelope declares weakened Argon2id parameters \
             (m_cost={m_cost}, t_cost={t_cost}); floor is m_cost={ARGON_M_COST}, \
             t_cost={ARGON_T_COST}"
        )));
    }
    let mut salt = [0u8; SALT_LEN];
    salt.copy_from_slice(&bytes[21..PASSPHRASE_HEADER_LEN]);
    let header = &bytes[..PASSPHRASE_HEADER_LEN];
    let mut nonce_bytes = [0u8; aead::NONCE_SIZE];
    nonce_bytes
        .copy_from_slice(&bytes[PASSPHRASE_HEADER_LEN..PASSPHRASE_HEADER_LEN + aead::NONCE_SIZE]);
    let key = argon_key(passphrase, &salt, m_cost, t_cost, p_cost)?;
    let key = aead::Key::from_bytes(*key);
    let plaintext = aead::open(
        &key,
        &aead::Nonce::from_bytes(nonce_bytes),
        &aad(header, domain),
        &bytes[PASSPHRASE_HEADER_LEN + aead::NONCE_SIZE..],
    )
    .map_err(|e| SecretEnvelopeError::Crypto(e.to_string()))?;
    Ok(Zeroizing::new(plaintext))
}

fn device_header(method: &str) -> Vec<u8> {
    let mut header = Vec::with_capacity(8 + 1 + 2 + method.len());
    header.extend_from_slice(DEVICE_ENVELOPE_MAGIC);
    header.push(ENVELOPE_VERSION);
    header.extend_from_slice(&(method.len() as u16).to_be_bytes());
    header.extend_from_slice(method.as_bytes());
    header
}

/// Seal `plaintext` under the platform device sealer (TPM → OS credential
/// store → encrypted process-ephemeral).
///
/// The sealer's own output is already authenticated; this adds the method tag
/// and `domain` so a blob sealed for one purpose cannot be replayed as another,
/// and so an operator can read the method without holding any key.
pub fn seal_with_device_sealer(
    domain: &[u8],
    sealer: &dyn Sealer,
    plaintext: &[u8],
) -> Result<Vec<u8>> {
    let header = device_header(sealer.method_label());
    let mut framed = Vec::with_capacity(8 + domain.len() + plaintext.len());
    framed.extend_from_slice(&(domain.len() as u64).to_be_bytes());
    framed.extend_from_slice(domain);
    framed.extend_from_slice(plaintext);
    let sealed = sealer
        .seal(&framed)
        .map_err(|e| SecretEnvelopeError::Sealer(e.to_string()))?;
    let mut out = Vec::with_capacity(header.len() + sealed.len());
    out.extend_from_slice(&header);
    out.extend_from_slice(&sealed);
    Ok(out)
}

/// Inverse of [`seal_with_device_sealer`]. A blob sealed by another OS
/// account's credential store, or by another device, fails here and returns
/// zero plaintext bytes.
pub fn open_with_device_sealer(
    domain: &[u8],
    sealer: &dyn Sealer,
    bytes: &[u8],
) -> Result<Zeroizing<Vec<u8>>> {
    if !bytes.starts_with(DEVICE_ENVELOPE_MAGIC) {
        return Err(SecretEnvelopeError::Malformed(
            "device envelope magic missing".into(),
        ));
    }
    if bytes.len() < 11 || bytes[8] != ENVELOPE_VERSION {
        return Err(SecretEnvelopeError::Malformed(
            "device envelope version unsupported".into(),
        ));
    }
    let method_len = u16::from_be_bytes(bytes[9..11].try_into().expect("2 bytes")) as usize;
    if bytes.len() < 11 + method_len {
        return Err(SecretEnvelopeError::Malformed(
            "device envelope method tag truncated".into(),
        ));
    }
    let method = std::str::from_utf8(&bytes[11..11 + method_len])
        .map_err(|_| SecretEnvelopeError::Malformed("device envelope method tag not utf-8".into()))?
        .to_owned();
    if method != sealer.method_label() {
        return Err(SecretEnvelopeError::Malformed(format!(
            "device envelope was sealed by {method}, this device offers {}",
            sealer.method_label()
        )));
    }
    let opened = sealer
        .unseal(&bytes[11 + method_len..])
        .map_err(|e| SecretEnvelopeError::Sealer(e.to_string()))?;
    if opened.len() < 8 {
        return Err(SecretEnvelopeError::Malformed(
            "device envelope inner frame truncated".into(),
        ));
    }
    let declared = u64::from_be_bytes(opened[..8].try_into().expect("8 bytes")) as usize;
    if opened.len() < 8 + declared || &opened[8..8 + declared] != domain {
        return Err(SecretEnvelopeError::Malformed(
            "device envelope domain mismatch".into(),
        ));
    }
    Ok(Zeroizing::new(opened[8 + declared..].to_vec()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOMAIN: &[u8] = b"OSL/test/secret-at-rest/v1";
    const PASSPHRASE: &str = "kit-passphrase-5402-abcdef";

    #[test]
    fn passphrase_envelope_round_trips_and_hides_the_plaintext() {
        let secret = [0x5au8; 32];
        let sealed = seal_with_passphrase(DOMAIN, PASSPHRASE, &secret).expect("seal");
        assert_eq!(envelope_kind(&sealed), Some("passphrase-derived-aead"));
        assert!(
            sealed.windows(32).all(|w| w != secret),
            "sealed kit must not contain its own plaintext secret"
        );
        let opened = open_with_passphrase(DOMAIN, PASSPHRASE, &sealed).expect("open");
        assert_eq!(&opened[..], &secret[..]);
    }

    #[test]
    fn passphrase_envelope_refuses_wrong_passphrase_domain_and_tampering() {
        let secret = [0x11u8; 48];
        let sealed = seal_with_passphrase(DOMAIN, PASSPHRASE, &secret).expect("seal");
        assert!(open_with_passphrase(DOMAIN, "kit-passphrase-5402-abcdeg", &sealed).is_err());
        assert!(open_with_passphrase(b"other/domain/v1", PASSPHRASE, &sealed).is_err());
        for index in [0usize, 9, 21, PASSPHRASE_HEADER_LEN, sealed.len() - 1] {
            let mut tampered = sealed.clone();
            tampered[index] ^= 0x01;
            assert!(
                open_with_passphrase(DOMAIN, PASSPHRASE, &tampered).is_err(),
                "flipping byte {index} must fail the envelope"
            );
        }
    }

    #[test]
    fn passphrase_envelope_refuses_weakened_parameters() {
        let sealed = seal_with_passphrase(DOMAIN, PASSPHRASE, b"secret").expect("seal");
        let mut weakened = sealed.clone();
        weakened[9..13].copy_from_slice(&8u32.to_be_bytes());
        let error = open_with_passphrase(DOMAIN, PASSPHRASE, &weakened).unwrap_err();
        assert!(matches!(error, SecretEnvelopeError::Malformed(_)), "{error}");
    }

    #[test]
    fn passphrase_envelope_enforces_a_minimum_passphrase() {
        assert_eq!(
            seal_with_passphrase(DOMAIN, "short", b"x").unwrap_err(),
            SecretEnvelopeError::PassphraseTooShort {
                got: 5,
                min: MIN_KIT_PASSPHRASE_LEN
            }
        );
    }

    #[test]
    fn device_envelope_round_trips_and_refuses_another_credential_store() {
        let secret = [0x77u8; 64];
        let sealer = crate::sealer::MemorySealer::new();
        let sealed = seal_with_device_sealer(DOMAIN, &sealer, &secret).expect("seal");
        assert_eq!(envelope_kind(&sealed), Some("device-sealed-aead"));
        assert!(sealed.windows(64).all(|w| w != secret));
        let opened = open_with_device_sealer(DOMAIN, &sealer, &sealed).expect("open");
        assert_eq!(&opened[..], &secret[..]);

        let other_account = crate::sealer::MemorySealer::new();
        assert!(
            open_with_device_sealer(DOMAIN, &other_account, &sealed).is_err(),
            "an independent credential store must release zero bytes"
        );
        assert!(open_with_device_sealer(b"other/domain/v1", &sealer, &sealed).is_err());
    }
}
