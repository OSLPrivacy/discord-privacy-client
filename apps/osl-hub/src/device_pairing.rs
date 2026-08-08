//! Same-account second-device pairing.
//!
//! This is the backend ceremony around the existing sealed transfer bundle and
//! the existing two-party TOFU number. The destination device can display a
//! one-time code and later open the sealed identity, but the device list only
//! changes after the already-held source device confirms the matching digits.

use crate::device_transfer::{self, TransferBundle};
use base64::{engine::general_purpose::STANDARD, Engine};
use crypto::{ed25519, random};
use ipc::tofu::{self, KeyBundle};
use keystore::{Identity, Sealer};
use serde::{Deserialize, Serialize};
use sha2::Digest;
use std::path::Path;

pub const NEW_DEVICE_CONFIRMATION_REFUSAL: &str = "Confirm this on the device you already have.";

const DEVICE_LIST_NOTE_DOMAIN: &str = "OSL/device-list/add/v1";

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewDevicePairingOffer {
    pub one_time_code: String,
    pub transfer_identifier: String,
    pub device_name: String,
    pub key_bundle: KeyBundle,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OldDevicePairingScreen {
    pub device_name: String,
    pub safety_number: String,
    pub transfer_bundle: TransferBundle,
    pub old_device_bundle: KeyBundle,
    pub new_device_bundle: KeyBundle,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewDevicePairingScreen {
    pub device_name: String,
    pub safety_number: String,
    pub opened_user_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceListEntry {
    pub device_id: String,
    pub device_name: String,
    pub key_bundle: KeyBundle,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceList {
    pub version: u32,
    pub devices: Vec<DeviceListEntry>,
    pub latest_note: Option<SignedDeviceListNote>,
}

impl DeviceList {
    pub fn one_device(device_name: impl Into<String>, identity: &Identity) -> Self {
        let entry = DeviceListEntry::from_identity(device_name, identity);
        Self {
            version: 1,
            devices: vec![entry],
            latest_note: None,
        }
    }

    pub fn device_count(&self) -> usize {
        self.devices.len()
    }
}

impl DeviceListEntry {
    pub fn from_identity(device_name: impl Into<String>, identity: &Identity) -> Self {
        let key_bundle = identity_key_bundle(identity);
        Self {
            device_id: device_id_for_bundle(&key_bundle),
            device_name: device_name.into(),
            key_bundle,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignedDeviceListNote {
    pub version: u32,
    pub previous_version: u32,
    pub added_device_id: String,
    pub added_device_name: String,
    pub signer_device_id: String,
    pub safety_number: String,
    pub signature_b64: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DevicePairingConfirmation {
    pub list: DeviceList,
    pub note_verified: bool,
}

/// Destination-side start: generate and show a one-time code with this
/// device's public bundle.
pub fn new_device_show_one_time_code(
    device_name: impl Into<String>,
    identity: &Identity,
) -> Result<NewDevicePairingOffer, String> {
    let one_time_code = decimal_code_from_random();
    let transfer_identifier = STANDARD.encode(random::random_bytes(16));
    new_device_offer_with_code(device_name, identity, one_time_code, transfer_identifier)
}

pub fn new_device_offer_with_code(
    device_name: impl Into<String>,
    identity: &Identity,
    one_time_code: impl Into<String>,
    transfer_identifier: impl Into<String>,
) -> Result<NewDevicePairingOffer, String> {
    let one_time_code = one_time_code.into();
    let transfer_identifier = transfer_identifier.into();
    if one_time_code.is_empty() || transfer_identifier.is_empty() {
        return Err("OSL device pairing code and transfer id are required".to_owned());
    }
    Ok(NewDevicePairingOffer {
        one_time_code,
        transfer_identifier,
        device_name: device_name.into(),
        key_bundle: identity_key_bundle(identity),
    })
}

/// Source-side code entry: seal the identity to the code and show the shared
/// six-block number. This is the first production caller of `device_transfer`.
pub fn old_device_accept_one_time_code(
    identity_path: &Path,
    source_sealer: &dyn Sealer,
    offer: &NewDevicePairingOffer,
) -> Result<OldDevicePairingScreen, String> {
    let transfer_bundle = device_transfer::export_identity_bundle(
        identity_path,
        source_sealer,
        &offer.one_time_code,
        &offer.transfer_identifier,
    )?;
    let old_identity = keystore::load_identity(identity_path, source_sealer)
        .map_err(|_| "OSL could not open the source identity for pairing".to_owned())?;
    let old_device_bundle = identity_key_bundle(&old_identity);
    let safety_number = tofu::safety_number_pair(&old_device_bundle, &offer.key_bundle)
        .map_err(|_| "OSL device pairing key bundle is invalid".to_owned())?;
    Ok(OldDevicePairingScreen {
        device_name: offer.device_name.clone(),
        safety_number,
        transfer_bundle,
        old_device_bundle,
        new_device_bundle: offer.key_bundle.clone(),
    })
}

/// Destination-side receipt: open the sealed bundle and show the same
/// six-block number. This is the second production caller of `device_transfer`.
pub fn new_device_receive_transfer_bundle(
    offer: &NewDevicePairingOffer,
    transfer_bundle: &TransferBundle,
) -> Result<NewDevicePairingScreen, String> {
    let opened = device_transfer::open_exported_identity(
        transfer_bundle,
        &offer.one_time_code,
        &offer.transfer_identifier,
    )?;
    let opened_bundle = identity_key_bundle(&opened);
    let safety_number = tofu::safety_number_pair(&opened_bundle, &offer.key_bundle)
        .map_err(|_| "OSL device pairing key bundle is invalid".to_owned())?;
    Ok(NewDevicePairingScreen {
        device_name: offer.device_name.clone(),
        safety_number,
        opened_user_id: opened.user_id.clone(),
    })
}

/// The new device may acknowledge what it sees, but that cannot authorize list
/// membership.
pub fn new_device_confirm_pairing(_list: &DeviceList) -> Result<(), String> {
    Err(NEW_DEVICE_CONFIRMATION_REFUSAL.to_owned())
}

/// Destination-side install after the old device has already signed the list
/// note. This reuses the existing transfer restore path; it does not add the
/// device to the list.
pub fn new_device_restore_after_old_confirmation(
    offer: &NewDevicePairingOffer,
    transfer_bundle: &TransferBundle,
    destination_identity_path: &Path,
    destination_sealer: &dyn Sealer,
) -> Result<Identity, String> {
    device_transfer::restore_exported_identity(
        transfer_bundle,
        &offer.one_time_code,
        &offer.transfer_identifier,
        destination_identity_path,
        destination_sealer,
    )
}

pub fn old_device_confirm_pairing(
    old_identity: &Identity,
    mut list: DeviceList,
    screen: &OldDevicePairingScreen,
    compared_safety_number: &str,
) -> Result<DevicePairingConfirmation, String> {
    if normalize_digits(&screen.safety_number) != normalize_digits(compared_safety_number) {
        return Err(format!(
            "OSL device pairing refused for {}: safety number mismatch",
            screen.device_name
        ));
    }
    let added = DeviceListEntry {
        device_id: device_id_for_bundle(&screen.new_device_bundle),
        device_name: screen.device_name.clone(),
        key_bundle: screen.new_device_bundle.clone(),
    };
    if list
        .devices
        .iter()
        .any(|device| device.device_id == added.device_id)
    {
        return Err(format!(
            "OSL device pairing refused for {}: device already listed",
            screen.device_name
        ));
    }

    let previous_version = list.version;
    let next_version = previous_version + 1;
    let signer_device_id = device_id_for_bundle(&identity_key_bundle(old_identity));
    let unsigned = unsigned_note_bytes(
        next_version,
        previous_version,
        &added.device_id,
        &added.device_name,
        &signer_device_id,
        &screen.safety_number,
    );
    let signature = ed25519::sign(&old_identity.ed25519_secret, &unsigned);
    let note = SignedDeviceListNote {
        version: next_version,
        previous_version,
        added_device_id: added.device_id.clone(),
        added_device_name: added.device_name.clone(),
        signer_device_id,
        safety_number: screen.safety_number.clone(),
        signature_b64: STANDARD.encode(signature.as_bytes()),
    };

    list.version = next_version;
    list.devices.push(added);
    list.latest_note = Some(note.clone());
    let note_verified = verify_device_list_note(&old_identity.ed25519_public, &note);
    Ok(DevicePairingConfirmation {
        list,
        note_verified,
    })
}

pub fn verify_device_list_note(public: &ed25519::PublicKey, note: &SignedDeviceListNote) -> bool {
    let Ok(signature_bytes) = STANDARD.decode(&note.signature_b64) else {
        return false;
    };
    let Ok(signature_bytes) = <[u8; ed25519::SIGNATURE_SIZE]>::try_from(signature_bytes.as_slice())
    else {
        return false;
    };
    let unsigned = unsigned_note_bytes(
        note.version,
        note.previous_version,
        &note.added_device_id,
        &note.added_device_name,
        &note.signer_device_id,
        &note.safety_number,
    );
    ed25519::verify(
        public,
        &unsigned,
        &ed25519::Signature::from_bytes(signature_bytes),
    )
    .unwrap_or(false)
}

fn identity_key_bundle(identity: &Identity) -> KeyBundle {
    KeyBundle {
        ed25519_pub: STANDARD.encode(identity.ed25519_public.as_bytes()),
        x25519_pub: STANDARD.encode(identity.x25519_public.as_bytes()),
        mlkem768_pub: STANDARD.encode(identity.mlkem_public_bytes),
        ratchet_initial_pub: identity
            .ratchet_initial_pub
            .map(|key| STANDARD.encode(key.as_bytes())),
    }
}

fn device_id_for_bundle(bundle: &KeyBundle) -> String {
    let digest = sha2::Sha256::digest(bundle.ed25519_pub.as_bytes());
    format!("osl-device-{}", STANDARD.encode(&digest[..18]))
}

fn unsigned_note_bytes(
    version: u32,
    previous_version: u32,
    added_device_id: &str,
    added_device_name: &str,
    signer_device_id: &str,
    safety_number: &str,
) -> Vec<u8> {
    format!(
        "{DEVICE_LIST_NOTE_DOMAIN}\n{version}\n{previous_version}\n{added_device_id}\n{added_device_name}\n{signer_device_id}\n{}\n",
        normalize_digits(safety_number)
    )
    .into_bytes()
}

fn normalize_digits(value: &str) -> String {
    value.chars().filter(|c| c.is_ascii_digit()).collect()
}

fn decimal_code_from_random() -> String {
    let bytes = random::random_bytes(4);
    let value = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) % 1_000_000;
    format!("{value:06}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn altered_one_digit(number: &str) -> String {
        let mut changed = String::with_capacity(number.len());
        let mut altered = false;
        for c in number.chars() {
            if !altered && c.is_ascii_digit() {
                changed.push(if c == '9' {
                    '0'
                } else {
                    char::from(c as u8 + 1)
                });
                altered = true;
            } else {
                changed.push(c);
            }
        }
        changed
    }

    fn safety_shape(number: &str) -> (usize, bool) {
        let groups: Vec<&str> = number.split(' ').collect();
        (
            groups.len(),
            groups
                .iter()
                .all(|group| group.len() == 5 && group.chars().all(|c| c.is_ascii_digit())),
        )
    }

    #[test]
    fn task_4801_one_time_code_pairing_requires_old_device_confirmation() {
        let root = tempfile::TempDir::new().unwrap();
        let old_app_folder = root.path().join("old-app-folder");
        let new_app_folder = root.path().join("new-app-folder");
        fs::create_dir_all(&old_app_folder).unwrap();
        fs::create_dir_all(&new_app_folder).unwrap();
        let old_path = old_app_folder.join("identity.json");
        let restored_path = new_app_folder.join("identity.json");
        let old_sealer = keystore::MemorySealer::new();
        let new_sealer = keystore::MemorySealer::new();
        let old_identity = keystore::identity_from_entropy([0x48; 16], "task-4801-owner".into());
        let new_identity =
            keystore::identity_from_entropy([0x01; 16], "task-4801-new-device".into());
        keystore::save_identity(&old_path, &old_identity, &old_sealer).unwrap();

        let offer = new_device_offer_with_code(
            "Work laptop",
            &new_identity,
            "480101",
            "task-4801-transfer-id",
        )
        .unwrap();
        let mut list = DeviceList::one_device("Existing desktop", &old_identity);
        let old_count_before = list.device_count();
        let old_version_before = list.version;

        let old_screen = old_device_accept_one_time_code(&old_path, &old_sealer, &offer).unwrap();
        let new_screen =
            new_device_receive_transfer_bundle(&offer, &old_screen.transfer_bundle).unwrap();

        assert_eq!(old_screen.safety_number, new_screen.safety_number);
        let (group_count, groups_are_five_digits) = safety_shape(&old_screen.safety_number);
        assert_eq!(group_count, 6);
        assert!(groups_are_five_digits);
        assert_eq!(normalize_digits(&old_screen.safety_number).len(), 30);

        let new_confirm_message = new_device_confirm_pairing(&list).unwrap_err();
        assert_eq!(new_confirm_message, NEW_DEVICE_CONFIRMATION_REFUSAL);
        let new_confirm_added_devices = list.device_count() - old_count_before;

        let wrong = altered_one_digit(&new_screen.safety_number);
        let refused = old_device_confirm_pairing(&old_identity, list.clone(), &old_screen, &wrong)
            .unwrap_err();
        assert!(refused.contains("Work laptop"), "{refused}");
        let altered_digit_added_devices = list.device_count() - old_count_before;

        let confirmed = old_device_confirm_pairing(
            &old_identity,
            list.clone(),
            &old_screen,
            &new_screen.safety_number,
        )
        .unwrap();
        list = confirmed.list;
        assert!(confirmed.note_verified);
        assert_eq!(old_count_before, 1);
        assert_eq!(old_version_before, 1);
        assert_eq!(list.device_count(), 2);
        assert_eq!(list.version, 2);

        let restored = new_device_restore_after_old_confirmation(
            &offer,
            &old_screen.transfer_bundle,
            &restored_path,
            &new_sealer,
        )
        .unwrap();
        assert_eq!(restored.user_id, old_identity.user_id);
        assert!(restored_path.exists());

        println!("TASK4801 new_device_one_time_code={}", offer.one_time_code);
        println!(
            "TASK4801 old_screen_safety_number={}",
            old_screen.safety_number
        );
        println!(
            "TASK4801 new_screen_safety_number={}",
            new_screen.safety_number
        );
        println!("TASK4801 safety_number_groups={group_count}");
        println!("TASK4801 safety_number_digits=30");
        println!("TASK4801 new_confirm_message={new_confirm_message}");
        println!("TASK4801 new_confirm_added_devices={new_confirm_added_devices}");
        println!("TASK4801 old_confirm_before_devices={old_count_before}");
        println!("TASK4801 old_confirm_after_devices={}", list.device_count());
        println!("TASK4801 old_confirm_after_version={}", list.version);
        println!(
            "TASK4801 old_confirm_signed_note_verified={}",
            confirmed.note_verified
        );
        println!("TASK4801 altered_digit_refusal={refused}");
        println!("TASK4801 altered_digit_added_devices={altered_digit_added_devices}");
        println!(
            "TASK4801 lesser_run_label=LESSER RUN: single machine with two separate app folders proves wiring; live run still needs two real machines"
        );
        println!("TASK4801 lesser_run_app_folders=2");
        println!("TASK4801 device_transfer_callers=3");
    }
}
