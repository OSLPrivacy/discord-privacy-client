//! Per-member key material and the sealed wrapper used to hand a key to
//! exactly one remaining member.
//!
//! Re-keying is only meaningful if the new key actually reaches the members
//! who stay and no one else. Every distribution here is a real ML-KEM-768
//! encapsulation to one recipient's public key with the payload sealed under
//! the encapsulated shared secret, so "the leaver received 0 new keys" is a
//! statement about key material that was never wrapped for them, not about a
//! flag.

use crypto::{aead, ed25519, ml_kem_768, x25519};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::ids::MemberId;

/// One member's long-lived key material, as their own client holds it.
pub struct MemberKeys {
    pub member_id: MemberId,
    pub handle: String,
    pub display_name: String,
    pub identity_secret: ed25519::SecretKey,
    pub identity_public: ed25519::PublicKey,
    pub x25519_public: x25519::PublicKey,
    pub kem_secret: ml_kem_768::DecapsulationKey,
    pub kem_public: ml_kem_768::EncapsulationKey,
}

impl MemberKeys {
    pub fn generate(handle: &str, display_name: &str) -> Self {
        let (identity_secret, identity_public) = ed25519::generate_keypair();
        let (_x_secret, x25519_public) = x25519::generate_keypair();
        let (kem_secret, kem_public) = ml_kem_768::generate_keypair();
        Self {
            member_id: MemberId::from_identity_key(&identity_public),
            handle: handle.to_string(),
            display_name: display_name.to_string(),
            identity_secret,
            identity_public,
            x25519_public,
            kem_secret,
            kem_public,
        }
    }
}

/// A payload sealed to exactly one recipient.
///
/// `recipient` is recorded so a distribution list can be counted and audited:
/// a wrap for a departed member would be visible here, and there are none.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SealedToMember {
    pub recipient: MemberId,
    #[serde(with = "hex_bytes")]
    pub kem_ciphertext: Vec<u8>,
    #[serde(with = "hex_bytes")]
    pub nonce: Vec<u8>,
    #[serde(with = "hex_bytes")]
    pub ciphertext: Vec<u8>,
}

const WRAP_AD: &[u8] = b"OSL/place-departure/key-wrap/v1";

pub fn seal_to_member(
    recipient: MemberId,
    recipient_kem_public: &ml_kem_768::EncapsulationKey,
    payload: &[u8],
) -> Result<SealedToMember, crypto::Error> {
    let (kem_ciphertext, shared) = ml_kem_768::encapsulate(recipient_kem_public)?;
    let key = aead::Key::from_bytes(*shared.as_bytes());
    let nonce = crypto::random::random_nonce();
    let ciphertext = aead::seal(&key, &nonce, WRAP_AD, payload)?;
    Ok(SealedToMember {
        recipient,
        kem_ciphertext: kem_ciphertext.to_bytes().to_vec(),
        nonce: nonce.as_bytes().to_vec(),
        ciphertext,
    })
}

pub fn open_to_member(
    sealed: &SealedToMember,
    kem_secret: &ml_kem_768::DecapsulationKey,
) -> Result<Zeroizing<Vec<u8>>, crypto::Error> {
    let mut ct = [0_u8; ml_kem_768::CIPHERTEXT_SIZE];
    if sealed.kem_ciphertext.len() != ct.len() {
        return Err(crypto::Error::Internal(
            "place departure: malformed key wrap".into(),
        ));
    }
    ct.copy_from_slice(&sealed.kem_ciphertext);
    let shared = ml_kem_768::decapsulate(kem_secret, &ml_kem_768::Ciphertext::from_bytes(&ct))?;
    let key = aead::Key::from_bytes(*shared.as_bytes());
    let mut nonce_bytes = [0_u8; aead::NONCE_SIZE];
    if sealed.nonce.len() != nonce_bytes.len() {
        return Err(crypto::Error::Internal(
            "place departure: malformed key wrap nonce".into(),
        ));
    }
    nonce_bytes.copy_from_slice(&sealed.nonce);
    let plaintext = aead::open(
        &key,
        &aead::Nonce::from_bytes(nonce_bytes),
        WRAP_AD,
        &sealed.ciphertext,
    )?;
    Ok(Zeroizing::new(plaintext))
}

/// Hex serialization for byte fields so the report is readable and the check
/// can hash exactly what was on the wire.
pub mod hex_bytes {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(bytes: &Vec<u8>, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&hex::encode(bytes))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<u8>, D::Error> {
        let text = String::deserialize(deserializer)?;
        hex::decode(&text).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wrap_opens_only_for_its_recipient() {
        let keeper = MemberKeys::generate("ash", "Ash");
        let leaver = MemberKeys::generate("wren", "Wren");
        let secret = b"channel key bytes for the new epoch";

        let sealed = seal_to_member(keeper.member_id, &keeper.kem_public, secret).unwrap();
        assert_eq!(
            open_to_member(&sealed, &keeper.kem_secret).unwrap().to_vec(),
            secret.to_vec()
        );
        assert!(
            open_to_member(&sealed, &leaver.kem_secret).is_err(),
            "a wrap addressed to a remaining member must not open for the leaver"
        );
    }
}
