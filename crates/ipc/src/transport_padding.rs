//! Padmé padding for ciphertext objects sent to the cipher-store.
//!
//! This is deliberately separate from `crypto::padding`: that module pads a
//! plaintext before AEAD encryption, whereas this one pads the final object
//! whose length is visible to the transport observer.

/// Return the Padmé-padded length for a transport object.
///
/// The cipher-store rejects empty objects, so an empty input is promoted to
/// one byte. `None` means the padded length cannot be represented as a
/// `usize`.
pub fn padded_transport_len(length: usize) -> Option<usize> {
    if length <= 1 {
        return Some(1);
    }

    let exponent = length.ilog2();
    let significant_bits = exponent.ilog2() + 1;
    let granularity = 1usize << (exponent - significant_bits);
    length
        .checked_add(granularity - 1)
        .map(|length| length / granularity * granularity)
}

/// Append zero bytes until `object` has a Padmé transport length.
///
/// The original object remains a prefix of the returned buffer.  The appended
/// bytes are outside the encrypted payload and exist solely to conceal its
/// length from the transport observer.
pub fn pad_transport_object(mut object: Vec<u8>) -> Option<Vec<u8>> {
    let padded_len = padded_transport_len(object.len())?;
    object.resize(padded_len, 0);
    Some(object)
}

/// Bytes of the length header a framed transport object carries.
const LENGTH_PREFIX_BYTES: usize = 4;

/// Pad an object whose own bytes do not describe their length.
///
/// Padmé conceals a length by appending bytes, which is only recoverable for a
/// payload that is self-delimiting. A raw ciphertext is not: appending zeroes
/// to it moves the AEAD tag and the object no longer opens. Frame the true
/// length ahead of the payload so the padding stays invisible to the reader
/// and visible only as an object size to the transport observer.
///
/// `None` means the framed length cannot be represented as a `usize`.
pub fn frame_padded_transport_object(payload: &[u8]) -> Option<Vec<u8>> {
    let length = u32::try_from(payload.len()).ok()?;
    let mut framed = Vec::with_capacity(payload.len().checked_add(LENGTH_PREFIX_BYTES)?);
    framed.extend_from_slice(&length.to_be_bytes());
    framed.extend_from_slice(payload);
    pad_transport_object(framed)
}

/// Recover the exact payload from a framed, padded transport object.
///
/// `None` for any object whose header is absent or claims more bytes than the
/// object holds. A truncated or forged object is refused here rather than
/// handed on as a shorter payload.
pub fn unframe_padded_transport_object(object: &[u8]) -> Option<&[u8]> {
    let header = object.get(..LENGTH_PREFIX_BYTES)?;
    let length = usize::try_from(u32::from_be_bytes(header.try_into().ok()?)).ok()?;
    object
        .get(LENGTH_PREFIX_BYTES..LENGTH_PREFIX_BYTES.checked_add(length)?)
}

#[cfg(test)]
mod tests {
    use super::{
        frame_padded_transport_object, pad_transport_object, padded_transport_len,
        unframe_padded_transport_object,
    };

    #[test]
    fn pads_an_unpadded_upload_to_a_server_accepted_length() {
        let object = vec![0xa5; 1_001];
        let padded = pad_transport_object(object.clone()).expect("representable padding");

        // The cipher-store's Padmé implementation rounds 1,001 to 1,024.
        // Checking the prefix and suffix ensures padding never changes the
        // ciphertext and is deterministic zero-fill.
        assert_eq!(padded.len(), 1_024);
        assert_eq!(&padded[..object.len()], object.as_slice());
        assert!(padded[object.len()..].iter().all(|byte| *byte == 0));
    }

    #[test]
    fn preserves_lengths_that_already_have_a_padme_shape() {
        for length in [1, 2, 4, 8, 16, 32, 64, 128, 512, 1_024, 4_096] {
            assert_eq!(padded_transport_len(length), Some(length));
        }
    }

    #[test]
    fn promotes_an_empty_object_to_the_smallest_uploadable_length() {
        assert_eq!(padded_transport_len(0), Some(1));
        assert_eq!(pad_transport_object(Vec::new()), Some(vec![0]));
    }

    /// The property the send path depends on: whatever an arbitrary ciphertext
    /// is padded to, the reader gets back the exact original bytes. Without
    /// this the padding silently corrupts every message.
    #[test]
    fn framing_survives_padding_for_lengths_that_are_not_padme() {
        for length in [0, 1, 3, 17, 1_001, 4_097, 40_000] {
            let payload: Vec<u8> = (0..length).map(|index| (index % 251) as u8).collect();
            let object = frame_padded_transport_object(&payload).expect("representable framing");
            assert_eq!(
                padded_transport_len(object.len()),
                Some(object.len()),
                "a framed object is uploadable at its Padmé length"
            );
            assert!(
                unframe_padded_transport_object(&object).expect("framed object reads back")
                    == payload.as_slice(),
                "the recovered payload is byte-identical to the original"
            );
        }
    }

    #[test]
    fn refuses_an_object_whose_header_overruns_it() {
        assert_eq!(unframe_padded_transport_object(&[]), None);
        assert_eq!(unframe_padded_transport_object(&[0, 0, 0]), None);
        assert_eq!(unframe_padded_transport_object(&[0, 0, 0, 9, 1, 2]), None);
        assert_eq!(
            unframe_padded_transport_object(&[0xff, 0xff, 0xff, 0xff, 1]),
            None
        );
    }
}
