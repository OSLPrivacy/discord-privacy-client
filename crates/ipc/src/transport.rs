//! Client-side authority construction for the pointer transport.
//!
//! The server-facing tuple is derived from a fresh carrier seed, never from
//! scope metadata.  This module deliberately exposes only the derived upload
//! metadata: callers must not accidentally serialize the seed `P` into an
//! HTTP request.  Encoding `P` into the prose carrier is owned by T1-32.

use crypto::pointer::{
    derive_capabilities, derive_conversation_message_key, derive_delivery_tag_window,
    resolve_delivery_tag, DeliveryTagMatch, DeliveryTagResolveError, Pointer, CAPABILITY_BYTES,
    POINTER_BYTES,
};
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

pub use crypto::pointer::DELIVERY_TAG_LOOKAHEAD;

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
    conversation_epoch_secret: &[u8],
    send_counter: u32,
    object_class: ObjectClass,
) -> Result<UploadMetadata, crypto::Error> {
    let conversation_message_key =
        derive_conversation_message_key(conversation_epoch_secret, send_counter)?;
    let caps = derive_capabilities(pointer, message_key, send_key, &conversation_message_key)?;
    Ok(UploadMetadata {
        blob_id: hex(&caps.blob_id),
        fetch_digest: digest_hex(&caps.fetch_cap),
        ack_digest: digest_hex(&caps.ack_cap),
        manage_digest: digest_hex(&caps.manage_cap),
        delivery_tag: hex(&caps.delivery_tag),
        object_class,
    })
}

pub fn subscription_delivery_tags(
    conversation_epoch_secret: &[u8],
    receiver_counter: u32,
) -> Result<Vec<[u8; CAPABILITY_BYTES]>, crypto::Error> {
    Ok(
        derive_delivery_tag_window(conversation_epoch_secret, receiver_counter)?
            .into_iter()
            .map(|candidate| candidate.delivery_tag)
            .collect(),
    )
}

pub fn resolve_subscription_delivery_tag(
    conversation_epoch_secret: &[u8],
    receiver_counter: u32,
    observed: &[u8; CAPABILITY_BYTES],
) -> core::result::Result<DeliveryTagMatch, DeliveryTagResolveError> {
    resolve_delivery_tag(conversation_epoch_secret, receiver_counter, observed)
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
        let first = upload_metadata(&p1, &key, &key, &key, 0, ObjectClass::SingleAck).unwrap();
        let second = upload_metadata(&p2, &key, &key, &key, 1, ObjectClass::SingleAck).unwrap();

        assert_ne!(first.blob_id, second.blob_id);
        assert_ne!(first.fetch_digest, second.fetch_digest);
        assert_ne!(first.ack_digest, second.ack_digest);
        assert_ne!(first.manage_digest, second.manage_digest);

        let p1_hex: String = p1
            .as_bytes()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        let server_values = [
            &first.blob_id,
            &first.fetch_digest,
            &first.ack_digest,
            &first.manage_digest,
            &first.delivery_tag,
        ];
        assert!(server_values.iter().all(|value| **value != p1_hex));
    }

    /// An Adversary correctly rejected the previous version of this helper: it
    /// returned `first.delivery_tag == second.delivery_tag`, so the test above it
    /// only asserted that two tags differ. Any counter satisfies that. It caught a
    /// static tag and nothing else, while reading like a privacy proof.
    ///
    /// Linkability is the real property: given two tags, can an observer WITHOUT
    /// the conversation secret tell whether they belong to one conversation? So
    /// this models the observer's actual power — every relation computable from
    /// the two tag values alone — and reports whether any of them separates a
    /// same-conversation pair from a cross-conversation pair.
    fn public_observer_can_link_only_by_repeated_delivery_tag(
        first: &UploadMetadata,
        second: &UploadMetadata,
    ) -> bool {
        let a = tag_hex_to_bytes(&first.delivery_tag);
        let b = tag_hex_to_bytes(&second.delivery_tag);

        // 1. equality -- a static or repeated tag
        if a == b {
            return true;
        }
        // 2. shared prefix or suffix -- a conversation id spliced into the tag
        let shared_prefix = a.iter().zip(b.iter()).take_while(|(x, y)| x == y).count();
        let shared_suffix = a
            .iter()
            .rev()
            .zip(b.iter().rev())
            .take_while(|(x, y)| x == y)
            .count();
        if shared_prefix >= 4 || shared_suffix >= 4 {
            return true;
        }
        // 3. a constant XOR delta -- tags advancing by a fixed, guessable step
        let delta: Vec<u8> = a.iter().zip(b.iter()).map(|(x, y)| x ^ y).collect();
        if delta.windows(2).all(|w| w[0] == w[1]) {
            return true;
        }
        // 4. low Hamming distance -- a counter incremented in the clear
        let differing_bits: u32 = delta.iter().map(|byte| byte.count_ones()).sum();
        let total_bits = (CAPABILITY_BYTES * 8) as u32;
        if differing_bits * 4 < total_bits {
            return true;
        }
        false
    }

    fn tag_hex_to_bytes(value: &str) -> [u8; CAPABILITY_BYTES] {
        assert_eq!(value.len(), CAPABILITY_BYTES * 2);
        let mut out = [0u8; CAPABILITY_BYTES];
        for (index, slot) in out.iter_mut().enumerate() {
            *slot = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16).unwrap();
        }
        out
    }

    /// A test of the test. The previous linkability helper could not fail for any
    /// input that was not literally identical, so it passed while proving nothing.
    /// This starves the new one: each crafted pair exhibits exactly one linkage
    /// shape, and the helper must catch every one. If any assertion here goes
    /// green-when-it-should-be-red, the privacy assertion above is decoration.
    #[test]
    fn b0_14_the_linkability_helper_actually_detects_each_linkage_shape() {
        fn meta(tag: [u8; CAPABILITY_BYTES]) -> UploadMetadata {
            let key = [0x42u8; 32];
            let mut m = upload_metadata(
                &Pointer::from_bytes([0x01; POINTER_BYTES]),
                &key,
                &key,
                &key,
                0,
                ObjectClass::SingleAck,
            )
            .unwrap();
            m.delivery_tag = tag.iter().map(|b| format!("{b:02x}")).collect();
            m
        }
        let base = [0xA5u8; CAPABILITY_BYTES];

        // 1. identical tags
        assert!(public_observer_can_link_only_by_repeated_delivery_tag(
            &meta(base),
            &meta(base)
        ));

        // 2. shared prefix -- a conversation id spliced in
        let mut prefixed = [0x11u8; CAPABILITY_BYTES];
        prefixed[..6].copy_from_slice(&base[..6]);
        assert!(public_observer_can_link_only_by_repeated_delivery_tag(
            &meta(base),
            &meta(prefixed)
        ));

        // 3. constant XOR delta -- a fixed, guessable step
        let stepped: [u8; CAPABILITY_BYTES] = base.map(|b| b ^ 0x0F);
        assert!(public_observer_can_link_only_by_repeated_delivery_tag(
            &meta(base),
            &meta(stepped)
        ));

        // 4. low Hamming distance -- a counter incremented in the clear
        let mut nudged = base;
        nudged[CAPABILITY_BYTES - 1] ^= 0x01;
        assert!(public_observer_can_link_only_by_repeated_delivery_tag(
            &meta(base),
            &meta(nudged)
        ));

        // and the control: two genuinely unrelated tags must NOT be flagged,
        // or the helper would fail everything and be equally useless.
        let unrelated: [u8; CAPABILITY_BYTES] =
            std::array::from_fn(|i| (i as u8).wrapping_mul(37) ^ 0x5C);
        assert!(!public_observer_can_link_only_by_repeated_delivery_tag(
            &meta(base),
            &meta(unrelated)
        ));
    }

    #[test]
    fn b0_14_upload_delivery_tags_rotate_without_a_public_conversation_link_key() {
        let key = [0x42; 32];
        let first = upload_metadata(
            &Pointer::from_bytes([0x01; POINTER_BYTES]),
            &key,
            &key,
            &key,
            0,
            ObjectClass::SingleAck,
        )
        .unwrap();
        let second = upload_metadata(
            &Pointer::from_bytes([0x02; POINTER_BYTES]),
            &key,
            &key,
            &key,
            1,
            ObjectClass::SingleAck,
        )
        .unwrap();

        assert_ne!(first.delivery_tag, second.delivery_tag);
        assert!(
            !public_observer_can_link_only_by_repeated_delivery_tag(&first, &second),
            "an observer holding only stored tags must not get a repeated conversation key"
        );
    }

    #[test]
    fn b0_14_receiver_resolves_one_message_desync_and_names_a_window_miss() {
        let key = [0x42; 32];
        let start = 7;
        let one_ahead = upload_metadata(
            &Pointer::from_bytes([0x03; POINTER_BYTES]),
            &key,
            &key,
            &key,
            start + 1,
            ObjectClass::SingleAck,
        )
        .unwrap();
        let mut observed = tag_hex_to_bytes(&one_ahead.delivery_tag);

        let resolved = resolve_subscription_delivery_tag(&key, start, &observed).unwrap();
        assert_eq!(resolved.counter, start + 1);

        let too_far = upload_metadata(
            &Pointer::from_bytes([0x04; POINTER_BYTES]),
            &key,
            &key,
            &key,
            start + DELIVERY_TAG_LOOKAHEAD as u32,
            ObjectClass::SingleAck,
        )
        .unwrap();
        observed = tag_hex_to_bytes(&too_far.delivery_tag);

        assert!(
            matches!(
                resolve_subscription_delivery_tag(&key, start, &observed),
                Err(DeliveryTagResolveError::LookaheadMiss {
                    start_counter,
                    window: DELIVERY_TAG_LOOKAHEAD
                }) if start_counter == start
            ),
            "past-window desync must be a named clean miss"
        );
    }
}
