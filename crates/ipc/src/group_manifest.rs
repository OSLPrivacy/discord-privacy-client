//! Group transport manifests: one carrier pointer, one sealed capability per
//! recipient.
//!
//! A manifest is uploaded as `multi-fetch`.  That class is an explicit upload
//! property: the first recipient's acknowledgement is a server-side no-op, so
//! it cannot starve the rest of the group.

use crypto::aead::{self, Key, Nonce};

use crate::transport::ObjectClass;

const VERSION: u8 = 1;
const HEADER_BYTES: usize = 3;
const CAPABILITY_BYTES: usize = 16;
const SEALED_ENTRY_BYTES: usize = aead::NONCE_SIZE + CAPABILITY_BYTES + aead::TAG_SIZE;
const MANIFEST_AAD: &[u8] = b"osl/group-manifest/v1";

/// Key material held by exactly one group recipient for opening that
/// recipient's manifest entry.  It is deliberately separate from a
/// sender-key content key: sharing the latter would let every group member
/// open every transport capability.
#[derive(Clone)]
pub struct RecipientManifestKey(Key);

impl RecipientManifestKey {
    pub fn from_bytes(bytes: [u8; aead::KEY_SIZE]) -> Self {
        Self(Key::from_bytes(bytes))
    }
}

/// One recipient capability before it is sealed into the manifest.
pub struct ManifestEntry<'a> {
    pub recipient_key: &'a RecipientManifestKey,
    pub capability: [u8; CAPABILITY_BYTES],
}

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum GroupManifestError {
    #[error("a manifest requires at least one recipient and at most {0}")]
    RecipientCount(usize),
    #[error("manifest is malformed")]
    Malformed,
    #[error("manifest entry authentication failed")]
    Authentication,
    #[error("manifest exceeds the cipher-store blob limit")]
    TooLarge,
}

/// The object class that must accompany every manifest upload.  It is never
/// inferred from the pointer or the ciphertext.
pub const fn object_class() -> ObjectClass {
    ObjectClass::MultiFetch
}

/// Build a padded manifest. Each entry receives an independent nonce and is
/// sealed under that recipient's own key, so another member cannot recover
/// the capability even though every member downloads the same blob.
pub fn seal(entries: &[ManifestEntry<'_>]) -> Result<Vec<u8>, GroupManifestError> {
    let count = u16::try_from(entries.len()).map_err(|_| GroupManifestError::RecipientCount(u16::MAX as usize))?;
    if count == 0 {
        return Err(GroupManifestError::RecipientCount(0));
    }

    let raw_len = HEADER_BYTES
        .checked_add(entries.len().checked_mul(SEALED_ENTRY_BYTES).ok_or(GroupManifestError::TooLarge)?)
        .ok_or(GroupManifestError::TooLarge)?;
    if raw_len > 64 * 1024 {
        return Err(GroupManifestError::TooLarge);
    }

    let mut manifest = Vec::with_capacity(raw_len);
    manifest.push(VERSION);
    manifest.extend_from_slice(&count.to_be_bytes());
    for entry in entries {
        let nonce_bytes: [u8; aead::NONCE_SIZE] = crypto::random::random_bytes(aead::NONCE_SIZE)
            .try_into()
            .expect("random manifest nonce length is fixed");
        let nonce = Nonce::from_bytes(nonce_bytes);
        let ciphertext = aead::seal(&entry.recipient_key.0, &nonce, MANIFEST_AAD, &entry.capability)
            .map_err(|_| GroupManifestError::Authentication)?;
        manifest.extend_from_slice(nonce.as_bytes());
        manifest.extend_from_slice(&ciphertext);
    }

    crate::transport_padding::pad_transport_object(manifest).ok_or(GroupManifestError::TooLarge)
}

/// Open the entry intended for `recipient_key`.  Entries for other recipients
/// intentionally fail AEAD authentication and reveal no capability.
pub fn open_for(
    manifest: &[u8],
    recipient_key: &RecipientManifestKey,
) -> Result<[u8; CAPABILITY_BYTES], GroupManifestError> {
    let count = parse_count(manifest)?;
    let entries_end = HEADER_BYTES
        .checked_add(count.checked_mul(SEALED_ENTRY_BYTES).ok_or(GroupManifestError::Malformed)?)
        .ok_or(GroupManifestError::Malformed)?;
    if entries_end > manifest.len() {
        return Err(GroupManifestError::Malformed);
    }

    for entry in manifest[HEADER_BYTES..entries_end].chunks_exact(SEALED_ENTRY_BYTES) {
        let nonce = Nonce::from_bytes(entry[..aead::NONCE_SIZE].try_into().expect("fixed nonce slice"));
        if let Ok(plaintext) = aead::open(&recipient_key.0, &nonce, MANIFEST_AAD, &entry[aead::NONCE_SIZE..]) {
            return plaintext.try_into().map_err(|_| GroupManifestError::Malformed);
        }
    }
    Err(GroupManifestError::Authentication)
}

fn parse_count(manifest: &[u8]) -> Result<usize, GroupManifestError> {
    if manifest.len() < HEADER_BYTES || manifest[0] != VERSION {
        return Err(GroupManifestError::Malformed);
    }
    let count = u16::from_be_bytes([manifest[1], manifest[2]]) as usize;
    if count == 0 {
        return Err(GroupManifestError::Malformed);
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn t1_t37_entries_are_recipient_isolated_and_manifest_ack_is_non_destructive() {
        let alice = RecipientManifestKey::from_bytes([0xa1; aead::KEY_SIZE]);
        let bob = RecipientManifestKey::from_bytes([0xb2; aead::KEY_SIZE]);
        let alice_cap = [0x11; CAPABILITY_BYTES];
        let bob_cap = [0x22; CAPABILITY_BYTES];
        let manifest = seal(&[
            ManifestEntry { recipient_key: &alice, capability: alice_cap },
            ManifestEntry { recipient_key: &bob, capability: bob_cap },
        ]).expect("manifest seals");

        assert_eq!(object_class(), ObjectClass::MultiFetch);
        assert_eq!(open_for(&manifest, &alice).unwrap(), alice_cap);
        assert_eq!(open_for(&manifest, &bob).unwrap(), bob_cap);

        let mallory = RecipientManifestKey::from_bytes([0xc3; aead::KEY_SIZE]);
        assert_eq!(open_for(&manifest, &mallory), Err(GroupManifestError::Authentication));

        // The upload class is the storage-side lifecycle contract: T6-W5
        // returns 204 without deleting a `multi-fetch` object, so Bob can
        // still fetch after Alice has acknowledged it.
        assert_eq!(object_class(), ObjectClass::MultiFetch);
        assert_eq!(open_for(&manifest, &bob).unwrap(), bob_cap);
    }
}
