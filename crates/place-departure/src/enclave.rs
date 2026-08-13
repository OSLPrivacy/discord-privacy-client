//! Enclave places: a roster/epoch ladder plus per-channel content keys.
//!
//! A departure advances the roster epoch and then re-keys **every channel the
//! leaver could read**. Re-keying means a fresh CSPRNG content key with a fresh
//! key id, wrapped to each remaining member of that channel and to nobody else.
//! Channels the leaver was never a member of are deliberately left alone: a
//! blanket re-key would be indistinguishable from a no-op re-key, and this way
//! the check can tell the difference.

use std::collections::{BTreeMap, BTreeSet};

use crypto::{aead, ml_kem_768};
use serde::{Deserialize, Serialize};

use crate::ids::MemberId;
use crate::keys::{open_to_member, seal_to_member, SealedToMember};

/// A channel's opaque, name-independent identity.
///
/// Renaming a channel cannot create a second key domain, so this is CSPRNG
/// output carried alongside the display name rather than derived from it.
pub type ChannelId = ipc::space_roster::SpaceChannelId;

/// Who may read a channel.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "scope", rename_all = "snake_case")]
pub enum ChannelMemberList {
    /// Every member on the enclave roster.
    OpenToEnclave,
    /// An explicit list, independent of any governance role.
    Limited { members: BTreeSet<MemberId> },
}

/// One channel of an enclave, as every client holds it.
#[derive(Clone, Debug)]
pub struct EnclaveChannel {
    pub channel_id: ChannelId,
    pub handle: String,
    pub name: String,
    pub member_list: ChannelMemberList,
    pub key_epoch: u64,
    pub key_id: [u8; 16],
}

impl EnclaveChannel {
    pub fn readers(&self, roster: &BTreeSet<MemberId>) -> BTreeSet<MemberId> {
        match &self.member_list {
            ChannelMemberList::OpenToEnclave => roster.clone(),
            ChannelMemberList::Limited { members } => {
                members.intersection(roster).copied().collect()
            }
        }
    }

    pub fn includes(&self, roster: &BTreeSet<MemberId>, member: MemberId) -> bool {
        self.readers(roster).contains(&member)
    }
}

/// A content key as one client holds it for one channel.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct HeldChannelKey {
    pub channel_handle: String,
    #[serde(with = "crate::keys::hex_bytes")]
    pub channel_id: Vec<u8>,
    pub key_epoch: u64,
    #[serde(with = "crate::keys::hex_bytes")]
    pub key_id: Vec<u8>,
    #[serde(with = "crate::keys::hex_bytes")]
    pub key: Vec<u8>,
}

/// A re-key distribution addressed to exactly one remaining channel member.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ChannelKeyDistribution {
    pub channel_handle: String,
    pub key_epoch: u64,
    #[serde(with = "crate::keys::hex_bytes")]
    pub key_id: Vec<u8>,
    pub wrap: SealedToMember,
}

/// Sealed channel content.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ChannelMessage {
    pub channel_handle: String,
    pub sender: MemberId,
    #[serde(with = "crate::keys::hex_bytes")]
    pub key_id: Vec<u8>,
    #[serde(with = "crate::keys::hex_bytes")]
    pub nonce: Vec<u8>,
    #[serde(with = "crate::keys::hex_bytes")]
    pub ciphertext: Vec<u8>,
}

fn content_ad(channel: &ChannelId, key_id: &[u8; 16], key_epoch: u64) -> Vec<u8> {
    let mut ad = Vec::new();
    ad.extend_from_slice(b"OSL/place-departure/channel-content/v1");
    ad.extend_from_slice(channel.as_bytes());
    ad.extend_from_slice(key_id);
    ad.extend_from_slice(&key_epoch.to_be_bytes());
    ad
}

/// Generates the next content key for a channel: fresh key, fresh key id, next
/// key epoch. Nothing about the previous key survives.
pub fn next_channel_key(channel: &EnclaveChannel) -> (u64, [u8; 16], [u8; 32]) {
    let mut key_id = [0_u8; 16];
    key_id.copy_from_slice(&crypto::random::random_bytes(16));
    let mut key = [0_u8; 32];
    key.copy_from_slice(&crypto::random::random_bytes(32));
    (channel.key_epoch + 1, key_id, key)
}

/// Wraps a channel key for each remaining reader of that channel.
pub fn distribute_channel_key(
    channel: &EnclaveChannel,
    key: &[u8; 32],
    recipients: &[(MemberId, &ml_kem_768::EncapsulationKey)],
) -> Result<Vec<ChannelKeyDistribution>, EnclaveError> {
    let mut out = Vec::new();
    for (recipient, kem_public) in recipients {
        out.push(ChannelKeyDistribution {
            channel_handle: channel.handle.clone(),
            key_epoch: channel.key_epoch,
            key_id: channel.key_id.to_vec(),
            wrap: seal_to_member(*recipient, kem_public, key)?,
        });
    }
    Ok(out)
}

pub fn accept_channel_key(
    distribution: &ChannelKeyDistribution,
    channel: &EnclaveChannel,
    kem_secret: &ml_kem_768::DecapsulationKey,
) -> Result<HeldChannelKey, EnclaveError> {
    let key = open_to_member(&distribution.wrap, kem_secret)?;
    if key.len() != 32 {
        return Err(EnclaveError::Malformed);
    }
    Ok(HeldChannelKey {
        channel_handle: distribution.channel_handle.clone(),
        channel_id: channel.channel_id.as_bytes().to_vec(),
        key_epoch: distribution.key_epoch,
        key_id: distribution.key_id.clone(),
        key: key.to_vec(),
    })
}

pub fn seal_channel_message(
    channel: &EnclaveChannel,
    sender: MemberId,
    held: &HeldChannelKey,
    plaintext: &[u8],
) -> Result<ChannelMessage, EnclaveError> {
    let mut key_bytes = [0_u8; 32];
    let mut key_id = [0_u8; 16];
    if held.key.len() != key_bytes.len() || held.key_id.len() != key_id.len() {
        return Err(EnclaveError::Malformed);
    }
    key_bytes.copy_from_slice(&held.key);
    key_id.copy_from_slice(&held.key_id);
    let nonce = crypto::random::random_nonce();
    let ciphertext = aead::seal(
        &aead::Key::from_bytes(key_bytes),
        &nonce,
        &content_ad(&channel.channel_id, &key_id, held.key_epoch),
        plaintext,
    )?;
    Ok(ChannelMessage {
        channel_handle: channel.handle.clone(),
        sender,
        key_id: key_id.to_vec(),
        nonce: nonce.as_bytes().to_vec(),
        ciphertext,
    })
}

pub fn open_channel_message(
    channel: &EnclaveChannel,
    message: &ChannelMessage,
    held: &HeldChannelKey,
) -> Result<Vec<u8>, EnclaveError> {
    let mut key_bytes = [0_u8; 32];
    let mut key_id = [0_u8; 16];
    let mut nonce_bytes = [0_u8; aead::NONCE_SIZE];
    if held.key.len() != key_bytes.len()
        || held.key_id.len() != key_id.len()
        || message.nonce.len() != nonce_bytes.len()
    {
        return Err(EnclaveError::Malformed);
    }
    key_bytes.copy_from_slice(&held.key);
    key_id.copy_from_slice(&held.key_id);
    nonce_bytes.copy_from_slice(&message.nonce);
    Ok(aead::open(
        &aead::Key::from_bytes(key_bytes),
        &aead::Nonce::from_bytes(nonce_bytes),
        &content_ad(&channel.channel_id, &key_id, held.key_epoch),
        &message.ciphertext,
    )?)
}

/// The enclave's admission decision for one delivery.
///
/// A departed member is denied before any key or ciphertext is handed over.
/// This is separate from the fact that they hold no current key: both have to
/// hold, because either alone would be a single point of failure.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "decision", rename_all = "snake_case")]
pub enum Admission {
    Allowed,
    DeniedNotOnRoster,
    DeniedNotInChannel,
}

pub fn admit(
    roster: &BTreeSet<MemberId>,
    channel: &EnclaveChannel,
    member: MemberId,
) -> Admission {
    if !roster.contains(&member) {
        return Admission::DeniedNotOnRoster;
    }
    if !channel.includes(roster, member) {
        return Admission::DeniedNotInChannel;
    }
    Admission::Allowed
}

/// Channel key material held by one client, keyed by channel handle.
pub type HeldChannelKeys = BTreeMap<String, HeldChannelKey>;

#[derive(Debug, thiserror::Error)]
pub enum EnclaveError {
    #[error("enclave wire value is malformed")]
    Malformed,
    #[error("enclave crypto failed: {0}")]
    Crypto(#[from] crypto::Error),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::MemberKeys;

    fn channel(handle: &str, list: ChannelMemberList) -> EnclaveChannel {
        let mut id = [0_u8; ipc::space_roster::SpaceChannelId::LENGTH];
        let id_len = id.len();
            id.copy_from_slice(&crypto::random::random_bytes(id_len));
        EnclaveChannel {
            channel_id: ChannelId::from_bytes(id),
            handle: handle.to_string(),
            name: handle.to_string(),
            member_list: list,
            key_epoch: 1,
            key_id: [7_u8; 16],
        }
    }

    #[test]
    fn a_rekeyed_channel_stops_opening_for_the_departed_member() {
        let ash = MemberKeys::generate("ash", "Ash");
        let wren = MemberKeys::generate("wren", "Wren");
        let roster_before = BTreeSet::from([ash.member_id, wren.member_id]);
        let mut general = channel("general", ChannelMemberList::OpenToEnclave);

        let mut key = [0_u8; 32];
        key.copy_from_slice(&crypto::random::random_bytes(32));
        let recipients = vec![
            (ash.member_id, &ash.kem_public),
            (wren.member_id, &wren.kem_public),
        ];
        let distributions = distribute_channel_key(&general, &key, &recipients).unwrap();
        let ash_key = accept_channel_key(&distributions[0], &general, &ash.kem_secret).unwrap();
        let wren_key = accept_channel_key(&distributions[1], &general, &wren.kem_secret).unwrap();

        let before = seal_channel_message(&general, ash.member_id, &ash_key, b"before").unwrap();
        assert_eq!(
            open_channel_message(&general, &before, &wren_key).unwrap(),
            b"before"
        );

        // Wren leaves: roster shrinks, the channel is re-keyed to Ash only.
        let roster_after = BTreeSet::from([ash.member_id]);
        assert_eq!(
            admit(&roster_after, &general, wren.member_id),
            Admission::DeniedNotOnRoster
        );
        let (epoch, key_id, new_key) = next_channel_key(&general);
        general.key_epoch = epoch;
        general.key_id = key_id;
        let rekeyed =
            distribute_channel_key(&general, &new_key, &[(ash.member_id, &ash.kem_public)]).unwrap();
        assert_eq!(rekeyed.len(), 1);
        assert_eq!(rekeyed[0].wrap.recipient, ash.member_id);
        let ash_new = accept_channel_key(&rekeyed[0], &general, &ash.kem_secret).unwrap();

        let after = seal_channel_message(&general, ash.member_id, &ash_new, b"after").unwrap();
        assert_eq!(
            open_channel_message(&general, &after, &ash_new).unwrap(),
            b"after"
        );
        assert!(
            open_channel_message(&general, &after, &wren_key).is_err(),
            "a re-keyed channel must not open for the key the leaver kept"
        );
        assert_eq!(roster_before.len(), 2);
    }
}
