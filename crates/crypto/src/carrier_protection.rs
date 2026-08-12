//! Authenticated content protection for shipping carrier cells.
//!
//! This module is deliberately narrower than the general wire formats.  A
//! caller supplies the complete carrier context and a 256-bit endpoint secret;
//! every object gets a fresh random salt, a fresh XChaCha nonce, a distinct
//! HKDF-SHA-256 content key, and one canonical AAD tuple.  The only root-secret
//! constructor uses the crate's OS CSPRNG.  There is no password, raw-byte,
//! reduced-key, global-content-key, custom-cipher, or downgrade constructor.

use crate::{aead, hkdf, random, Error, Result};
use std::collections::HashSet;
use zeroize::ZeroizeOnDrop;

pub const PROTOCOL: &str = "OSL-SHIPPING-CARRIER";
pub const VERSION: &str = "1";
pub const ALGORITHM: &str = "XChaCha20-Poly1305-IETF";
pub const PRIMITIVE_LIBRARY: &str = "RustCrypto chacha20poly1305 0.10.1";
pub const ROOT_PROVENANCE: &str = "rand::rngs::OsRng/getrandom";
pub const ROOT_BITS: usize = 256;
pub const NONCE_BITS: usize = 192;
pub const HKDF_ALGORITHM: &str = "HKDF-SHA-256";
pub const HKDF_DOMAIN: &[u8] = b"OSL/shipping-carrier/content-key/v1";

/// A CSPRNG-created root secret.  It cannot be constructed from caller bytes.
#[derive(Clone, ZeroizeOnDrop)]
pub struct CarrierRootSecret([u8; 32]);

impl CarrierRootSecret {
    pub fn generate() -> Self {
        let generated = random::random_aead_key();
        let mut root = [0u8; 32];
        root.copy_from_slice(generated.as_bytes());
        Self(root)
    }
}

/// The authenticated context required before any recipient rendering.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CarrierContext {
    pub protocol: String,
    pub version: String,
    pub sender_account: String,
    pub sender_device: String,
    pub recipient_account: String,
    pub recipient_device: String,
    pub conversation: String,
    pub adapter_implementation_id: String,
    pub send_constructor: String,
    pub content_type: String,
    pub object_id: String,
    /// Authenticated ordering fact used by [`DeliveryGuard`].
    pub delivery_sequence: u64,
}

impl CarrierContext {
    pub fn shipping_v1(
        sender_account: impl Into<String>,
        sender_device: impl Into<String>,
        recipient_account: impl Into<String>,
        recipient_device: impl Into<String>,
        conversation: impl Into<String>,
        adapter_implementation_id: impl Into<String>,
        send_constructor: impl Into<String>,
        content_type: impl Into<String>,
        object_id: impl Into<String>,
        delivery_sequence: u64,
    ) -> Self {
        Self {
            protocol: PROTOCOL.to_owned(),
            version: VERSION.to_owned(),
            sender_account: sender_account.into(),
            sender_device: sender_device.into(),
            recipient_account: recipient_account.into(),
            recipient_device: recipient_device.into(),
            conversation: conversation.into(),
            adapter_implementation_id: adapter_implementation_id.into(),
            send_constructor: send_constructor.into(),
            content_type: content_type.into(),
            object_id: object_id.into(),
            delivery_sequence,
        }
    }
}

/// Public carrier bytes.  The root and derived content key are never members.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CarrierEnvelope {
    pub algorithm: String,
    pub protocol: String,
    pub version: String,
    pub hkdf_salt: [u8; 32],
    pub nonce: [u8; aead::NONCE_SIZE],
    pub ciphertext_and_tag: Vec<u8>,
}

impl CarrierEnvelope {
    /// Stable byte representation used by carrier-capture checks.
    pub fn carrier_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        append_field(&mut out, b"algorithm", self.algorithm.as_bytes());
        append_field(&mut out, b"protocol", self.protocol.as_bytes());
        append_field(&mut out, b"version", self.version.as_bytes());
        append_field(&mut out, b"hkdf-salt", &self.hkdf_salt);
        append_field(&mut out, b"nonce", &self.nonce);
        append_field(&mut out, b"ciphertext-and-tag", &self.ciphertext_and_tag);
        out
    }
}

fn append_field(out: &mut Vec<u8>, label: &[u8], value: &[u8]) {
    out.extend_from_slice(&(label.len() as u32).to_be_bytes());
    out.extend_from_slice(label);
    out.extend_from_slice(&(value.len() as u64).to_be_bytes());
    out.extend_from_slice(value);
}

fn validate_context(context: &CarrierContext) -> Result<()> {
    let fields = [
        context.protocol.as_str(),
        context.version.as_str(),
        context.sender_account.as_str(),
        context.sender_device.as_str(),
        context.recipient_account.as_str(),
        context.recipient_device.as_str(),
        context.conversation.as_str(),
        context.adapter_implementation_id.as_str(),
        context.send_constructor.as_str(),
        context.content_type.as_str(),
        context.object_id.as_str(),
    ];
    if context.protocol != PROTOCOL
        || context.version != VERSION
        || context.delivery_sequence == 0
        || fields.iter().any(|value| value.is_empty())
    {
        return Err(Error::AeadFailure);
    }
    Ok(())
}

fn associated_data(context: &CarrierContext) -> Result<Vec<u8>> {
    validate_context(context)?;
    let mut aad = Vec::new();
    for (label, value) in [
        ("protocol", context.protocol.as_str()),
        ("version", context.version.as_str()),
        ("sender-account", context.sender_account.as_str()),
        ("sender-device", context.sender_device.as_str()),
        ("recipient-account", context.recipient_account.as_str()),
        ("recipient-device", context.recipient_device.as_str()),
        ("conversation-channel", context.conversation.as_str()),
        (
            "adapter-implementation-id",
            context.adapter_implementation_id.as_str(),
        ),
        ("send-constructor", context.send_constructor.as_str()),
        ("content-type", context.content_type.as_str()),
        ("message-object-id", context.object_id.as_str()),
    ] {
        append_field(&mut aad, label.as_bytes(), value.as_bytes());
    }
    append_field(
        &mut aad,
        b"delivery-sequence",
        &context.delivery_sequence.to_be_bytes(),
    );
    Ok(aad)
}

fn content_key(
    root: &CarrierRootSecret,
    salt: &[u8; 32],
    context: &CarrierContext,
) -> Result<aead::Key> {
    let mut info = HKDF_DOMAIN.to_vec();
    append_field(
        &mut info,
        b"adapter-implementation-id",
        context.adapter_implementation_id.as_bytes(),
    );
    append_field(
        &mut info,
        b"send-constructor",
        context.send_constructor.as_bytes(),
    );
    append_field(&mut info, b"content-type", context.content_type.as_bytes());
    Ok(aead::Key::from_bytes(hkdf::derive_32(
        salt, &root.0, &info,
    )?))
}

/// Protect one carrier-cell object with a fresh nonce and separated content key.
pub fn seal(
    root: &CarrierRootSecret,
    context: &CarrierContext,
    plaintext: &[u8],
) -> Result<CarrierEnvelope> {
    validate_context(context)?;
    if plaintext.is_empty() {
        return Err(Error::AeadFailure);
    }
    let salt_bytes = random::random_bytes(32);
    let mut hkdf_salt = [0u8; 32];
    hkdf_salt.copy_from_slice(&salt_bytes);
    let nonce = random::random_nonce();
    let key = content_key(root, &hkdf_salt, context)?;
    let aad = associated_data(context)?;
    let ciphertext_and_tag = aead::seal(&key, &nonce, &aad, plaintext)?;
    Ok(CarrierEnvelope {
        algorithm: ALGORITHM.to_owned(),
        protocol: PROTOCOL.to_owned(),
        version: VERSION.to_owned(),
        hkdf_salt,
        nonce: *nonce.as_bytes(),
        ciphertext_and_tag,
    })
}

/// Authenticate all public metadata and context before returning plaintext.
pub fn open(
    root: &CarrierRootSecret,
    context: &CarrierContext,
    envelope: &CarrierEnvelope,
) -> Result<Vec<u8>> {
    validate_context(context)?;
    if envelope.algorithm != ALGORITHM
        || envelope.protocol != PROTOCOL
        || envelope.version != VERSION
        || envelope.protocol != context.protocol
        || envelope.version != context.version
    {
        return Err(Error::AeadFailure);
    }
    let key = content_key(root, &envelope.hkdf_salt, context)?;
    let aad = associated_data(context)?;
    aead::open(
        &key,
        &aead::Nonce::from_bytes(envelope.nonce),
        &aad,
        &envelope.ciphertext_and_tag,
    )
}

/// Recipient-side exactly-once and in-order admission barrier.
#[derive(Debug)]
pub struct DeliveryGuard {
    next_sequence: u64,
    delivered_object_ids: HashSet<String>,
    delivered: usize,
}

impl Default for DeliveryGuard {
    fn default() -> Self {
        Self {
            next_sequence: 1,
            delivered_object_ids: HashSet::new(),
            delivered: 0,
        }
    }
}

impl DeliveryGuard {
    pub fn open_once(
        &mut self,
        root: &CarrierRootSecret,
        context: &CarrierContext,
        envelope: &CarrierEnvelope,
    ) -> Result<Vec<u8>> {
        if context.delivery_sequence != self.next_sequence
            || self.delivered_object_ids.contains(&context.object_id)
        {
            return Err(Error::AeadFailure);
        }
        let plaintext = open(root, context, envelope)?;
        self.delivered_object_ids.insert(context.object_id.clone());
        self.next_sequence = self
            .next_sequence
            .checked_add(1)
            .ok_or(Error::AeadFailure)?;
        self.delivered += 1;
        Ok(plaintext)
    }

    pub fn delivered(&self) -> usize {
        self.delivered
    }
}
