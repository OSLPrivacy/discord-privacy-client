//! Opaque local identities used by the departure engine.
//!
//! Every identifier here is either CSPRNG output or a digest of key material.
//! None of them is derived from a display name, so renaming a place, a channel
//! or a role cannot move a member between key domains or change who holds
//! authority.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sha2::{Digest, Sha256};

/// A member's stable roster reference: the SHA-256 digest of their ed25519
/// identity key.
///
/// Binding the roster reference to the signing key is what lets a client
/// verify a received self-removal against *its own* roster entry instead of a
/// public key nominated by the wire.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct MemberId([u8; 32]);

impl MemberId {
    pub fn from_identity_key(identity_key: &crypto::ed25519::PublicKey) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(b"OSL/place-departure/member-id/v1");
        hasher.update(identity_key.as_bytes());
        let digest = hasher.finalize();
        let mut out = [0_u8; 32];
        out.copy_from_slice(&digest);
        Self(out)
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    pub fn hex(&self) -> String {
        hex::encode(self.0)
    }
}

/// A role's stable reference.
///
/// Derived from the place handle and a role key that never appears in the
/// interface, so the label is free to change without producing a different
/// role id. `RoleGrant::label` is display text and is deliberately not an
/// input here.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RoleId([u8; 16]);

impl RoleId {
    pub fn derive(place_handle: &str, role_key: &str) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(b"OSL/place-departure/role-id/v1");
        hasher.update((place_handle.len() as u32).to_be_bytes());
        hasher.update(place_handle.as_bytes());
        hasher.update((role_key.len() as u32).to_be_bytes());
        hasher.update(role_key.as_bytes());
        let digest = hasher.finalize();
        let mut out = [0_u8; 16];
        out.copy_from_slice(&digest[..16]);
        Self(out)
    }

    pub fn hex(&self) -> String {
        hex::encode(self.0)
    }
}

/// A place's local handle.
///
/// This is the client's own reference for one group or enclave. It is never
/// sent anywhere: the enclave's replicated identity is its `SpaceId`, and the
/// group's is its CSPRNG group id.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct PlaceHandle(String);

impl PlaceHandle {
    pub fn new(handle: impl Into<String>) -> Self {
        Self(handle.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for PlaceHandle {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

macro_rules! hex_serde {
    ($type:ty, $len:expr, $name:literal) => {
        impl Serialize for $type {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.serialize_str(&hex::encode(self.0))
            }
        }

        impl<'de> Deserialize<'de> for $type {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let text = String::deserialize(deserializer)?;
                let bytes = hex::decode(&text).map_err(serde::de::Error::custom)?;
                if bytes.len() != $len {
                    return Err(serde::de::Error::custom(format!(
                        "{} must be {} hex bytes, got {}",
                        $name,
                        $len,
                        bytes.len()
                    )));
                }
                let mut out = [0_u8; $len];
                out.copy_from_slice(&bytes);
                Ok(Self(out))
            }
        }
    };
}

hex_serde!(MemberId, 32, "member id");
hex_serde!(RoleId, 16, "role id");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_role_id_ignores_the_label_and_a_member_id_follows_the_identity_key() {
        let first = RoleId::derive("quarry-enclave", "warden");
        let same = RoleId::derive("quarry-enclave", "warden");
        let other_place = RoleId::derive("foundry-group", "warden");
        assert_eq!(first, same, "a role id is a function of place + role key");
        assert_ne!(first, other_place, "role ids do not span places");

        let (_secret, public) = crypto::ed25519::generate_keypair();
        let (_other_secret, other_public) = crypto::ed25519::generate_keypair();
        assert_eq!(
            MemberId::from_identity_key(&public),
            MemberId::from_identity_key(&public)
        );
        assert_ne!(
            MemberId::from_identity_key(&public),
            MemberId::from_identity_key(&other_public)
        );
    }
}
