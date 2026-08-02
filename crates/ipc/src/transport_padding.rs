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

#[cfg(test)]
mod tests {
    use super::{pad_transport_object, padded_transport_len};

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
}
