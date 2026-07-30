//! Phase 9-A3: `sender_key_state.json`.
//!
//! Scope-keyed map of `SenderKeyStateOnDisk` entries. One row per
//! group/server scope that the local user has ever participated in
//! as either sender or receiver. The crate is responsible only for
//! at-rest serialization; rotation triggers, SKDM dispatch, and
//! membership-change detection live in `commands.rs`.
//!
//! Same OSL-ENC1 envelope as `peer_map.json` / `burned_scopes.json` /
//! `whitelist_state.json`. Plain-JSON fallback when no main password
//! is installed; encryption activates transparently once the user
//! sets one.

use crypto::sender_keys::SenderKeyStateOnDisk;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct SenderKeyStateFile {
    #[serde(default)]
    pub version: u32,
    /// Key = `scope.storage_key()` (e.g. `"gc:1234"`,
    /// `"server_channel:9876:5432"`).
    #[serde(default)]
    pub states: HashMap<String, SenderKeyStateOnDisk>,
}

pub fn load_sender_key_state(path: &Path) -> SenderKeyStateFile {
    let Ok(blob) = std::fs::read(path) else {
        return SenderKeyStateFile::default();
    };
    let plain = match crate::main_password::maybe_decrypt(&blob) {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!(error = %e, "OSL: load sender_key_state.json decrypt failed");
            return SenderKeyStateFile::default();
        }
    };
    serde_json::from_slice(&plain).unwrap_or_default()
}

pub fn write_sender_key_state(path: &Path, file: &SenderKeyStateFile) -> Result<(), String> {
    let body = serde_json::to_vec_pretty(file)
        .map_err(|e| format!("OSL: serialize sender_key_state: {e}"))?;
    let out = crate::main_password::maybe_encrypt(&body)
        .map_err(|e| format!("OSL: encrypt sender_key_state: {e}"))?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, &out).map_err(|e| format!("OSL: write {}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("OSL: rename {}: {e}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crypto::sender_keys::{
        PhysicalDeviceId, SenderContext, SenderKeyState, SESSION_VERSION_V1,
    };
    use crypto::x25519;
    use tempfile::tempdir;

    struct FileKeyReset;

    impl Drop for FileKeyReset {
        fn drop(&mut self) {
            crate::main_password::set_file_storage_key(None);
        }
    }

    fn device(byte: u8) -> PhysicalDeviceId {
        PhysicalDeviceId::from_bytes([byte; 32]).unwrap()
    }

    fn ctx(seed: u8) -> SenderContext {
        SenderContext {
            sender_ik_x25519_pub: x25519::PublicKey::from_bytes([seed; 32]),
            sender_ik_mlkem_pub: vec![seed; 1184],
            group_id: b"gc:two-device".to_vec(),
            session_version: SESSION_VERSION_V1,
        }
    }

    #[test]
    fn v5_device_bound_sender_keys_do_not_desync_across_two_devices() {
        let _reset = FileKeyReset;
        crate::main_password::set_file_storage_key(Some([0x51; 32]));
        let peer = b"same-peer-account".to_vec();
        let mut device_a = SenderKeyState::new();
        let mut device_b = SenderKeyState::new();
        device_a
            .install_sender_for_physical_device(device(0xa1))
            .unwrap();
        device_b
            .install_sender_for_physical_device(device(0xb2))
            .unwrap();

        let mut receiver = SenderKeyState::new();
        let sender_a = device_a.sender_chain().unwrap();
        receiver
            .install_receiver(
                peer.clone(),
                sender_a.current_chain_id(),
                &sender_a.rotation_root_bytes(),
                sender_a.physical_device_id(),
            )
            .unwrap();
        let sender_b = device_b.sender_chain().unwrap();
        receiver
            .install_receiver(
                peer.clone(),
                sender_b.current_chain_id(),
                &sender_b.rotation_root_bytes(),
                sender_b.physical_device_id(),
            )
            .unwrap();

        let mut file = SenderKeyStateFile {
            version: 1,
            ..Default::default()
        };
        file.states.insert(
            "gc:two-device".to_owned(),
            SenderKeyStateOnDisk::from(&receiver),
        );
        let dir = tempdir().unwrap();
        let path = dir.path().join("sender_key_state.json");
        write_sender_key_state(&path, &file).unwrap();

        let loaded = load_sender_key_state(&path);
        let mut loaded_receiver: SenderKeyState = loaded
            .states
            .get("gc:two-device")
            .unwrap()
            .clone()
            .try_into()
            .unwrap();
        let context = ctx(0x51);
        let from_a = device_a
            .encrypt(b"from physical device a", &context)
            .unwrap();
        let from_b = device_b
            .encrypt(b"from physical device b", &context)
            .unwrap();

        assert_eq!(
            loaded_receiver
                .decrypt_from(&peer, &from_a, &context)
                .unwrap(),
            b"from physical device a"
        );
        assert_eq!(
            loaded_receiver
                .decrypt_from(&peer, &from_b, &context)
                .unwrap(),
            b"from physical device b"
        );

        let mut collapsed = loaded.states.get("gc:two-device").unwrap().clone();
        let first_device = collapsed.receivers[0].1.physical_device_id_b64.clone();
        collapsed.receivers[1].1.physical_device_id_b64 = first_device;
        let mut collapsed_receiver: SenderKeyState = collapsed.try_into().unwrap();
        let second_from_b = device_b
            .encrypt(b"second message from physical device b", &context)
            .unwrap();
        assert!(
            collapsed_receiver
                .decrypt_from(&peer, &second_from_b, &context)
                .is_err(),
            "collapsing physical_device_id must not decrypt the second device"
        );
    }
}
