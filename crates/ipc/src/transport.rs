//! Client-side authority construction for the pointer transport.
//!
//! The server-facing tuple is derived from a fresh carrier seed, never from
//! scope metadata.  This module deliberately exposes only the derived upload
//! metadata: callers must not accidentally serialize the seed `P` into an
//! HTTP request.  Encoding `P` into the prose carrier is owned by T1-32.

use crypto::pointer::{derive_capabilities, Pointer, CAPABILITY_BYTES, POINTER_BYTES};
use sha2::{Digest, Sha256};

/// The upload class declared to the cipher store.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjectClass {
    SingleAck,
    MultiFetch,
}

impl ObjectClass {
    pub const fn as_header_value(self) -> &'static str {
        match self {
            Self::SingleAck => "single-ack",
            Self::MultiFetch => "multi-fetch",
        }
    }
}

/// Values that may cross the upload boundary.  In particular, this type has
/// no pointer field: `P` and the detect tag are carrier-only data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UploadMetadata {
    pub blob_id: String,
    pub fetch_digest: String,
    pub ack_digest: String,
    pub manage_digest: String,
    pub delivery_tag: String,
    pub object_class: ObjectClass,
}

/// Generates a fresh 160-bit carrier seed with the workspace CSPRNG.
pub fn fresh_pointer() -> Pointer {
    let bytes = crypto::random::random_bytes(POINTER_BYTES);
    let pointer: [u8; POINTER_BYTES] = bytes
        .try_into()
        .expect("random pointer length is fixed by POINTER_BYTES");
    Pointer::from_bytes(pointer)
}

/// Derives upload metadata from `P` and the key material held by the sending
/// layer.  Only SHA-256 digests of capabilities are supplied on upload; the
/// bearer capabilities themselves are never persisted by the server.
pub fn upload_metadata(
    pointer: &Pointer,
    message_key: &[u8],
    send_key: &[u8],
    conversation_key: &[u8],
    object_class: ObjectClass,
) -> Result<UploadMetadata, crypto::Error> {
    let caps = derive_capabilities(pointer, message_key, send_key, conversation_key)?;
    Ok(UploadMetadata {
        blob_id: hex(&caps.blob_id),
        fetch_digest: digest_hex(&caps.fetch_cap),
        ack_digest: digest_hex(&caps.ack_cap),
        manage_digest: digest_hex(&caps.manage_cap),
        delivery_tag: hex(&caps.delivery_tag),
        object_class,
    })
}

fn hex(bytes: &[u8; CAPABILITY_BYTES]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn digest_hex(bytes: &[u8; CAPABILITY_BYTES]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn t1_t30_fresh_seeds_produce_distinct_wire_authority_without_transmitting_p() {
        let p1 = fresh_pointer();
        let p2 = fresh_pointer();
        assert_ne!(p1, p2, "each message must use a fresh carrier seed");

        let key = [0x42; 32];
        let first = upload_metadata(&p1, &key, &key, &key, ObjectClass::SingleAck).unwrap();
        let second = upload_metadata(&p2, &key, &key, &key, ObjectClass::SingleAck).unwrap();

        assert_ne!(first.blob_id, second.blob_id);
        assert_ne!(first.fetch_digest, second.fetch_digest);
        assert_ne!(first.ack_digest, second.ack_digest);
        assert_ne!(first.manage_digest, second.manage_digest);

        let p1_hex: String = p1.as_bytes().iter().map(|byte| format!("{byte:02x}")).collect();
        let server_values = [
            &first.blob_id,
            &first.fetch_digest,
            &first.ack_digest,
            &first.manage_digest,
            &first.delivery_tag,
        ];
        assert!(server_values.iter().all(|value| **value != p1_hex));
    }
}
