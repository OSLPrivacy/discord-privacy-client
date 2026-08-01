//! Opaque local binding between one OSL identity and one service account.
//!
//! This module does not prove ownership of an account at the remote service.
//! It only proves that the account row belongs to the exact locally loaded OSL
//! identity whose public key tuple is committed here. Platform-account proofs
//! and sender attribution must bind to this value in their own layers.

use sha2::{Digest, Sha256};

use crate::models::ServiceKind;

const PUBLIC_KEYS_DOMAIN: &[u8] = b"OSL-HUB/account-service-public-keys/v1";
const AUTHORITY_DOMAIN: &[u8] = b"OSL-HUB/account-service-identity-authority/v1";

/// Issued only after the protected service registry has authenticated the
/// exact owner/service/account row. It intentionally has no public constructor
/// and does not implement `Deserialize`.
#[derive(Eq, PartialEq)]
pub struct AccountServiceIdentityAuthority {
    owner_osl_user_id: String,
    service_id: &'static str,
    local_account_id: String,
    identity_public_keys_sha256: [u8; 32],
    binding_sha256: [u8; 32],
}

impl AccountServiceIdentityAuthority {
    pub fn owner_osl_user_id(&self) -> &str {
        &self.owner_osl_user_id
    }

    pub fn service_id(&self) -> &str {
        self.service_id
    }

    pub fn local_account_id(&self) -> &str {
        &self.local_account_id
    }

    pub fn identity_public_keys_sha256(&self) -> [u8; 32] {
        self.identity_public_keys_sha256
    }

    pub fn binding_sha256(&self) -> [u8; 32] {
        self.binding_sha256
    }

    pub(crate) fn issue(
        identity: &keystore::Identity,
        service: ServiceKind,
        local_account_id: &str,
    ) -> Result<Self, String> {
        let canonical_owner = keystore::native_user_id(identity);
        if identity.user_id != canonical_owner {
            return Err("active OSL identity authority is invalid".to_owned());
        }
        if crypto::x25519::derive_public(&identity.x25519_secret) != identity.x25519_public
            || crypto::ed25519::derive_public(&identity.ed25519_secret) != identity.ed25519_public
        {
            return Err("active OSL identity authority is invalid".to_owned());
        }
        match (
            identity.ratchet_initial_secret.as_ref(),
            identity.ratchet_initial_pub,
        ) {
            (None, None) => {}
            (Some(secret), Some(public)) if crypto::x25519::derive_public(secret) == public => {}
            _ => return Err("active OSL identity authority is invalid".to_owned()),
        }

        let service_id = canonical_service_id(service);
        let identity_public_keys_sha256 = public_keys_digest(identity);
        let mut binding = Sha256::new();
        binding.update(AUTHORITY_DOMAIN);
        hash_field(
            &mut binding,
            b"owner_osl_user_id",
            canonical_owner.as_bytes(),
        );
        hash_field(
            &mut binding,
            b"identity_public_keys_sha256",
            &identity_public_keys_sha256,
        );
        hash_field(&mut binding, b"service_id", service_id.as_bytes());
        hash_field(
            &mut binding,
            b"local_account_id",
            local_account_id.as_bytes(),
        );

        Ok(Self {
            owner_osl_user_id: canonical_owner,
            service_id,
            local_account_id: local_account_id.to_owned(),
            identity_public_keys_sha256,
            binding_sha256: binding.finalize().into(),
        })
    }
}

fn public_keys_digest(identity: &keystore::Identity) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(PUBLIC_KEYS_DOMAIN);
    hash_field(
        &mut hash,
        b"x25519_public",
        identity.x25519_public.as_bytes(),
    );
    hash_field(
        &mut hash,
        b"ed25519_public",
        identity.ed25519_public.as_bytes(),
    );
    hash_field(&mut hash, b"mlkem768_public", &identity.mlkem_public_bytes);
    match identity.ratchet_initial_pub {
        Some(public) => hash_field(&mut hash, b"ratchet_initial_public", public.as_bytes()),
        None => hash_field(&mut hash, b"ratchet_initial_public", &[]),
    }
    hash.finalize().into()
}

fn hash_field(hash: &mut Sha256, label: &[u8], value: &[u8]) {
    let label_len = u32::try_from(label.len()).expect("authority field label is bounded");
    let value_len = u32::try_from(value.len()).expect("authority field value is bounded");
    hash.update(label_len.to_be_bytes());
    hash.update(label);
    hash.update(value_len.to_be_bytes());
    hash.update(value);
}

fn canonical_service_id(service: ServiceKind) -> &'static str {
    match service {
        ServiceKind::Discord => "discord",
        ServiceKind::Telegram => "telegram",
        ServiceKind::WhatsApp => "whatsapp",
        ServiceKind::Instagram => "instagram",
        ServiceKind::Snapchat => "snapchat",
        ServiceKind::Email => "email",
        ServiceKind::X => "x",
        ServiceKind::Signal => "signal",
        ServiceKind::Slack => "slack",
        ServiceKind::Linkedin => "linkedin",
        ServiceKind::Teams => "teams",
        ServiceKind::Messenger => "messenger",
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;
    use crate::core_bridge::HubCoreState;
    use crate::services::{service_kind_from_id, ServiceRegistryState};

    const TEST_KEY: [u8; 32] = [0x91; 32];

    fn temporary_root(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "osl-a1-account-authority-{label}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    fn load_identity(core: &HubCoreState, identity: keystore::Identity) {
        *core.osl.identity.lock().unwrap() = Some(identity);
    }

    #[test]
    fn account_service_identity_authority_contract() {
        let _serial = crate::GLOBAL_KEYSTORE_TEST_LOCK.lock().unwrap();
        ipc::main_password::set_file_storage_key(Some(TEST_KEY));

        let root = temporary_root("primary");
        fs::create_dir(&root).unwrap();
        let registry_path = root.join("services.json");
        let identity = keystore::native_identity_from_entropy([0x31; 16]);
        let recovered = keystore::native_identity_from_entropy([0x31; 16]);
        let core = HubCoreState::default();
        load_identity(&core, identity.clone());
        let state = ServiceRegistryState::load(registry_path.clone());
        let account = state
            .create_for_owner(
                &identity.user_id,
                ServiceKind::Discord,
                "not authority".to_owned(),
            )
            .unwrap();

        let issued = state
            .require_identity_authority(&core, ServiceKind::Discord, &account.id)
            .unwrap();
        load_identity(&core, recovered.clone());
        let reloaded = ServiceRegistryState::load(registry_path.clone());
        let recovered_issued = reloaded
            .require_identity_authority(&core, ServiceKind::Discord, &account.id)
            .unwrap();
        assert!(issued == recovered_issued);
        assert_eq!(issued.owner_osl_user_id(), identity.user_id);
        assert_eq!(issued.service_id(), "discord");
        assert_eq!(issued.local_account_id(), account.id);
        assert_ne!(issued.identity_public_keys_sha256(), [0; 32]);
        assert_ne!(issued.binding_sha256(), [0; 32]);

        // Mutation 1: a different complete key tuple cannot borrow the real
        // identity's routing id.
        let mut substituted_keys = keystore::native_identity_from_entropy([0x32; 16]);
        substituted_keys.user_id = identity.user_id.clone();
        load_identity(&core, substituted_keys);
        assert!(reloaded
            .require_identity_authority(&core, ServiceKind::Discord, &account.id)
            .is_err());

        // Mutation 2: a genuine different OSL owner cannot authorize the
        // first owner's local account id.
        let other_owner = keystore::native_identity_from_entropy([0x33; 16]);
        load_identity(&core, other_owner);
        assert!(reloaded
            .require_identity_authority(&core, ServiceKind::Discord, &account.id)
            .is_err());

        // Mutation 3: service ids are exact enums; neither a different
        // service nor a noncanonical alias can select the Discord row.
        load_identity(&core, identity.clone());
        assert!(reloaded
            .require_identity_authority(&core, ServiceKind::Instagram, &account.id)
            .is_err());
        assert_eq!(service_kind_from_id("Discord"), None);
        assert_eq!(service_kind_from_id("discord.com"), None);

        // Mutation 4: an ownerless legacy row is quarantined rather than
        // claimed by the currently loaded identity.
        let legacy_root = temporary_root("legacy");
        fs::create_dir(&legacy_root).unwrap();
        let legacy_path = legacy_root.join("services.json");
        fs::write(
            &legacy_path,
            br#"{"version":1,"accounts":[{"serviceId":"discord","id":"acct-legacy","label":"Legacy"}]}"#,
        )
        .unwrap();
        let legacy = ServiceRegistryState::load(legacy_path);
        load_identity(&core, identity.clone());
        assert!(legacy
            .require_identity_authority(&core, ServiceKind::Discord, "acct-legacy")
            .is_err());

        // Mutation 5: a Scheme-1-shaped or caller-authored routing id cannot
        // downgrade/fall back to the current legacy identity authority.
        let mut downgraded = recovered.clone();
        downgraded.user_id = "osl1_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_owned();
        load_identity(&core, downgraded);
        assert!(reloaded
            .require_identity_authority(&core, ServiceKind::Discord, &account.id)
            .is_err());

        // Mutation 6: copying a protected registry does not transfer its
        // owner binding to a different recovery entropy or identity slot.
        let copied_root = temporary_root("copied");
        fs::create_dir(&copied_root).unwrap();
        let copied_path = copied_root.join("services.json");
        fs::copy(&registry_path, &copied_path).unwrap();
        let copied = ServiceRegistryState::load(copied_path);
        let switched_identity = keystore::native_identity_from_entropy([0x34; 16]);
        load_identity(&core, switched_identity);
        assert!(copied
            .require_identity_authority(&core, ServiceKind::Discord, &account.id)
            .is_err());

        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(legacy_root).unwrap();
        fs::remove_dir_all(copied_root).unwrap();
        ipc::main_password::set_file_storage_key(None);
    }
}
