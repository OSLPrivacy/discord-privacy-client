//! Opaque cipher-store pointer and its server-facing capabilities.
//!
//! A carrier contains only [`Pointer`].  The capabilities are independently
//! derived from the roots that authorize each operation, so a recipient that
//! has the pointer cannot derive the sender's manage capability.

use crate::{hkdf, Error as CryptoError, Result};
use thiserror::Error;

/// Size of the opaque carrier pointer: 160 bits.
pub const POINTER_BYTES: usize = 20;
/// Size of every cipher-store identifier and capability digest.
pub const CAPABILITY_BYTES: usize = 16;
/// Size of the per-message conversation key used for one delivery tag.
pub const CONVERSATION_MESSAGE_KEY_BYTES: usize = 32;
/// Receiver-side delivery-tag look-ahead width.
pub const DELIVERY_TAG_LOOKAHEAD: usize = 32;

const BLOB_ID_INFO: &[u8] = b"osl/ptr/id/v1";
const FETCH_CAP_INFO: &[u8] = b"osl/ptr/fetch/v1";
const ACK_CAP_LABEL: &[u8] = b"osl/ptr/ack/v1";
const MANAGE_CAP_LABEL: &[u8] = b"osl/ptr/manage/v1";
const DELIVERY_TAG_INFO: &[u8] = b"osl/tag/v1";
const CONVERSATION_MESSAGE_KEY_INFO: &[u8] = b"osl/tag/k-conv-n/v1";

/// The 160-bit unlinkable value transported in a protected-message carrier.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct Pointer([u8; POINTER_BYTES]);

impl Pointer {
    /// Creates a pointer from its exact wire representation.
    pub const fn from_bytes(bytes: [u8; POINTER_BYTES]) -> Self {
        Self(bytes)
    }

    /// Returns the exact bytes carried on the wire.
    pub const fn as_bytes(&self) -> &[u8; POINTER_BYTES] {
        &self.0
    }
}

impl TryFrom<&[u8]> for Pointer {
    type Error = core::array::TryFromSliceError;

    fn try_from(bytes: &[u8]) -> core::result::Result<Self, Self::Error> {
        Ok(Self(bytes.try_into()?))
    }
}

/// The two values a bare pointer authorizes on its own.
///
/// A recipient holding only `P` can address the object and fetch it, and can
/// derive nothing else: acknowledgement and management authority are rooted in
/// keys the pointer does not contain.
pub struct PointerFetchAuthority {
    pub blob_id: [u8; CAPABILITY_BYTES],
    pub fetch_cap: [u8; CAPABILITY_BYTES],
}

/// Cipher-store values derived for one pointer delivery.
pub struct PointerCapabilities {
    pub blob_id: [u8; CAPABILITY_BYTES],
    pub fetch_cap: [u8; CAPABILITY_BYTES],
    pub ack_cap: [u8; CAPABILITY_BYTES],
    pub manage_cap: [u8; CAPABILITY_BYTES],
    pub delivery_tag: [u8; CAPABILITY_BYTES],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DeliveryTagMatch {
    pub counter: u32,
    pub delivery_tag: [u8; CAPABILITY_BYTES],
}

#[derive(Debug, Error)]
pub enum DeliveryTagResolveError {
    #[error("delivery tag derivation failed: {0}")]
    Derive(#[from] CryptoError),
    #[error(
        "delivery tag outside {window}-tag look-ahead window starting at counter {start_counter}"
    )]
    LookaheadMiss { start_counter: u32, window: usize },
}

/// Derives all server-facing values for a pointer delivery.
///
/// HKDF-SHA256 uses an empty salt to match the cipher-store Worker.  ACK and
/// manage capabilities are rooted in separate message/send keys and bind the
/// derived blob id into their HKDF info, preventing either authority from
/// being recovered from the public pointer alone. The delivery tag is rooted
/// in the per-message `K_conv_n`; callers that hold only the conversation
/// epoch root must first call [`derive_conversation_message_key`].
pub fn derive_capabilities(
    pointer: &Pointer,
    message_key: &[u8],
    send_key: &[u8],
    conversation_message_key: &[u8],
) -> Result<PointerCapabilities> {
    let PointerFetchAuthority { blob_id, fetch_cap } = derive_fetch_authority(pointer)?;
    let ack_cap = derive_ack_capability(message_key, &blob_id)?;
    let manage_cap = derive_manage_capability(send_key, &blob_id)?;
    let delivery_tag = derive_delivery_tag(conversation_message_key)?;

    Ok(PointerCapabilities {
        blob_id,
        fetch_cap,
        ack_cap,
        manage_cap,
        delivery_tag,
    })
}

/// Derives `K_conv_n` from the current conversation epoch and an explicit
/// message counter.
pub fn derive_conversation_message_key(
    conversation_epoch_secret: &[u8],
    counter: u32,
) -> Result<[u8; CONVERSATION_MESSAGE_KEY_BYTES]> {
    let mut info = Vec::with_capacity(CONVERSATION_MESSAGE_KEY_INFO.len() + 4);
    info.extend_from_slice(CONVERSATION_MESSAGE_KEY_INFO);
    info.extend_from_slice(&counter.to_be_bytes());
    let output = hkdf::derive(
        &[],
        conversation_epoch_secret,
        &info,
        CONVERSATION_MESSAGE_KEY_BYTES,
    )?;
    Ok(output
        .try_into()
        .expect("fixed HKDF output length must fit message-key array"))
}

/// Derives the store-visible delivery tag from one per-message `K_conv_n`.
pub fn derive_delivery_tag(conversation_message_key: &[u8]) -> Result<[u8; CAPABILITY_BYTES]> {
    derive_128(conversation_message_key, DELIVERY_TAG_INFO)
}

/// Convenience wrapper for the sender's current counter.
pub fn derive_delivery_tag_for_counter(
    conversation_epoch_secret: &[u8],
    counter: u32,
) -> Result<[u8; CAPABILITY_BYTES]> {
    let conversation_message_key =
        derive_conversation_message_key(conversation_epoch_secret, counter)?;
    derive_delivery_tag(&conversation_message_key)
}

/// Build the receiver's clean 32-tag look-ahead window.
pub fn derive_delivery_tag_window(
    conversation_epoch_secret: &[u8],
    start_counter: u32,
) -> Result<Vec<DeliveryTagMatch>> {
    let mut tags = Vec::with_capacity(DELIVERY_TAG_LOOKAHEAD);
    for offset in 0..DELIVERY_TAG_LOOKAHEAD {
        let Some(counter) = start_counter.checked_add(offset as u32) else {
            break;
        };
        tags.push(DeliveryTagMatch {
            counter,
            delivery_tag: derive_delivery_tag_for_counter(conversation_epoch_secret, counter)?,
        });
    }
    Ok(tags)
}

/// Resolve a store wakeup tag without mutating the caller's counter.
///
/// A miss past the look-ahead window is named and detectable; callers must not
/// advance their receive counter unless this returns `Ok`.
pub fn resolve_delivery_tag(
    conversation_epoch_secret: &[u8],
    start_counter: u32,
    observed: &[u8; CAPABILITY_BYTES],
) -> core::result::Result<DeliveryTagMatch, DeliveryTagResolveError> {
    for candidate in derive_delivery_tag_window(conversation_epoch_secret, start_counter)? {
        if &candidate.delivery_tag == observed {
            return Ok(candidate);
        }
    }
    Err(DeliveryTagResolveError::LookaheadMiss {
        start_counter,
        window: DELIVERY_TAG_LOOKAHEAD,
    })
}

/// Derives exactly what a bare pointer authorizes: the object's id and the
/// capability that reads it.
///
/// The receiving side has only `P`, so this is the whole of what it can
/// compute.  It is the same derivation [`derive_capabilities`] performs — one
/// definition, so a sender and a receiver can never drift apart.
pub fn derive_fetch_authority(pointer: &Pointer) -> Result<PointerFetchAuthority> {
    Ok(PointerFetchAuthority {
        blob_id: derive_128(pointer.as_bytes(), BLOB_ID_INFO)?,
        fetch_cap: derive_128(pointer.as_bytes(), FETCH_CAP_INFO)?,
    })
}

/// Derives the receipt authority for one already-known object id.
pub fn derive_ack_capability(
    message_key: &[u8],
    blob_id: &[u8; CAPABILITY_BYTES],
) -> Result<[u8; CAPABILITY_BYTES]> {
    derive_bound_128(message_key, ACK_CAP_LABEL, blob_id)
}

/// Derives the burn authority for one already-known object id.
///
/// A burn happens long after the send that created the object, by which time
/// the pointer is gone; the sender keeps only the id it recorded.  Because the
/// id is bound into the HKDF info rather than being the key, that recorded id
/// plus the sender's own send key is enough, and remains insufficient for
/// anybody else.
pub fn derive_manage_capability(
    send_key: &[u8],
    blob_id: &[u8; CAPABILITY_BYTES],
) -> Result<[u8; CAPABILITY_BYTES]> {
    derive_bound_128(send_key, MANAGE_CAP_LABEL, blob_id)
}

fn derive_128(key_material: &[u8], info: &[u8]) -> Result<[u8; CAPABILITY_BYTES]> {
    let output = hkdf::derive(&[], key_material, info, CAPABILITY_BYTES)?;
    Ok(output
        .try_into()
        .expect("fixed HKDF output length must fit capability array"))
}

fn derive_bound_128(
    key_material: &[u8],
    label: &[u8],
    blob_id: &[u8; CAPABILITY_BYTES],
) -> Result<[u8; CAPABILITY_BYTES]> {
    let mut info = Vec::with_capacity(label.len() + blob_id.len());
    info.extend_from_slice(label);
    info.extend_from_slice(blob_id);
    derive_128(key_material, &info)
}
