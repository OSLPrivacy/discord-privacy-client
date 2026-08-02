//! State and verification boundary for removable OSL components.
//!
//! This module deliberately has no dependency on encryption, key management,
//! or carrier decoding. The word-bank carrier is represented as a permanent
//! base component and cannot be removed.

use minisign_verify::{PublicKey, Signature};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const STATE_FILE: &str = "components.json";
const PAYLOAD_DIRECTORY: &str = "component-payloads";
const MAX_STATE_BYTES: u64 = 64 * 1024;
// This is the updater public key from `tauri.conf.json`, encoded exactly as
// Tauri's updater config encodes it. Component packages share that release key
// so optional installs do not introduce separate key management.
const UPDATER_PUBLIC_KEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDNCNkFFNDczOTg1OEU4RDQKUldUVTZGaVljK1JxTy9QeXZQZGhwTGpiK2lwODg1MlQySVREUVdiY0pHZlNBOWtEemVkeklSUFoK";

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ComponentId(String);

impl ComponentId {
    pub const WORD_BANK_CARRIER: &'static str = "word-bank-carrier";

    pub fn parse(value: impl Into<String>) -> Result<Self, ComponentError> {
        let value = value.into();
        let valid_named = matches!(
            value.as_str(),
            "word-bank-carrier"
                | "local-cover-model"
                | "tor-proxy"
                | "mullvad-integration"
                | "autoscrub-module"
        );
        let valid_adapter = value
            .strip_prefix("service-adapter/")
            .is_some_and(valid_service_id);
        if valid_named || valid_adapter {
            Ok(Self(value))
        } else {
            Err(ComponentError::InvalidId)
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    fn is_base(&self) -> bool {
        self.0 == Self::WORD_BANK_CARRIER
    }
}

fn valid_service_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ComponentArtifact {
    pub id: ComponentId,
    pub payload: Vec<u8>,
    pub download_bytes: u64,
    pub installed_bytes: u64,
    /// Base64-encoded UTF-8 minisign signature, as used by the Tauri updater.
    pub signature: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ComponentStatus {
    pub id: ComponentId,
    pub installed: bool,
    pub removable: bool,
    pub download_bytes: u64,
    pub installed_bytes: u64,
}

#[derive(Default, Serialize, Deserialize)]
struct ComponentState {
    installed: BTreeMap<ComponentId, InstalledComponent>,
}

#[derive(Serialize, Deserialize)]
struct InstalledComponent {
    download_bytes: u64,
    installed_bytes: u64,
}

pub trait ArtifactVerifier {
    fn verify(&self, payload: &[u8], signature: &str) -> Result<(), ComponentError>;
}

pub struct MinisignArtifactVerifier;

impl ArtifactVerifier for MinisignArtifactVerifier {
    fn verify(&self, payload: &[u8], signature: &str) -> Result<(), ComponentError> {
        let public_key_text = decode_base64_text(UPDATER_PUBLIC_KEY)?;
        let public_key =
            PublicKey::decode(&public_key_text).map_err(|_| ComponentError::InvalidSignature)?;
        let signature_text = decode_base64_text(signature)?;
        let signature =
            Signature::decode(&signature_text).map_err(|_| ComponentError::InvalidSignature)?;
        public_key
            .verify(payload, &signature, true)
            .map_err(|_| ComponentError::InvalidSignature)
    }
}

pub struct ComponentStore {
    root: PathBuf,
}

impl ComponentStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn list(&self) -> Result<Vec<ComponentStatus>, ComponentError> {
        let state = self.read_state()?;
        let base = ComponentStatus {
            id: ComponentId::parse(ComponentId::WORD_BANK_CARRIER)?,
            installed: true,
            removable: false,
            download_bytes: 0,
            installed_bytes: 6_970,
        };
        let mut components = vec![base];
        for (id, installed) in state.installed {
            if self.payload_path(&id).is_file() {
                components.push(ComponentStatus {
                    id,
                    installed: true,
                    removable: true,
                    download_bytes: installed.download_bytes,
                    installed_bytes: installed.installed_bytes,
                });
            }
        }
        Ok(components)
    }

    pub fn install(&self, artifact: ComponentArtifact) -> Result<(), ComponentError> {
        self.install_with_verifier(artifact, &MinisignArtifactVerifier)
    }

    pub fn install_with_verifier(
        &self,
        artifact: ComponentArtifact,
        verifier: &impl ArtifactVerifier,
    ) -> Result<(), ComponentError> {
        if artifact.id.is_base() {
            return Err(ComponentError::BaseComponent);
        }
        let payload_bytes =
            u64::try_from(artifact.payload.len()).map_err(|_| ComponentError::SizeMismatch)?;
        if artifact.download_bytes != payload_bytes || artifact.installed_bytes != payload_bytes {
            return Err(ComponentError::SizeMismatch);
        }
        verifier.verify(&artifact.payload, &artifact.signature)?;
        let payload_path = self.payload_path(&artifact.id);
        crate::atomic_file::write_recoverable(
            &payload_path,
            &artifact.payload,
            "component payload",
        )
        .map_err(ComponentError::Storage)?;
        let mut state = self.read_state()?;
        state.installed.insert(
            artifact.id,
            InstalledComponent {
                download_bytes: artifact.download_bytes,
                installed_bytes: artifact.installed_bytes,
            },
        );
        self.write_state(&state)
    }

    pub fn remove(&self, id: &ComponentId) -> Result<(), ComponentError> {
        if id.is_base() {
            return Err(ComponentError::BaseComponent);
        }
        let mut state = self.read_state()?;
        if !state.installed.contains_key(id) {
            return Err(ComponentError::NotInstalled);
        }
        match std::fs::remove_file(self.payload_path(id)) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => {
                return Err(ComponentError::Storage(
                    "component payload could not be removed".to_owned(),
                ))
            }
        }
        state.installed.remove(id);
        self.write_state(&state)
    }

    fn state_path(&self) -> PathBuf {
        self.root.join(STATE_FILE)
    }

    fn payload_path(&self, id: &ComponentId) -> PathBuf {
        // IDs are validated before construction, so the namespace separator is
        // data, not a path traversal opportunity.
        self.root.join(PAYLOAD_DIRECTORY).join(id.as_str())
    }

    fn read_state(&self) -> Result<ComponentState, ComponentError> {
        let Some(bytes) = crate::atomic_file::read_recoverable_bounded(
            &self.state_path(),
            MAX_STATE_BYTES,
            "component state",
        )
        .map_err(ComponentError::Storage)?
        else {
            return Ok(ComponentState::default());
        };
        let state: ComponentState = serde_json::from_slice(&bytes)
            .map_err(|_| ComponentError::Storage("component state is invalid".to_owned()))?;
        for id in state.installed.keys() {
            if ComponentId::parse(id.as_str()).is_err() || id.is_base() {
                return Err(ComponentError::Storage(
                    "component state contains an invalid component".to_owned(),
                ));
            }
        }
        Ok(state)
    }

    fn write_state(&self, state: &ComponentState) -> Result<(), ComponentError> {
        let bytes = serde_json::to_vec(state).map_err(|_| {
            ComponentError::Storage("component state could not be encoded".to_owned())
        })?;
        crate::atomic_file::write_recoverable(&self.state_path(), &bytes, "component state")
            .map_err(ComponentError::Storage)
    }
}

fn decode_base64_text(value: &str) -> Result<String, ComponentError> {
    use base64::Engine as _;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(value)
        .map_err(|_| ComponentError::InvalidSignature)?;
    String::from_utf8(bytes).map_err(|_| ComponentError::InvalidSignature)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ComponentError {
    InvalidId,
    InvalidSignature,
    SizeMismatch,
    BaseComponent,
    NotInstalled,
    Storage(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    struct AcceptOnlySigned;
    impl ArtifactVerifier for AcceptOnlySigned {
        fn verify(&self, _payload: &[u8], signature: &str) -> Result<(), ComponentError> {
            (signature == "signed")
                .then_some(())
                .ok_or(ComponentError::InvalidSignature)
        }
    }

    fn artifact(id: &str, signature: &str) -> ComponentArtifact {
        ComponentArtifact {
            id: ComponentId::parse(id).unwrap(),
            payload: b"component bytes".to_vec(),
            download_bytes: 15,
            installed_bytes: 15,
            signature: signature.to_owned(),
        }
    }

    #[test]
    fn install_then_remove_persists_real_on_disk_state_and_refuses_unsigned_artifacts() {
        let directory = tempfile::tempdir().unwrap();
        let store = ComponentStore::new(directory.path());
        let verifier = AcceptOnlySigned;
        let id = ComponentId::parse("tor-proxy").unwrap();

        assert_eq!(
            store.install_with_verifier(artifact("tor-proxy", "unsigned"), &verifier),
            Err(ComponentError::InvalidSignature)
        );
        assert!(!store.payload_path(&id).exists());

        store
            .install_with_verifier(artifact("tor-proxy", "signed"), &verifier)
            .unwrap();
        assert!(store.payload_path(&id).is_file());
        assert!(store
            .list()
            .unwrap()
            .iter()
            .any(|item| item.id == id && item.installed));

        store.remove(&id).unwrap();
        assert!(!store.payload_path(&id).exists());
        assert!(!store.list().unwrap().iter().any(|item| item.id == id));
        assert_eq!(
            store.remove(&ComponentId::parse("word-bank-carrier").unwrap()),
            Err(ComponentError::BaseComponent)
        );
    }

    #[test]
    fn production_verifier_rejects_an_unsigned_artifact() {
        let verifier = MinisignArtifactVerifier;
        assert_eq!(
            verifier.verify(b"component bytes", "not a signature"),
            Err(ComponentError::InvalidSignature)
        );
    }
}
