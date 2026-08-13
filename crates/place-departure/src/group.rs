//! Group places: sender-key groups whose departure rotates real sender state.
//!
//! Each member runs one outbound `SenderChain` and one `ReceiverChain` per
//! peer. A departure makes every remaining member rotate their own chain and
//! re-distribute the new rotation root to the current roster only. The leaver
//! keeps whatever they already had — which is the honest outcome, because keys
//! already on a device cannot be recalled — but the rotation means their
//! retained receiver chains open nothing sent after they left.

use std::collections::BTreeMap;

use crypto::sender_keys::{
    EncryptedMessage, PhysicalDeviceId, SenderContext, SenderKeyState, PHYSICAL_DEVICE_ID_BYTES,
};
use crypto::{aead, ml_kem_768, x25519};
use serde::{Deserialize, Serialize};

use crate::ids::MemberId;
use crate::keys::{open_to_member, seal_to_member, MemberKeys, SealedToMember};

/// One sender-key distribution, addressed to exactly one recipient.
///
/// The rotation root travels inside `wrap`, sealed to that recipient's ML-KEM
/// key. Counting these is how "the leaver received 0 new keys" is measured.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SenderKeyDistribution {
    pub sender: MemberId,
    pub chain_id: u32,
    #[serde(with = "crate::keys::hex_bytes")]
    pub physical_device_id: Vec<u8>,
    pub wrap: SealedToMember,
}

/// The receiver material one client holds for one peer, as persisted.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct HeldSenderKey {
    pub sender: MemberId,
    pub chain_id: u32,
    #[serde(with = "crate::keys::hex_bytes")]
    pub physical_device_id: Vec<u8>,
    #[serde(with = "crate::keys::hex_bytes")]
    pub rotation_root: Vec<u8>,
}

/// A sender-key message on the wire.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct GroupMessage {
    pub sender: MemberId,
    #[serde(with = "crate::keys::hex_bytes")]
    pub header_nonce: Vec<u8>,
    #[serde(with = "crate::keys::hex_bytes")]
    pub enc_header: Vec<u8>,
    #[serde(with = "crate::keys::hex_bytes")]
    pub message_nonce: Vec<u8>,
    #[serde(with = "crate::keys::hex_bytes")]
    pub ciphertext: Vec<u8>,
}

impl GroupMessage {
    fn to_encrypted(&self) -> Result<EncryptedMessage, GroupError> {
        let mut header_nonce = [0_u8; aead::NONCE_SIZE];
        let mut message_nonce = [0_u8; aead::NONCE_SIZE];
        if self.header_nonce.len() != header_nonce.len()
            || self.message_nonce.len() != message_nonce.len()
        {
            return Err(GroupError::Malformed);
        }
        header_nonce.copy_from_slice(&self.header_nonce);
        message_nonce.copy_from_slice(&self.message_nonce);
        Ok(EncryptedMessage {
            header_nonce: aead::Nonce::from_bytes(header_nonce),
            enc_header: self.enc_header.clone(),
            message_nonce: aead::Nonce::from_bytes(message_nonce),
            ciphertext: self.ciphertext.clone(),
        })
    }
}

const SESSION_VERSION: u32 = 1;

/// The live sender-key session one client runs for one group place.
pub struct GroupSession {
    group_id: Vec<u8>,
    device_id: PhysicalDeviceId,
    state: SenderKeyState,
    /// Persisted receiver material, keyed by peer. Rebuilt into receiver
    /// chains on load, exactly as `sender_key_state.json` does in the client.
    held: BTreeMap<MemberId, HeldSenderKey>,
    /// The roster this client's own chain was last distributed to.
    last_distribution: Vec<MemberId>,
}

impl GroupSession {
    pub fn new(group_id: Vec<u8>, device_id: PhysicalDeviceId) -> Self {
        Self {
            group_id,
            device_id,
            state: SenderKeyState::new(),
            held: BTreeMap::new(),
            last_distribution: Vec::new(),
        }
    }

    /// Rebuilds a session from persisted receiver material.
    ///
    /// The outbound sender chain is deliberately not restored: sender chains
    /// are session-only, so the first send after a restart installs and
    /// distributes a fresh chain to whoever is on the roster *then*.
    pub fn restore(
        group_id: Vec<u8>,
        device_id: PhysicalDeviceId,
        held: Vec<HeldSenderKey>,
    ) -> Result<Self, GroupError> {
        let mut session = Self::new(group_id, device_id);
        for record in held {
            session.install_held(&record)?;
        }
        Ok(session)
    }

    fn install_held(&mut self, record: &HeldSenderKey) -> Result<(), GroupError> {
        let mut root = [0_u8; 32];
        let mut device = [0_u8; PHYSICAL_DEVICE_ID_BYTES];
        if record.rotation_root.len() != root.len()
            || record.physical_device_id.len() != device.len()
        {
            return Err(GroupError::Malformed);
        }
        root.copy_from_slice(&record.rotation_root);
        device.copy_from_slice(&record.physical_device_id);
        let device = PhysicalDeviceId::from_bytes(device)?;
        self.state.install_receiver(
            record.sender.as_bytes().to_vec(),
            record.chain_id,
            &root,
            device,
        )?;
        self.held.insert(record.sender, record.clone());
        Ok(())
    }

    pub fn held_keys(&self) -> Vec<HeldSenderKey> {
        self.held.values().cloned().collect()
    }

    pub fn current_chain_id(&self) -> Option<u32> {
        self.state.sender_chain().map(|chain| chain.current_chain_id())
    }

    pub fn has_sender_chain(&self) -> bool {
        self.state.sender_chain().is_some()
    }

    /// Installs a fresh outbound chain if there is none.
    pub fn ensure_sender_chain(&mut self) -> Result<bool, GroupError> {
        if self.state.sender_chain().is_some() {
            return Ok(false);
        }
        self.state
            .install_sender_for_physical_device(self.device_id)?;
        Ok(true)
    }

    /// Rotates the outbound chain. This is the group half of a departure: the
    /// rotation root behind every future message is replaced, and only the
    /// members still on the roster are given the new one.
    pub fn rotate_sender_chain(&mut self) -> Result<u32, GroupError> {
        self.ensure_sender_chain()?;
        self.state.rotate_sender()?;
        Ok(self
            .state
            .sender_chain()
            .expect("sender chain installed above")
            .current_chain_id())
    }

    /// Builds one distribution per recipient. Recipients come from the caller's
    /// current roster; a member who is not on it gets nothing, which is the
    /// whole mechanism behind "0 new keys for the leaver".
    pub fn distribute(
        &mut self,
        self_id: MemberId,
        recipients: &[(MemberId, &ml_kem_768::EncapsulationKey)],
    ) -> Result<Vec<SenderKeyDistribution>, GroupError> {
        self.ensure_sender_chain()?;
        let chain = self
            .state
            .sender_chain()
            .expect("sender chain installed above");
        let chain_id = chain.current_chain_id();
        let root = chain.rotation_root_bytes();
        let device = *chain.physical_device_id().as_bytes();
        let mut out = Vec::new();
        for (recipient, kem_public) in recipients {
            if *recipient == self_id {
                continue;
            }
            out.push(SenderKeyDistribution {
                sender: self_id,
                chain_id,
                physical_device_id: device.to_vec(),
                wrap: seal_to_member(*recipient, kem_public, &root)?,
            });
        }
        self.last_distribution = out.iter().map(|entry| entry.wrap.recipient).collect();
        Ok(out)
    }

    pub fn last_distribution(&self) -> &[MemberId] {
        &self.last_distribution
    }

    /// Accepts a distribution addressed to this client and installs or rotates
    /// the corresponding receiver chain.
    pub fn accept(
        &mut self,
        distribution: &SenderKeyDistribution,
        kem_secret: &ml_kem_768::DecapsulationKey,
    ) -> Result<(), GroupError> {
        let root = open_to_member(&distribution.wrap, kem_secret)?;
        if root.len() != 32 {
            return Err(GroupError::Malformed);
        }
        let mut rotation_root = [0_u8; 32];
        rotation_root.copy_from_slice(&root);
        let record = HeldSenderKey {
            sender: distribution.sender,
            chain_id: distribution.chain_id,
            physical_device_id: distribution.physical_device_id.clone(),
            rotation_root: rotation_root.to_vec(),
        };
        self.install_held(&record)
    }

    pub fn send(
        &mut self,
        sender: &MemberKeys,
        plaintext: &[u8],
    ) -> Result<GroupMessage, GroupError> {
        self.ensure_sender_chain()?;
        let context = self.context(&sender.x25519_public, &sender.kem_public);
        let chain = self
            .state
            .sender_chain_mut()
            .expect("sender chain installed above");
        let encrypted = chain.encrypt(plaintext, &context)?;
        Ok(GroupMessage {
            sender: sender.member_id,
            header_nonce: encrypted.header_nonce.as_bytes().to_vec(),
            enc_header: encrypted.enc_header,
            message_nonce: encrypted.message_nonce.as_bytes().to_vec(),
            ciphertext: encrypted.ciphertext,
        })
    }

    pub fn receive(
        &mut self,
        message: &GroupMessage,
        sender_x25519: &x25519::PublicKey,
        sender_kem: &ml_kem_768::EncapsulationKey,
    ) -> Result<Vec<u8>, GroupError> {
        let context = self.context(sender_x25519, sender_kem);
        let encrypted = message.to_encrypted()?;
        if self
            .state
            .receiver_chain(message.sender.as_bytes())
            .is_none()
        {
            return Err(GroupError::NoReceiverChain);
        }
        // Every chain held for that peer is tried, so a pass here cannot be an
        // artefact of picking the wrong one of several devices.
        Ok(self
            .state
            .decrypt_from(message.sender.as_bytes(), &encrypted, &context)?)
    }

    fn context(
        &self,
        sender_x25519: &x25519::PublicKey,
        sender_kem: &ml_kem_768::EncapsulationKey,
    ) -> SenderContext {
        SenderContext {
            sender_ik_x25519_pub: *sender_x25519,
            sender_ik_mlkem_pub: sender_kem.to_bytes().to_vec(),
            group_id: self.group_id.clone(),
            session_version: SESSION_VERSION,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum GroupError {
    #[error("group wire value is malformed")]
    Malformed,
    #[error("no receiver chain is held for that sender")]
    NoReceiverChain,
    #[error("group crypto failed: {0}")]
    Crypto(#[from] crypto::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rotation_that_skips_a_member_closes_their_retained_chain() {
        let ash = MemberKeys::generate("ash", "Ash");
        let wren = MemberKeys::generate("wren", "Wren");
        let brook = MemberKeys::generate("brook", "Brook");
        let group_id = b"atlas".to_vec();

        let mut sender = GroupSession::new(group_id.clone(), PhysicalDeviceId::random());
        let mut leaver = GroupSession::new(group_id.clone(), PhysicalDeviceId::random());
        let mut stayer = GroupSession::new(group_id.clone(), PhysicalDeviceId::random());

        let recipients = vec![
            (wren.member_id, &wren.kem_public),
            (brook.member_id, &brook.kem_public),
        ];
        for distribution in sender.distribute(ash.member_id, &recipients).unwrap() {
            if distribution.wrap.recipient == wren.member_id {
                leaver.accept(&distribution, &wren.kem_secret).unwrap();
            } else {
                stayer.accept(&distribution, &brook.kem_secret).unwrap();
            }
        }

        let before = sender.send(&ash, b"before the departure").unwrap();
        assert_eq!(
            leaver
                .receive(&before, &ash.x25519_public, &ash.kem_public)
                .unwrap(),
            b"before the departure"
        );

        // Wren leaves: rotate, then distribute to the remaining roster only.
        let rotated = sender.rotate_sender_chain().unwrap();
        assert_eq!(rotated, 1);
        let after_roster = vec![(brook.member_id, &brook.kem_public)];
        let distributions = sender.distribute(ash.member_id, &after_roster).unwrap();
        assert_eq!(distributions.len(), 1);
        assert!(distributions
            .iter()
            .all(|entry| entry.wrap.recipient != wren.member_id));
        stayer.accept(&distributions[0], &brook.kem_secret).unwrap();

        let after = sender.send(&ash, b"after the departure").unwrap();
        assert_eq!(
            stayer
                .receive(&after, &ash.x25519_public, &ash.kem_public)
                .unwrap(),
            b"after the departure"
        );
        assert!(
            leaver
                .receive(&after, &ash.x25519_public, &ash.kem_public)
                .is_err(),
            "a rotated chain must not open for a member who was not re-keyed"
        );
    }
}
