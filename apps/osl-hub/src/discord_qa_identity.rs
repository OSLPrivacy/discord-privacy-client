//! Device-bound identity bootstrap for the disposable Discord VM QA build.
//!
//! This module is absent from production binaries. It replaces password entry
//! with a random 256-bit storage key sealed by the same TPM / operating-system
//! credential mechanism used for local identities. No password, fixed secret,
//! network secret, or renderer-callable bootstrap surface exists.

use std::path::Path;

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use keystore::Sealer;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use crate::core_bridge::HubCoreState;
use crate::security::{self, HubSecurityState};

const DEVICE_KEY_FILENAME: &str = "discord-qa-device-key.v1.json";
const DEVICE_KEY_VERSION: u32 = 1;
const DEVICE_KEY_BYTES: usize = 32;
const MAX_DEVICE_KEY_RECORD_BYTES: u64 = 16 * 1024;
const PUBLIC_OFFER_FILENAME: &str = "discord-qa-offer.v1.json";
const PEER_OFFER_FILENAME: &str = "discord-qa-peer-offer.v1.json";
const PAIRING_STATUS_FILENAME: &str = "discord-qa-pairing-status.v1.json";
const PUBLIC_OFFER_VERSION: u32 = 1;
const MAX_PUBLIC_OFFER_BYTES: u64 = 16 * 1024;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DeviceKeyRecord {
    version: u32,
    sealer: String,
    sealed_key_b64: String,
}

/// Public, signed identity material exchanged by the two-VM QA controller.
/// This record contains exactly the same public values as a normal friend-code
/// export. It never contains a storage key, recovery phrase, or private key.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PublicOffer {
    version: u32,
    friend_code: String,
    osl_user_id: String,
    safety_number: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PairingStatus {
    version: u32,
    peer_person_id: String,
    peer_osl_user_id: String,
    peer_safety_number: String,
    peer_offer_sha256: String,
    verified: bool,
}

/// Install the QA file-storage key before any encrypted account state loads.
/// An existing production password marker is never bypassed.
pub fn install_device_bound_storage_key(account_dir: &Path) -> Result<(), String> {
    if account_dir.join("password_marker.json").exists() {
        return Err(
            "Discord QA requires a disposable passwordless profile; an existing password-protected profile was not opened"
                .to_owned(),
        );
    }
    let sealer = crate::password_lifecycle::persistent_sealer()?;
    let key = load_or_create_key(account_dir, sealer.as_ref())?;
    ipc::main_password::set_file_storage_key(Some(*key));
    Ok(())
}

/// Create the disposable service-neutral identity exactly once. Its private
/// keys remain sealed by TPM / OS credentials through the production identity
/// lifecycle; the recovery phrase is intentionally never logged or surfaced.
pub fn ensure_disposable_identity(core: &HubCoreState) -> Result<(), String> {
    if crate::password_lifecycle::readiness(core).identity_loaded {
        return Ok(());
    }
    let result = crate::password_lifecycle::create_native_identity(core)?;
    drop(result);
    Ok(())
}

/// Publish this device's signed public offer and, when the controller has
/// installed the other VM's offer at the fixed peer path, import and verify it
/// through the production People security functions. The operation is
/// deliberately file-only, bounded, and idempotent; no renderer or network
/// surface can select a path or supply key material.
pub fn publish_and_consume_pairing(
    account_dir: &Path,
    core: &HubCoreState,
    security_state: &HubSecurityState,
) -> Result<(), String> {
    let exported = security::export_friend_code(core)?;
    let offer = PublicOffer {
        version: PUBLIC_OFFER_VERSION,
        friend_code: exported.friend_code,
        osl_user_id: exported.osl_user_id,
        safety_number: exported.safety_number,
    };
    let encoded = encode_public_offer(&offer)?;
    crate::atomic_file::write_recoverable(
        &account_dir.join(PUBLIC_OFFER_FILENAME),
        &encoded,
        "Discord QA public offer",
    )?;

    let peer_path = account_dir.join(PEER_OFFER_FILENAME);
    let Some(peer_bytes) = crate::atomic_file::read_recoverable_bounded(
        &peer_path,
        MAX_PUBLIC_OFFER_BYTES,
        "Discord QA peer offer",
    )?
    else {
        return Ok(());
    };
    let peer_offer = decode_public_offer(&peer_bytes)?;
    let added = security::add_friend_code(
        core,
        security_state,
        peer_offer.friend_code,
        Some("Discord QA peer".to_owned()),
    )?;
    if added.osl_user_id != peer_offer.osl_user_id
        || added.safety_number != peer_offer.safety_number
    {
        return Err("Discord QA peer offer metadata does not match its signed code".to_owned());
    }
    let verified = security::verify_friend_safety_number(
        core,
        security_state,
        added.person_id.clone(),
        added.safety_number.clone(),
    )?;
    if !verified.safety_number_verified || verified.pending_key_change {
        return Err("Discord QA peer offer was not verified to stable keys".to_owned());
    }

    let status = PairingStatus {
        version: PUBLIC_OFFER_VERSION,
        peer_person_id: verified.person_id,
        peer_osl_user_id: verified.osl_user_id,
        peer_safety_number: verified.safety_number,
        peer_offer_sha256: hex_digest(&peer_bytes),
        verified: true,
    };
    let status_bytes = serde_json::to_vec(&status)
        .map_err(|_| "Discord QA pairing status could not be encoded".to_owned())?;
    crate::atomic_file::write_recoverable(
        &account_dir.join(PAIRING_STATUS_FILENAME),
        &status_bytes,
        "Discord QA pairing status",
    )
}

fn encode_public_offer(offer: &PublicOffer) -> Result<Vec<u8>, String> {
    validate_public_offer(offer)?;
    let bytes = serde_json::to_vec(offer)
        .map_err(|_| "Discord QA public offer could not be encoded".to_owned())?;
    if bytes.len() as u64 > MAX_PUBLIC_OFFER_BYTES {
        return Err("Discord QA public offer exceeds its size limit".to_owned());
    }
    Ok(bytes)
}

fn decode_public_offer(bytes: &[u8]) -> Result<PublicOffer, String> {
    if bytes.len() as u64 > MAX_PUBLIC_OFFER_BYTES {
        return Err("Discord QA peer offer exceeds its size limit".to_owned());
    }
    let offer: PublicOffer =
        serde_json::from_slice(bytes).map_err(|_| "Discord QA peer offer is invalid".to_owned())?;
    validate_public_offer(&offer)?;
    Ok(offer)
}

fn validate_public_offer(offer: &PublicOffer) -> Result<(), String> {
    if offer.version != PUBLIC_OFFER_VERSION
        || offer.friend_code.len() < 16
        || offer.friend_code.len() > 8 * 1024
        || !offer.friend_code.starts_with("OSLFR1.")
        || offer.osl_user_id.is_empty()
        || offer.osl_user_id.len() > 160
        || offer.safety_number.is_empty()
        || offer.safety_number.len() > 160
        || offer
            .friend_code
            .bytes()
            .any(|byte| !(byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_')))
        || offer.osl_user_id.chars().any(char::is_control)
        || offer.safety_number.chars().any(char::is_control)
    {
        return Err("Discord QA public offer is invalid".to_owned());
    }
    Ok(())
}

fn hex_digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn load_or_create_key(
    account_dir: &Path,
    sealer: &dyn Sealer,
) -> Result<Zeroizing<[u8; DEVICE_KEY_BYTES]>, String> {
    let path = account_dir.join(DEVICE_KEY_FILENAME);
    if let Some(bytes) = crate::atomic_file::read_recoverable_bounded(
        &path,
        MAX_DEVICE_KEY_RECORD_BYTES,
        "Discord QA device key",
    )? {
        return decode_record(&bytes, sealer);
    }

    let random = Zeroizing::new(crypto::random::random_bytes(DEVICE_KEY_BYTES));
    let mut key = Zeroizing::new([0u8; DEVICE_KEY_BYTES]);
    key.copy_from_slice(&random);
    let sealed = sealer
        .seal(&key[..])
        .map_err(|_| "Discord QA device key could not be sealed".to_owned())?;
    let record = DeviceKeyRecord {
        version: DEVICE_KEY_VERSION,
        sealer: sealer.method_label().to_owned(),
        sealed_key_b64: STANDARD.encode(sealed),
    };
    let encoded = serde_json::to_vec(&record)
        .map_err(|_| "Discord QA device key record could not be encoded".to_owned())?;
    crate::atomic_file::write_recoverable(&path, &encoded, "Discord QA device key")?;
    Ok(key)
}

fn decode_record(
    bytes: &[u8],
    sealer: &dyn Sealer,
) -> Result<Zeroizing<[u8; DEVICE_KEY_BYTES]>, String> {
    let record: DeviceKeyRecord = serde_json::from_slice(bytes)
        .map_err(|_| "Discord QA device key record is invalid".to_owned())?;
    if record.version != DEVICE_KEY_VERSION || record.sealer != sealer.method_label() {
        return Err("Discord QA device key is not bound to the current device sealer".to_owned());
    }
    let sealed = STANDARD
        .decode(record.sealed_key_b64)
        .map_err(|_| "Discord QA device key record is invalid".to_owned())?;
    let plain = Zeroizing::new(
        sealer
            .unseal(&sealed)
            .map_err(|_| "Discord QA device key could not be unsealed".to_owned())?,
    );
    if plain.len() != DEVICE_KEY_BYTES {
        return Err("Discord QA device key has an invalid size".to_owned());
    }
    let mut key = Zeroizing::new([0u8; DEVICE_KEY_BYTES]);
    key.copy_from_slice(&plain);
    Ok(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    static GLOBAL_STORAGE_TEST: std::sync::Mutex<()> = std::sync::Mutex::new(());

    struct StorageGlobalsGuard;

    impl Drop for StorageGlobalsGuard {
        fn drop(&mut self) {
            ipc::main_password::set_file_storage_key(None);
            keystore::set_active_account_dir(None);
            keystore::set_base_dir_override(None);
        }
    }
    use keystore::MemorySealer;

    fn test_dir(label: &str) -> std::path::PathBuf {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "osl-discord-qa-key-{label}-{}-{nonce}",
            std::process::id()
        ))
    }

    #[test]
    fn device_key_round_trips_without_plaintext_on_disk() {
        let dir = test_dir("roundtrip");
        let sealer = MemorySealer::new();
        let first = load_or_create_key(&dir, &sealer).unwrap();
        let bytes = std::fs::read(dir.join(DEVICE_KEY_FILENAME)).unwrap();
        assert!(!bytes
            .windows(first.len())
            .any(|window| window == &first[..]));
        let second = load_or_create_key(&dir, &sealer).unwrap();
        assert_eq!(&first[..], &second[..]);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn wrong_device_sealer_cannot_open_record() {
        let dir = test_dir("wrong-device");
        let first = MemorySealer::new();
        load_or_create_key(&dir, &first).unwrap();
        let other = MemorySealer::new();
        assert!(load_or_create_key(&dir, &other).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn malformed_and_unknown_records_fail_closed() {
        let sealer = MemorySealer::new();
        assert!(decode_record(
            br#"{"version":2,"sealer":"memory-test","sealed_key_b64":"AA=="}"#,
            &sealer
        )
        .is_err());
        assert!(decode_record(
            br#"{"version":1,"sealer":"memory-test","sealed_key_b64":"AA==","extra":true}"#,
            &sealer
        )
        .is_err());
    }

    fn sample_offer() -> PublicOffer {
        PublicOffer {
            version: PUBLIC_OFFER_VERSION,
            friend_code: "OSLFR1.ABCDEFGHIJKLMNOP".to_owned(),
            osl_user_id: "osl-qa-public-id".to_owned(),
            safety_number: "1234 5678 9012".to_owned(),
        }
    }

    #[test]
    fn public_offer_round_trips_with_exact_schema() {
        let offer = sample_offer();
        let encoded = encode_public_offer(&offer).unwrap();
        assert_eq!(decode_public_offer(&encoded).unwrap(), offer);

        let mut value: serde_json::Value = serde_json::from_slice(&encoded).unwrap();
        value["private_key"] = serde_json::Value::String("forbidden".to_owned());
        assert!(decode_public_offer(&serde_json::to_vec(&value).unwrap()).is_err());
    }

    #[test]
    fn public_offer_rejects_unbounded_or_malformed_fields() {
        let mut offer = sample_offer();
        offer.version += 1;
        assert!(encode_public_offer(&offer).is_err());

        let mut offer = sample_offer();
        offer.friend_code = format!("OSLFR1.{}", "A".repeat(8 * 1024));
        assert!(encode_public_offer(&offer).is_err());

        let mut offer = sample_offer();
        offer.osl_user_id = "peer\nother".to_owned();
        assert!(encode_public_offer(&offer).is_err());

        assert!(decode_public_offer(&vec![b'A'; MAX_PUBLIC_OFFER_BYTES as usize + 1]).is_err());
    }

    #[test]
    fn public_offer_is_written_atomically_to_the_fixed_qa_filename() {
        let dir = test_dir("public-offer");
        let encoded = encode_public_offer(&sample_offer()).unwrap();
        crate::atomic_file::write_recoverable(
            &dir.join(PUBLIC_OFFER_FILENAME),
            &encoded,
            "Discord QA public offer",
        )
        .unwrap();
        assert_eq!(
            decode_public_offer(&std::fs::read(dir.join(PUBLIC_OFFER_FILENAME)).unwrap()).unwrap(),
            sample_offer()
        );
        assert!(!dir.join("discord-qa-offer.v1.tmp").exists());
        assert!(!dir.join("discord-qa-offer.v1.bak").exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn two_qa_profiles_exchange_only_public_offers_and_become_verified() {
        let _serial = GLOBAL_STORAGE_TEST.lock().unwrap();
        let _globals = StorageGlobalsGuard;
        let root = test_dir("pairing-roundtrip");
        let alice_dir = root.join("alice");
        let bob_dir = root.join("bob");
        ipc::main_password::set_file_storage_key(Some([0x6a; DEVICE_KEY_BYTES]));
        keystore::set_active_account_dir(None);

        let alice = HubCoreState::default();
        *alice.osl.identity.lock().unwrap() =
            Some(keystore::generate_identity("qa-alice".to_owned()));
        keystore::set_base_dir_override(Some(alice_dir.clone()));
        publish_and_consume_pairing(&alice_dir, &alice, &HubSecurityState::default()).unwrap();
        let alice_offer = std::fs::read(alice_dir.join(PUBLIC_OFFER_FILENAME)).unwrap();
        assert!(!alice_dir.join(PAIRING_STATUS_FILENAME).exists());

        let bob = HubCoreState::default();
        *bob.osl.identity.lock().unwrap() = Some(keystore::generate_identity("qa-bob".to_owned()));
        std::fs::create_dir_all(&bob_dir).unwrap();
        std::fs::write(bob_dir.join(PEER_OFFER_FILENAME), &alice_offer).unwrap();
        keystore::set_base_dir_override(Some(bob_dir.clone()));
        publish_and_consume_pairing(&bob_dir, &bob, &HubSecurityState::default()).unwrap();
        let bob_people = security::list_people(&bob).unwrap();
        assert_eq!(bob_people.len(), 1);
        assert!(bob_people[0].safety_number_verified);
        assert!(!bob_people[0].pending_key_change);
        let bob_offer = std::fs::read(bob_dir.join(PUBLIC_OFFER_FILENAME)).unwrap();
        let bob_status: PairingStatus =
            serde_json::from_slice(&std::fs::read(bob_dir.join(PAIRING_STATUS_FILENAME)).unwrap())
                .unwrap();
        assert!(bob_status.verified);
        assert_eq!(bob_status.peer_offer_sha256, hex_digest(&alice_offer));

        std::fs::write(alice_dir.join(PEER_OFFER_FILENAME), &bob_offer).unwrap();
        keystore::set_base_dir_override(Some(alice_dir.clone()));
        publish_and_consume_pairing(&alice_dir, &alice, &HubSecurityState::default()).unwrap();
        // Re-consuming the same fixed peer offer is intentionally idempotent.
        publish_and_consume_pairing(&alice_dir, &alice, &HubSecurityState::default()).unwrap();
        let alice_people = security::list_people(&alice).unwrap();
        assert_eq!(alice_people.len(), 1);
        assert!(alice_people[0].safety_number_verified);
        assert!(!alice_people[0].pending_key_change);

        let _ = std::fs::remove_dir_all(root);
    }
}
