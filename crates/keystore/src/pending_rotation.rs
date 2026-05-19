//! Persisted pending-rotation record (forward-fix for the "burn ⇒
//! permanent not-a-recipient" bug).
//!
//! ## Why this exists
//!
//! The keyserver's `/v1/register` enforces owner-authenticated key
//! rotation: once a `user_id` has a row, changing its keys requires a
//! Case-C `rotation` proof whose `prev_sig` is an Ed25519 signature
//! over `ROT_MSG` made with the **currently-stored (old)** identity
//! key. A burn / fresh-start regenerates the identity and DESTROYS
//! the old Ed25519 secret, so the client can never produce `prev_sig`
//! afterwards → register returns 403 → the new key is never published
//! → peers keep encrypting to the dead old key.
//!
//! Fix: at burn time, while the OLD identity still exists, pre-sign
//! the rotation authorizing the NEW identity and persist that proof
//! (sealed, on disk). The register path then presents it as a Case-C
//! rotation, so a burn re-publishes correctly.
//!
//! Persistence mirrors [`crate::prekeys`] exactly: a sealed two-layer
//! JSON file (`pending_rotation.json`) alongside `identity.json`,
//! sealed under the active [`crate::sealer::Sealer`].
//!
//! ## Security note
//!
//! `new_ik_ed25519_pub` binds the stored proof to the identity it
//! authorizes. The register path MUST verify that the in-state
//! identity's Ed25519 public matches this field before presenting the
//! proof, so a stale proof (from an older burn) is never sent. The
//! OLD key authorized the rotation at burn time — the threat model is
//! unchanged from `build_rotation_request`.

use crate::client::KeyServerClient;
use crate::identity::Identity;
use crate::sealer::Sealer;
use crate::{Error, Result};
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// A pre-signed Case-C rotation proof persisted at burn time.
///
/// - `prev_ik_ed25519_pub` + `prev_sig` come straight from the
///   `RotationProof` minted by
///   [`KeyServerClient::build_rotation_request`] (OLD key authorizes
///   the change over `ROT_MSG`).
/// - `new_ik_ed25519_pub` is base64 of the NEW identity's Ed25519
///   public — the register path checks this against the in-state
///   identity so a stale proof is never presented.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PendingRotation {
    pub prev_ik_ed25519_pub: String,
    pub prev_sig: String,
    pub new_ik_ed25519_pub: String,
}

/// Mint a [`PendingRotation`] authorizing `new` from `old`.
///
/// All signing is delegated to
/// [`KeyServerClient::build_rotation_request`] (the byte-exact,
/// server-mirrored Case-C path) — this helper only extracts the
/// rotation proof and the new identity's Ed25519 public from the
/// request it builds.
pub fn pending_rotation_from(old: &Identity, new: &Identity) -> PendingRotation {
    let req = KeyServerClient::build_rotation_request(old, new);
    let rotation = req
        .rotation
        .expect("build_rotation_request always produces a Case-C rotation");
    PendingRotation {
        prev_ik_ed25519_pub: rotation.prev_ik_ed25519_pub,
        prev_sig: rotation.prev_sig,
        new_ik_ed25519_pub: req.ik_ed25519_pub,
    }
}

// ---- persistence (mirrors `prekeys.rs`) ----

#[derive(Serialize, Deserialize)]
struct PendingRotationOnDisk {
    version: u32,
    method: String,
    sealed_b64: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    insecure_banner: Option<String>,
}

pub fn save_pending_rotation(
    path: &Path,
    pending: &PendingRotation,
    sealer: &dyn Sealer,
) -> Result<()> {
    let inner = serde_json::to_vec(pending)?;
    let sealed = sealer.seal(&inner)?;
    let on_disk = PendingRotationOnDisk {
        version: 1,
        method: sealer.method_label().to_string(),
        sealed_b64: STANDARD.encode(&sealed),
        insecure_banner: if sealer.requires_insecure_banner() {
            Some(
                "INSECURE prototype storage — plain JSON, no TPM. \
                 v1 stable replaces with TPM-sealed blob; do NOT use \
                 with real users."
                    .to_string(),
            )
        } else {
            None
        },
    };
    let json = serde_json::to_vec_pretty(&on_disk)?;
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    std::fs::write(path, &json)?;
    Ok(())
}

/// Load the pending-rotation record. A missing file is `Ok(None)`
/// (no rotation is pending — the common case).
pub fn load_pending_rotation(
    path: &Path,
    sealer: &dyn Sealer,
) -> Result<Option<PendingRotation>> {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(Error::Io(e)),
    };
    let on_disk: PendingRotationOnDisk = serde_json::from_slice(&bytes)?;
    if on_disk.version != 1 {
        return Err(Error::BlobVersionMismatch {
            got: on_disk.version,
            expected: 1,
        });
    }
    if on_disk.method != sealer.method_label() {
        return Err(Error::BlobMethodMismatch {
            got: on_disk.method,
            expected: sealer.method_label().to_string(),
        });
    }
    let sealed = STANDARD.decode(&on_disk.sealed_b64)?;
    let inner = sealer.unseal(&sealed)?;
    let pending: PendingRotation = serde_json::from_slice(&inner)?;
    Ok(Some(pending))
}

/// Delete the pending-rotation record. A missing file is `Ok(())`
/// (idempotent — clearing an already-cleared proof is fine).
pub fn delete_pending_rotation(path: &Path) -> Result<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(Error::Io(e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::generate_identity;
    use crate::sealer::{MemorySealer, NoOpSealer};
    use crate::client::rot_msg;
    use crypto::ed25519;
    use tempfile::TempDir;

    #[test]
    fn save_load_round_trip_with_memory_sealer() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("pending_rotation.json");
        let sealer = MemorySealer::new();
        let old = generate_identity("alice".to_string());
        let new = generate_identity("alice".to_string());
        let original = pending_rotation_from(&old, &new);
        save_pending_rotation(&path, &original, &sealer).unwrap();
        let loaded = load_pending_rotation(&path, &sealer).unwrap().unwrap();
        assert_eq!(loaded, original);
    }

    #[test]
    fn load_missing_file_is_none() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("pending_rotation.json");
        let sealer = MemorySealer::new();
        assert!(load_pending_rotation(&path, &sealer).unwrap().is_none());
    }

    #[test]
    fn delete_missing_file_is_ok() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("pending_rotation.json");
        delete_pending_rotation(&path).unwrap();
        // And idempotent after a real delete.
        let sealer = MemorySealer::new();
        let old = generate_identity("bob".to_string());
        let new = generate_identity("bob".to_string());
        save_pending_rotation(&path, &pending_rotation_from(&old, &new), &sealer).unwrap();
        assert!(path.exists());
        delete_pending_rotation(&path).unwrap();
        assert!(!path.exists());
        delete_pending_rotation(&path).unwrap();
    }

    #[test]
    fn pending_rotation_from_binds_old_and_new_keys() {
        let old = generate_identity("carol".to_string());
        let new = generate_identity("carol".to_string());
        let p = pending_rotation_from(&old, &new);
        assert_eq!(
            p.prev_ik_ed25519_pub,
            STANDARD.encode(old.ed25519_public.as_bytes()),
            "prev key must be the OLD identity's ed25519 public"
        );
        assert_eq!(
            p.new_ik_ed25519_pub,
            STANDARD.encode(new.ed25519_public.as_bytes()),
            "new key must be the NEW identity's ed25519 public"
        );
        let sig_bytes = STANDARD.decode(&p.prev_sig).unwrap();
        assert_eq!(sig_bytes.len(), 64, "ed25519 signature is 64 bytes");
    }

    #[test]
    fn minted_prev_sig_verifies_under_old_key_over_rot_msg() {
        // Byte-exact server-contract guard: the persisted `prev_sig`
        // must verify under the OLD ed25519 public over the exact
        // `rot_msg(..)` bytes for the NEW identity. If this breaks,
        // the keyserver will 403 the presented proof.
        let old = generate_identity("dave".to_string());
        let new = generate_identity("dave".to_string());
        let p = pending_rotation_from(&old, &new);

        let new_x = STANDARD.encode(new.x25519_public.as_bytes());
        let new_ed = STANDARD.encode(new.ed25519_public.as_bytes());
        let new_mlkem = STANDARD.encode(new.mlkem_public_bytes);
        let new_ratchet = new
            .ratchet_initial_pub
            .as_ref()
            .map(|pk| STANDARD.encode(pk.as_bytes()));
        let prev_ed = STANDARD.encode(old.ed25519_public.as_bytes());

        let msg = rot_msg(
            &old.user_id,
            &prev_ed,
            &new_x,
            &new_ed,
            &new_mlkem,
            new_ratchet.as_deref(),
        );
        let mut sig_arr = [0u8; 64];
        sig_arr.copy_from_slice(&STANDARD.decode(&p.prev_sig).unwrap());
        let sig = ed25519::Signature::from_bytes(sig_arr);
        assert!(
            ed25519::verify(&old.ed25519_public, &msg, &sig).unwrap(),
            "minted prev_sig must verify under the OLD ed25519 public \
             over rot_msg for the NEW identity"
        );
    }

    #[test]
    fn noop_sealer_writes_insecure_banner() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("pending_rotation.json");
        let sealer = NoOpSealer::new();
        let old = generate_identity("erin".to_string());
        let new = generate_identity("erin".to_string());
        save_pending_rotation(&path, &pending_rotation_from(&old, &new), &sealer).unwrap();
        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(raw.contains("INSECURE prototype storage"));
    }
}
