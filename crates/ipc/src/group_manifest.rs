//! Group transport manifests: one carrier pointer, one sealed capability per
//! recipient.
//!
//! A manifest is uploaded as `multi-fetch`.  That class is an explicit upload
//! property: the first recipient's acknowledgement is a server-side no-op, so
//! it cannot starve the rest of the group.

use std::collections::BTreeSet;

use crypto::aead::{self, Key, Nonce};

use crate::transport::ObjectClass;

const VERSION: u8 = 1;
const HEADER_BYTES: usize = 3;
const CAPABILITY_BYTES: usize = 16;
const SEALED_ENTRY_BYTES: usize = aead::NONCE_SIZE + CAPABILITY_BYTES + aead::TAG_SIZE;
const MANIFEST_AAD_LABEL: &[u8] = b"osl/group-manifest/v1";
const SHARD_BYTES: usize = 64 * 1024;
const ENTRIES_PER_SHARD: usize = (SHARD_BYTES - HEADER_BYTES) / SEALED_ENTRY_BYTES;

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

/// A logical group manifest backed by as many independently uploadable blobs
/// as its roster needs.  The transport bound applies to each shard, never to
/// the whole recipient set.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GroupManifest {
    shards: Vec<Vec<u8>>,
    recipient_count: usize,
}

impl GroupManifest {
    /// Rebuild a logical manifest after its blobs have been fetched.
    pub fn from_shards(shards: Vec<Vec<u8>>) -> Result<Self, GroupManifestError> {
        if shards.is_empty() {
            return Err(GroupManifestError::EmptyRecipients);
        }
        let mut recipient_count = 0usize;
        for shard in &shards {
            let count = parse_count(shard)?;
            recipient_count = recipient_count
                .checked_add(count)
                .ok_or(GroupManifestError::Malformed)?;
        }
        Ok(Self {
            shards,
            recipient_count,
        })
    }

    /// The bounded blobs to upload under independent transport capabilities.
    pub fn shards(&self) -> &[Vec<u8>] {
        &self.shards
    }

    pub const fn recipient_count(&self) -> usize {
        self.recipient_count
    }
}

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum GroupManifestError {
    #[error("a manifest requires at least one recipient")]
    EmptyRecipients,
    #[error("a manifest must contain one distinct recipient key per entry")]
    DuplicateRecipient,
    #[error("manifest is malformed")]
    Malformed,
    #[error("manifest entry authentication failed")]
    Authentication,
}

/// The object class that must accompany every manifest upload.  It is never
/// inferred from the pointer or the ciphertext.
pub const fn object_class() -> ObjectClass {
    ObjectClass::MultiFetch
}

/// Build a padded manifest. Each entry receives an independent nonce and is
/// sealed under that recipient's own key, so another member cannot recover
/// the capability even though every member downloads the same blob.
pub fn seal(entries: &[ManifestEntry<'_>]) -> Result<GroupManifest, GroupManifestError> {
    if entries.is_empty() {
        return Err(GroupManifestError::EmptyRecipients);
    }
    let mut recipient_keys = BTreeSet::new();
    for entry in entries {
        if !recipient_keys.insert(*entry.recipient_key.0.as_bytes()) {
            // Sharing a key would let one member open another member's
            // transport capability.  This is a send-time failure, never a
            // reason to silently collapse entries into a group-wide key.
            return Err(GroupManifestError::DuplicateRecipient);
        }
    }

    let mut shards = Vec::with_capacity(entries.len().div_ceil(ENTRIES_PER_SHARD));
    for shard_entries in entries.chunks(ENTRIES_PER_SHARD) {
        shards.push(seal_shard(shard_entries)?);
    }
    GroupManifest::from_shards(shards)
}

fn seal_shard(entries: &[ManifestEntry<'_>]) -> Result<Vec<u8>, GroupManifestError> {
    let count = u16::try_from(entries.len()).map_err(|_| GroupManifestError::Malformed)?;
    let raw_len = HEADER_BYTES + entries.len() * SEALED_ENTRY_BYTES;
    let mut manifest = Vec::with_capacity(raw_len);
    manifest.push(VERSION);
    manifest.extend_from_slice(&count.to_be_bytes());
    for (index, entry) in entries.iter().enumerate() {
        let nonce_bytes: [u8; aead::NONCE_SIZE] = crypto::random::random_bytes(aead::NONCE_SIZE)
            .try_into()
            .expect("random manifest nonce length is fixed");
        let nonce = Nonce::from_bytes(nonce_bytes);
        let ciphertext = aead::seal(
            &entry.recipient_key.0,
            &nonce,
            &entry_aad(count, index),
            &entry.capability,
        )
        .map_err(|_| GroupManifestError::Authentication)?;
        manifest.extend_from_slice(nonce.as_bytes());
        manifest.extend_from_slice(&ciphertext);
    }

    let padded = crate::transport_padding::pad_transport_object(manifest)
        .ok_or(GroupManifestError::Malformed)?;
    if padded.len() > SHARD_BYTES {
        return Err(GroupManifestError::Malformed);
    }
    Ok(padded)
}

/// Open the entry intended for `recipient_key`.  Entries for other recipients
/// intentionally fail AEAD authentication and reveal no capability.
pub fn open_for(
    manifest: &GroupManifest,
    recipient_key: &RecipientManifestKey,
) -> Result<[u8; CAPABILITY_BYTES], GroupManifestError> {
    for shard in &manifest.shards {
        let count = parse_count(shard)?;
        let entries_end = HEADER_BYTES + count * SEALED_ENTRY_BYTES;
        for (index, entry) in shard[HEADER_BYTES..entries_end]
            .chunks_exact(SEALED_ENTRY_BYTES)
            .enumerate()
        {
            let nonce = Nonce::from_bytes(
                entry[..aead::NONCE_SIZE]
                    .try_into()
                    .expect("fixed nonce slice"),
            );
            if let Ok(plaintext) = aead::open(
                &recipient_key.0,
                &nonce,
                &entry_aad(u16::try_from(count).expect("parsed count fits u16"), index),
                &entry[aead::NONCE_SIZE..],
            ) {
                return plaintext
                    .try_into()
                    .map_err(|_| GroupManifestError::Malformed);
            }
        }
    }
    Err(GroupManifestError::Authentication)
}

/// Bind each ciphertext to its exact manifest slot.  The count makes an
/// entry from a smaller manifest ineligible for a larger one, and the index
/// prevents a copied entry from becoming another member's entry.
fn entry_aad(count: u16, index: usize) -> Vec<u8> {
    let index = u16::try_from(index).expect("manifest index is bounded by u16 count");
    let mut aad = Vec::with_capacity(MANIFEST_AAD_LABEL.len() + 4);
    aad.extend_from_slice(MANIFEST_AAD_LABEL);
    aad.extend_from_slice(&count.to_be_bytes());
    aad.extend_from_slice(&index.to_be_bytes());
    aad
}

fn parse_count(manifest: &[u8]) -> Result<usize, GroupManifestError> {
    if manifest.len() < HEADER_BYTES || manifest.len() > SHARD_BYTES || manifest[0] != VERSION {
        return Err(GroupManifestError::Malformed);
    }
    let count = u16::from_be_bytes([manifest[1], manifest[2]]) as usize;
    if count == 0 {
        return Err(GroupManifestError::Malformed);
    }
    let entries_end = HEADER_BYTES
        .checked_add(
            count
                .checked_mul(SEALED_ENTRY_BYTES)
                .ok_or(GroupManifestError::Malformed)?,
        )
        .ok_or(GroupManifestError::Malformed)?;
    if entries_end > manifest.len() {
        return Err(GroupManifestError::Malformed);
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn t1_t37() {
        let alice = RecipientManifestKey::from_bytes([0xa1; aead::KEY_SIZE]);
        let bob = RecipientManifestKey::from_bytes([0xb2; aead::KEY_SIZE]);
        let alice_cap = [0x11; CAPABILITY_BYTES];
        let bob_cap = [0x22; CAPABILITY_BYTES];
        let manifest = seal(&[
            ManifestEntry {
                recipient_key: &alice,
                capability: alice_cap,
            },
            ManifestEntry {
                recipient_key: &bob,
                capability: bob_cap,
            },
        ])
        .expect("manifest seals");

        // Each member gets only its own per-copy capability.  If sealing is
        // changed to use one group key, Bob's entry either fails to open or
        // Alice can recover it, and this behavioral test fails.
        assert_eq!(open_for(&manifest, &alice).unwrap(), alice_cap);
        assert_eq!(open_for(&manifest, &bob).unwrap(), bob_cap);

        let mallory = RecipientManifestKey::from_bytes([0xc3; aead::KEY_SIZE]);
        assert_eq!(
            open_for(&manifest, &mallory),
            Err(GroupManifestError::Authentication)
        );

        // The class is part of the upload declaration, not an inference from
        // ciphertext. T6-W5 makes its ACK a 204 no-op; therefore an ACK by
        // Alice cannot consume the shared manifest before Bob fetches it.
        assert_eq!(object_class(), ObjectClass::MultiFetch);
        assert_eq!(open_for(&manifest, &bob).unwrap(), bob_cap);
    }

    #[test]
    fn duplicate_recipient_key_is_refused_instead_of_creating_a_shared_capability() {
        let shared = RecipientManifestKey::from_bytes([0xa1; aead::KEY_SIZE]);
        let result = seal(&[
            ManifestEntry {
                recipient_key: &shared,
                capability: [0x11; CAPABILITY_BYTES],
            },
            ManifestEntry {
                recipient_key: &shared,
                capability: [0x22; CAPABILITY_BYTES],
            },
        ]);

        assert_eq!(result, Err(GroupManifestError::DuplicateRecipient));
    }

    #[test]
    fn roster_larger_than_one_blob_is_transparently_sharded_without_a_member_ceiling() {
        let count = ENTRIES_PER_SHARD * 2 + 17;
        let keys: Vec<_> = (0..count)
            .map(|index| {
                let mut bytes = [0u8; aead::KEY_SIZE];
                bytes[..8].copy_from_slice(&(index as u64).to_be_bytes());
                RecipientManifestKey::from_bytes(bytes)
            })
            .collect();
        let entries: Vec<_> = keys
            .iter()
            .enumerate()
            .map(|(index, recipient_key)| {
                let mut capability = [0u8; CAPABILITY_BYTES];
                capability[..8].copy_from_slice(&(index as u64).to_be_bytes());
                ManifestEntry {
                    recipient_key,
                    capability,
                }
            })
            .collect();

        let manifest = seal(&entries).expect("an increasing real roster seals");
        assert_eq!(manifest.recipient_count(), count);
        assert_eq!(manifest.shards().len(), 3);
        assert!(manifest.shards().iter().all(|shard| shard.len() <= SHARD_BYTES));
        assert_eq!(
            open_for(&manifest, &keys[count - 1]).unwrap(),
            entries[count - 1].capability,
            "a recipient in the final shard opens its own capability"
        );

        let restored = GroupManifest::from_shards(manifest.shards().to_vec())
            .expect("fetched shards reconstruct the logical manifest");
        assert_eq!(restored.recipient_count(), count);
        assert_eq!(
            open_for(&restored, &keys[ENTRIES_PER_SHARD]).unwrap(),
            entries[ENTRIES_PER_SHARD].capability,
        );
    }
}
