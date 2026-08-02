//! Opaque cipher-store pointer and its server-facing capabilities.
//!
//! A carrier contains only [`Pointer`].  The capabilities are independently
//! derived from the roots that authorize each operation, so a recipient that
//! has the pointer cannot derive the sender's manage capability.

use crate::{hkdf, Result};

/// Size of the opaque carrier pointer: 160 bits.
pub const POINTER_BYTES: usize = 20;
/// Size of every cipher-store identifier and capability digest.
pub const CAPABILITY_BYTES: usize = 16;

const BLOB_ID_INFO: &[u8] = b"osl/ptr/id/v1";
const FETCH_CAP_INFO: &[u8] = b"osl/ptr/fetch/v1";
const ACK_CAP_LABEL: &[u8] = b"osl/ptr/ack/v1";
const MANAGE_CAP_LABEL: &[u8] = b"osl/ptr/manage/v1";
const DELIVERY_TAG_INFO: &[u8] = b"osl/tag/v1";

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

/// Cipher-store values derived for one pointer delivery.
pub struct PointerCapabilities {
    pub blob_id: [u8; CAPABILITY_BYTES],
    pub fetch_cap: [u8; CAPABILITY_BYTES],
    pub ack_cap: [u8; CAPABILITY_BYTES],
    pub manage_cap: [u8; CAPABILITY_BYTES],
    pub delivery_tag: [u8; CAPABILITY_BYTES],
}

/// Derives all server-facing values for a pointer delivery.
///
/// HKDF-SHA256 uses an empty salt to match the cipher-store Worker.  ACK and
/// manage capabilities are rooted in separate message/send keys and bind the
/// derived blob id into their HKDF info, preventing either authority from
/// being recovered from the public pointer alone.
pub fn derive_capabilities(
    pointer: &Pointer,
    message_key: &[u8],
    send_key: &[u8],
    conversation_key: &[u8],
) -> Result<PointerCapabilities> {
    let blob_id = derive_128(pointer.as_bytes(), BLOB_ID_INFO)?;
    let fetch_cap = derive_128(pointer.as_bytes(), FETCH_CAP_INFO)?;
    let ack_cap = derive_bound_128(message_key, ACK_CAP_LABEL, &blob_id)?;
    let manage_cap = derive_bound_128(send_key, MANAGE_CAP_LABEL, &blob_id)?;
    let delivery_tag = derive_128(conversation_key, DELIVERY_TAG_INFO)?;

    Ok(PointerCapabilities {
        blob_id,
        fetch_cap,
        ack_cap,
        manage_cap,
        delivery_tag,
    })
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
