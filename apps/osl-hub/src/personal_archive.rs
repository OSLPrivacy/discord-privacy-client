//! Complete, self-contained personal account archives.
//!
//! The archive key is derived from the export passphrase and a random salt.
//! OSL does not retain that passphrase or require a service-side wrapping key.
//! Every required account class is sealed independently so an importer can
//! identify an omitted or damaged class without accepting a partial restore.

use rand::{rngs::OsRng, RngCore};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use zeroize::Zeroizing;

pub const PAYMENT_EXCLUSION_NOTICE: &str =
    "Voucher/payment records are excluded by design — OSL holds no payment data.";
pub const PAYMENT_EXCLUSION_CLASS: &str = "Voucher/payment records";
pub const TRANSIENT_RUNTIME_EXCLUSION_CLASS: &str = "Transient diagnostic/runtime logs";
pub const TRANSIENT_RUNTIME_EXCLUSION_NOTICE: &str =
    "Transient diagnostic/runtime logs are excluded because they are not durable account state.";
pub const DELIBERATE_EXCLUSIONS: [&str; 2] =
    [PAYMENT_EXCLUSION_CLASS, TRANSIENT_RUNTIME_EXCLUSION_CLASS];

pub const REQUIRED_CLASSES: [&str; 7] = [
    "identity_and_keys",
    "friends",
    "conversations",
    "attachments",
    "settings",
    "memberships",
    "device_recovery",
];

pub const REQUIRED_JOURNEYS: [&str; 7] = [
    "authenticate_restored_identity",
    "inspect_exact_state",
    "open_every_message",
    "open_every_attachment",
    "send_receive_new_encrypted_message",
    "exercise_membership_authority",
    "complete_recovery_device_authority",
];

pub const REQUIRED_MUTANTS: [&str; 20] = [
    "omit_identity_and_keys",
    "omit_friends",
    "omit_conversations",
    "omit_attachments",
    "omit_settings",
    "omit_memberships",
    "omit_device_recovery",
    "corrupt_identity_and_keys",
    "corrupt_friends",
    "corrupt_conversations",
    "corrupt_attachments",
    "corrupt_settings",
    "corrupt_memberships",
    "corrupt_device_recovery",
    "depends_on_original_state",
    "checksum_only",
    "manifest_only",
    "offline_reader_only",
    "omit_exclusion",
    "add_unknown_class",
];

const ARCHIVE_FORMAT: &str = "osl-personal-archive-v1";
const STATE_FILE: &str = "account-state.osl";
const KDF_ALGORITHM: &str = "sha256-iterated-v1";
const KDF_ROUNDS: u32 = 120_000;
const SALT_BYTES: usize = 16;
const MAX_PASSPHRASE_BYTES: usize = 1024;
const MAX_ARCHIVE_BYTES: u64 = 128 * 1024 * 1024;
const MAX_CLASS_PLAINTEXT_BYTES: usize = 64 * 1024 * 1024;
const MAX_RECORDS_PER_CLASS: usize = 250_000;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct IdentityAndKeys {
    pub account_id: String,
    pub display_name: String,
    /// An opaque credential carried to the new installation.
    pub authentication_secret: Vec<u8>,
    pub identity_private_key: Vec<u8>,
    pub identity_public_key: Vec<u8>,
    /// Account message key material. The archive itself never exposes it.
    pub message_key: Vec<u8>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FriendRecord {
    pub friend_id: String,
    pub owner_account_id: String,
    pub display_name: String,
    pub public_key: Vec<u8>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MessageRecord {
    pub message_id: String,
    pub owner_account_id: String,
    pub author_id: String,
    pub body: Vec<u8>,
    pub sent_at_ms: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ConversationRecord {
    pub conversation_id: String,
    pub owner_account_id: String,
    pub member_ids: Vec<String>,
    pub messages: Vec<MessageRecord>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AttachmentRecord {
    pub attachment_id: String,
    pub owner_account_id: String,
    pub message_id: String,
    pub filename: String,
    pub media_type: String,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MembershipRecord {
    pub membership_id: String,
    pub owner_account_id: String,
    pub role: String,
    pub authority_key: Vec<u8>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DeviceRecord {
    pub device_id: String,
    pub public_key: Vec<u8>,
    pub authorized: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DeviceRecoveryState {
    pub owner_account_id: String,
    pub recovery_secret: Vec<u8>,
    pub devices: Vec<DeviceRecord>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AccountSnapshot {
    pub identity_and_keys: IdentityAndKeys,
    pub friends: Vec<FriendRecord>,
    pub conversations: Vec<ConversationRecord>,
    pub attachments: Vec<AttachmentRecord>,
    pub settings: BTreeMap<String, Vec<u8>>,
    pub memberships: Vec<MembershipRecord>,
    pub device_recovery: DeviceRecoveryState,
}

impl AccountSnapshot {
    /// Strip records owned by another account before the archive oracle is
    /// fixed. Foreign records are canaries, never transferable user data.
    pub fn owned_only(&self) -> Self {
        let owner = self.identity_and_keys.account_id.as_str();
        let mut copy = self.clone();
        copy.friends
            .retain(|record| record.owner_account_id == owner);
        copy.conversations
            .retain(|record| record.owner_account_id == owner);
        for conversation in &mut copy.conversations {
            conversation
                .messages
                .retain(|record| record.owner_account_id == owner);
        }
        copy.attachments
            .retain(|record| record.owner_account_id == owner);
        copy.memberships
            .retain(|record| record.owner_account_id == owner);
        copy
    }

    pub fn counts(&self) -> ClassCounts {
        ClassCounts {
            identity_and_keys: 1,
            friends: self.friends.len(),
            conversations: self.conversations.len(),
            messages: self.conversations.iter().map(|c| c.messages.len()).sum(),
            attachments: self.attachments.len(),
            settings: self.settings.len(),
            memberships: self.memberships.len(),
            devices: self.device_recovery.devices.len(),
        }
    }

    fn validate(&self) -> Result<(), String> {
        let owner = self.identity_and_keys.account_id.as_str();
        if owner.is_empty() {
            return Err("identity_and_keys: account_id is missing".to_string());
        }
        if self.identity_and_keys.authentication_secret.is_empty()
            || self.identity_and_keys.identity_private_key.is_empty()
            || self.identity_and_keys.identity_public_key.is_empty()
            || self.identity_and_keys.message_key.is_empty()
        {
            return Err(
                "identity_and_keys: required identity or key material is missing".to_string(),
            );
        }
        if self.device_recovery.owner_account_id != owner
            || self.device_recovery.recovery_secret.is_empty()
        {
            return Err("device_recovery: owner or recovery material is invalid".to_string());
        }
        bounded("friends", self.friends.len())?;
        bounded("conversations", self.conversations.len())?;
        bounded("attachments", self.attachments.len())?;
        bounded("settings", self.settings.len())?;
        bounded("memberships", self.memberships.len())?;
        bounded("device_recovery", self.device_recovery.devices.len())?;

        let mut message_ids = BTreeSet::new();
        for friend in &self.friends {
            require_owner("friends", owner, &friend.owner_account_id)?;
        }
        for conversation in &self.conversations {
            require_owner("conversations", owner, &conversation.owner_account_id)?;
            bounded("conversations.messages", conversation.messages.len())?;
            for message in &conversation.messages {
                require_owner("conversations", owner, &message.owner_account_id)?;
                if message.message_id.is_empty() || !message_ids.insert(message.message_id.clone())
                {
                    return Err("conversations: message id is empty or duplicated".to_string());
                }
            }
        }
        for attachment in &self.attachments {
            require_owner("attachments", owner, &attachment.owner_account_id)?;
            if !message_ids.contains(&attachment.message_id) {
                return Err(format!(
                    "attachments: {} refers to a missing message",
                    attachment.attachment_id
                ));
            }
        }
        for membership in &self.memberships {
            require_owner("memberships", owner, &membership.owner_account_id)?;
            if membership.authority_key.is_empty() {
                return Err(format!(
                    "memberships: {} has no authority material",
                    membership.membership_id
                ));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ClassCounts {
    pub identity_and_keys: usize,
    pub friends: usize,
    pub conversations: usize,
    pub messages: usize,
    pub attachments: usize,
    pub settings: usize,
    pub memberships: usize,
    pub devices: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ExportDisclosure {
    pub named_exclusions: Vec<String>,
    pub notice: String,
}

impl Default for ExportDisclosure {
    fn default() -> Self {
        Self {
            named_exclusions: DELIBERATE_EXCLUSIONS
                .into_iter()
                .map(str::to_string)
                .collect(),
            notice: format!("{PAYMENT_EXCLUSION_NOTICE} {TRANSIENT_RUNTIME_EXCLUSION_NOTICE}"),
        }
    }
}

/// A packaged Settings export cannot run until its exact disclosure was
/// recorded as visible. This models the product boundary rather than a reader.
pub struct SettingsExportSession {
    source: AccountSnapshot,
    disclosure_recorded: bool,
}

impl SettingsExportSession {
    pub fn new(source: AccountSnapshot) -> Self {
        Self {
            source: source.owned_only(),
            disclosure_recorded: false,
        }
    }

    pub fn disclosure(&self) -> ExportDisclosure {
        ExportDisclosure::default()
    }

    pub fn record_visible_disclosure(
        &mut self,
        named_exclusions: &[String],
        notice: &str,
    ) -> Result<(), String> {
        require_disclosure(named_exclusions, notice)?;
        self.disclosure_recorded = true;
        Ok(())
    }

    pub fn export_to(&self, destination: &Path, passphrase: &str) -> Result<ExportReceipt, String> {
        if !self.disclosure_recorded {
            return Err(format!(
                "hidden exclusion: export requires visible {} disclosure before export",
                PAYMENT_EXCLUSION_CLASS
            ));
        }
        require_export_time_dispositions(
            REQUIRED_CLASSES.into_iter().chain(DELIBERATE_EXCLUSIONS),
            REQUIRED_CLASSES,
            DELIBERATE_EXCLUSIONS,
        )?;
        self.source.validate()?;
        let bytes = seal_archive(&self.source, passphrase)?;
        crate::atomic_file::write_recoverable(destination, &bytes, "personal account archive")?;
        Ok(ExportReceipt {
            archive_bytes: bytes.len() as u64,
            class_counts: self.source.counts(),
            disclosure: ExportDisclosure::default(),
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ExportReceipt {
    pub archive_bytes: u64,
    pub class_counts: ClassCounts,
    pub disclosure: ExportDisclosure,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct KdfSpec {
    algorithm: String,
    rounds: u32,
    salt: Vec<u8>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct SealedPart {
    nonce: Vec<u8>,
    ciphertext: Vec<u8>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct ArchiveEnvelope {
    format: String,
    kdf: KdfSpec,
    metadata: SealedPart,
    classes: BTreeMap<String, SealedPart>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct ArchiveMetadata {
    deliberate_exclusions: Vec<String>,
    disclosure_notice: String,
    source_dependencies: Vec<String>,
}

fn seal_archive(snapshot: &AccountSnapshot, passphrase: &str) -> Result<Vec<u8>, String> {
    validate_passphrase(passphrase)?;
    snapshot.validate()?;
    let mut salt = vec![0u8; SALT_BYTES];
    OsRng.fill_bytes(&mut salt);
    let key = derive_archive_key(passphrase, &salt, KDF_ROUNDS)?;
    let metadata = ArchiveMetadata {
        deliberate_exclusions: DELIBERATE_EXCLUSIONS
            .into_iter()
            .map(str::to_string)
            .collect(),
        disclosure_notice: format!(
            "{PAYMENT_EXCLUSION_NOTICE} {TRANSIENT_RUNTIME_EXCLUSION_NOTICE}"
        ),
        source_dependencies: Vec::new(),
    };
    let mut classes = BTreeMap::new();
    classes.insert(
        "identity_and_keys".to_string(),
        seal_part(
            &key,
            &salt,
            "identity_and_keys",
            &snapshot.identity_and_keys,
        )?,
    );
    classes.insert(
        "friends".to_string(),
        seal_part(&key, &salt, "friends", &snapshot.friends)?,
    );
    classes.insert(
        "conversations".to_string(),
        seal_part(&key, &salt, "conversations", &snapshot.conversations)?,
    );
    classes.insert(
        "attachments".to_string(),
        seal_part(&key, &salt, "attachments", &snapshot.attachments)?,
    );
    classes.insert(
        "settings".to_string(),
        seal_part(&key, &salt, "settings", &snapshot.settings)?,
    );
    classes.insert(
        "memberships".to_string(),
        seal_part(&key, &salt, "memberships", &snapshot.memberships)?,
    );
    classes.insert(
        "device_recovery".to_string(),
        seal_part(&key, &salt, "device_recovery", &snapshot.device_recovery)?,
    );
    let envelope = ArchiveEnvelope {
        format: ARCHIVE_FORMAT.to_string(),
        kdf: KdfSpec {
            algorithm: KDF_ALGORITHM.to_string(),
            rounds: KDF_ROUNDS,
            salt,
        },
        metadata: seal_part(&key, &[], "metadata", &metadata)?,
        classes,
    };
    serde_json::to_vec(&envelope).map_err(|error| format!("archive serialize: {error}"))
}

fn open_archive(bytes: &[u8], passphrase: &str) -> Result<AccountSnapshot, String> {
    validate_passphrase(passphrase)?;
    if bytes.len() as u64 > MAX_ARCHIVE_BYTES {
        return Err("archive exceeds the bounded input limit".to_string());
    }
    let envelope: ArchiveEnvelope = serde_json::from_slice(bytes)
        .map_err(|error| format!("archive envelope is corrupt: {error}"))?;
    if envelope.format != ARCHIVE_FORMAT {
        return Err("archive format is unsupported".to_string());
    }
    if envelope.kdf.algorithm != KDF_ALGORITHM
        || envelope.kdf.rounds != KDF_ROUNDS
        || envelope.kdf.salt.len() != SALT_BYTES
    {
        return Err("archive KDF parameters are invalid".to_string());
    }
    validate_class_names(&envelope.classes)?;
    let key = derive_archive_key(passphrase, &envelope.kdf.salt, envelope.kdf.rounds)?;
    let metadata: ArchiveMetadata = open_part(&key, &[], "metadata", &envelope.metadata)
        .map_err(|_| "archive metadata is corrupt or the passphrase is incorrect".to_string())?;
    require_disclosure(&metadata.deliberate_exclusions, &metadata.disclosure_notice)?;
    if !metadata.source_dependencies.is_empty() {
        return Err(format!(
            "stale dependency: archive requires {}",
            metadata.source_dependencies.join(", ")
        ));
    }
    let get = |name: &str| {
        envelope
            .classes
            .get(name)
            .ok_or_else(|| format!("missing required class: {name}"))
    };
    let snapshot = AccountSnapshot {
        identity_and_keys: open_named(
            &key,
            &envelope.kdf.salt,
            "identity_and_keys",
            get("identity_and_keys")?,
        )?,
        friends: open_named(&key, &envelope.kdf.salt, "friends", get("friends")?)?,
        conversations: open_named(
            &key,
            &envelope.kdf.salt,
            "conversations",
            get("conversations")?,
        )?,
        attachments: open_named(&key, &envelope.kdf.salt, "attachments", get("attachments")?)?,
        settings: open_named(&key, &envelope.kdf.salt, "settings", get("settings")?)?,
        memberships: open_named(&key, &envelope.kdf.salt, "memberships", get("memberships")?)?,
        device_recovery: open_named(
            &key,
            &envelope.kdf.salt,
            "device_recovery",
            get("device_recovery")?,
        )?,
    };
    snapshot.validate()?;
    Ok(snapshot)
}

fn validate_class_names(classes: &BTreeMap<String, SealedPart>) -> Result<(), String> {
    let expected: BTreeSet<&str> = REQUIRED_CLASSES.into_iter().collect();
    for name in classes.keys() {
        if !expected.contains(name.as_str()) {
            return Err(format!("unknown archive class: {name}"));
        }
    }
    for name in REQUIRED_CLASSES {
        if !classes.contains_key(name) {
            return Err(format!("missing required class: {name}"));
        }
    }
    Ok(())
}

fn seal_part<T: Serialize>(
    key: &crypto::aead::Key,
    salt: &[u8],
    name: &str,
    value: &T,
) -> Result<SealedPart, String> {
    let plaintext =
        serde_json::to_vec(value).map_err(|error| format!("{name}: serialize failed: {error}"))?;
    if plaintext.len() > MAX_CLASS_PLAINTEXT_BYTES {
        return Err(format!("{name}: class exceeds the bounded size limit"));
    }
    let mut nonce_bytes = vec![0u8; crypto::aead::NONCE_SIZE];
    OsRng.fill_bytes(&mut nonce_bytes);
    let mut nonce_array = [0u8; crypto::aead::NONCE_SIZE];
    nonce_array.copy_from_slice(&nonce_bytes);
    let nonce = crypto::aead::Nonce::from_bytes(nonce_array);
    let ad = associated_data(salt, name);
    let ciphertext = crypto::aead::seal(key, &nonce, &ad, &plaintext)
        .map_err(|error| format!("{name}: encryption failed: {error}"))?;
    Ok(SealedPart {
        nonce: nonce_bytes,
        ciphertext,
    })
}

fn open_named<T: DeserializeOwned>(
    key: &crypto::aead::Key,
    salt: &[u8],
    name: &str,
    part: &SealedPart,
) -> Result<T, String> {
    open_part(key, salt, name, part)
        .map_err(|error| format!("corrupt required class {name}: {error}"))
}

fn open_part<T: DeserializeOwned>(
    key: &crypto::aead::Key,
    salt: &[u8],
    name: &str,
    part: &SealedPart,
) -> Result<T, String> {
    if part.nonce.len() != crypto::aead::NONCE_SIZE
        || part.ciphertext.len() < crypto::aead::TAG_SIZE
        || part.ciphertext.len() > MAX_CLASS_PLAINTEXT_BYTES + crypto::aead::TAG_SIZE
    {
        return Err("invalid sealed-part bounds".to_string());
    }
    let mut nonce_array = [0u8; crypto::aead::NONCE_SIZE];
    nonce_array.copy_from_slice(&part.nonce);
    let nonce = crypto::aead::Nonce::from_bytes(nonce_array);
    let plaintext = crypto::aead::open(key, &nonce, &associated_data(salt, name), &part.ciphertext)
        .map_err(|_| "authentication failed".to_string())?;
    serde_json::from_slice(&plaintext).map_err(|error| format!("decode failed: {error}"))
}

fn associated_data(salt: &[u8], name: &str) -> Vec<u8> {
    let mut ad = Vec::with_capacity(ARCHIVE_FORMAT.len() + salt.len() + name.len() + 2);
    ad.extend_from_slice(ARCHIVE_FORMAT.as_bytes());
    ad.push(0);
    ad.extend_from_slice(salt);
    ad.push(0);
    ad.extend_from_slice(name.as_bytes());
    ad
}

fn derive_archive_key(
    passphrase: &str,
    salt: &[u8],
    rounds: u32,
) -> Result<crypto::aead::Key, String> {
    validate_passphrase(passphrase)?;
    if salt.len() != SALT_BYTES || rounds != KDF_ROUNDS {
        return Err("archive KDF parameters are invalid".to_string());
    }
    let mut digest: [u8; 32] = Sha256::new()
        .chain_update(b"osl-personal-archive-passphrase-v1\0")
        .chain_update(salt)
        .chain_update(passphrase.as_bytes())
        .finalize()
        .into();
    for round in 1..rounds {
        digest = Sha256::new()
            .chain_update(b"osl-personal-archive-passphrase-v1\0")
            .chain_update(digest)
            .chain_update(salt)
            .chain_update(round.to_le_bytes())
            .chain_update(passphrase.as_bytes())
            .finalize()
            .into();
    }
    Ok(crypto::aead::Key::from_bytes(digest))
}

fn validate_passphrase(passphrase: &str) -> Result<(), String> {
    if passphrase.is_empty() || passphrase.len() > MAX_PASSPHRASE_BYTES {
        return Err("archive passphrase is empty or exceeds the bounded limit".to_string());
    }
    Ok(())
}

fn require_disclosure(exclusions: &[String], notice: &str) -> Result<(), String> {
    let expected = DELIBERATE_EXCLUSIONS.map(str::to_string);
    if exclusions != expected {
        return Err(format!(
            "hidden exclusion: expected exactly {}",
            DELIBERATE_EXCLUSIONS.join(" | ")
        ));
    }
    let expected_notice =
        format!("{PAYMENT_EXCLUSION_NOTICE} {TRANSIENT_RUNTIME_EXCLUSION_NOTICE}");
    if notice != expected_notice {
        return Err(format!(
            "hidden exclusion: exact notice missing: {expected_notice}"
        ));
    }
    Ok(())
}

/// Every production data class must be named as exported or deliberately
/// excluded before the Settings export boundary can write an archive.
pub fn require_export_time_dispositions(
    production_classes: impl IntoIterator<Item = &'static str>,
    exported_classes: impl IntoIterator<Item = &'static str>,
    excluded_classes: impl IntoIterator<Item = &'static str>,
) -> Result<(), String> {
    let exported = exported_classes.into_iter().collect::<BTreeSet<_>>();
    let excluded = excluded_classes.into_iter().collect::<BTreeSet<_>>();
    for class in production_classes {
        if !exported.contains(class) && !excluded.contains(class) {
            return Err(format!(
                "absent export-time words: production class {class} is neither exported nor deliberately excluded"
            ));
        }
    }
    Ok(())
}

fn bounded(class: &str, count: usize) -> Result<(), String> {
    if count > MAX_RECORDS_PER_CLASS {
        Err(format!("{class}: record count exceeds the bounded limit"))
    } else {
        Ok(())
    }
}

fn require_owner(class: &str, expected: &str, actual: &str) -> Result<(), String> {
    if actual == expected {
        Ok(())
    } else {
        Err(format!("{class}: foreign-owner record was not excluded"))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ImportReceipt {
    pub account_id: String,
    pub class_counts: ClassCounts,
    pub state_file: PathBuf,
}

/// A clean packaged installation. Import writes only the self-contained
/// archive; authentication subsequently reopens it with caller-held secrets.
#[derive(Clone, Debug)]
pub struct FreshInstall {
    root: PathBuf,
}

impl FreshInstall {
    pub fn new(root: impl AsRef<Path>) -> Result<Self, String> {
        let root = root.as_ref().to_path_buf();
        std::fs::create_dir_all(&root)
            .map_err(|error| format!("fresh install directory: {error}"))?;
        let state_file = root.join(STATE_FILE);
        if state_file.exists() {
            return Err("fresh install already contains account-state".to_string());
        }
        Ok(Self { root })
    }

    pub fn import_archive(
        &self,
        archive_path: &Path,
        passphrase: &str,
    ) -> Result<ImportReceipt, String> {
        if self.root.join(STATE_FILE).exists() {
            return Err(
                "import requires an installation with no existing account-state".to_string(),
            );
        }
        let bytes = read_bounded_regular_file(archive_path, MAX_ARCHIVE_BYTES, "personal archive")?;
        let snapshot = open_archive(&bytes, passphrase)?;
        let state_file = self.root.join(STATE_FILE);
        crate::atomic_file::write_recoverable(&state_file, &bytes, "restored account-state")?;
        Ok(ImportReceipt {
            account_id: snapshot.identity_and_keys.account_id.clone(),
            class_counts: snapshot.counts(),
            state_file,
        })
    }

    pub fn authenticate(
        &self,
        account_id: &str,
        authentication_secret: &[u8],
        archive_passphrase: &str,
    ) -> Result<WorkingAccount, String> {
        let state_file = self.root.join(STATE_FILE);
        let bytes =
            read_bounded_regular_file(&state_file, MAX_ARCHIVE_BYTES, "restored account-state")?;
        let snapshot = open_archive(&bytes, archive_passphrase)?;
        if snapshot.identity_and_keys.account_id != account_id
            || !secret_eq(
                &snapshot.identity_and_keys.authentication_secret,
                authentication_secret,
            )
        {
            return Err("authenticate_restored_identity: credentials do not match".to_string());
        }
        Ok(WorkingAccount {
            snapshot,
            state_file,
            archive_passphrase: Zeroizing::new(archive_passphrase.to_string()),
        })
    }

    pub fn state_file(&self) -> PathBuf {
        self.root.join(STATE_FILE)
    }
}

fn read_bounded_regular_file(path: &Path, max: u64, label: &str) -> Result<Vec<u8>, String> {
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|error| format!("{label} metadata could not be read: {error}"))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() > max {
        return Err(format!("{label} is not a bounded regular file"));
    }
    std::fs::read(path).map_err(|error| format!("{label} could not be read: {error}"))
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EncryptedMessage {
    pub conversation_id: String,
    pub message_id: String,
    pub sender_id: String,
    pub sent_at_ms: u64,
    pub nonce: Vec<u8>,
    pub ciphertext: Vec<u8>,
}

pub struct WorkingAccount {
    snapshot: AccountSnapshot,
    state_file: PathBuf,
    archive_passphrase: Zeroizing<String>,
}

impl WorkingAccount {
    pub fn snapshot(&self) -> &AccountSnapshot {
        &self.snapshot
    }

    pub fn open_message(&self, message_id: &str) -> Result<Vec<u8>, String> {
        self.snapshot
            .conversations
            .iter()
            .flat_map(|conversation| &conversation.messages)
            .find(|message| message.message_id == message_id)
            .map(|message| message.body.clone())
            .ok_or_else(|| format!("open_every_message: missing message {message_id}"))
    }

    pub fn open_attachment(&self, attachment_id: &str) -> Result<Vec<u8>, String> {
        self.snapshot
            .attachments
            .iter()
            .find(|attachment| attachment.attachment_id == attachment_id)
            .map(|attachment| attachment.bytes.clone())
            .ok_or_else(|| format!("open_every_attachment: missing attachment {attachment_id}"))
    }

    pub fn send_encrypted_message(
        &self,
        conversation_id: &str,
        message_id: &str,
        body: &[u8],
        sent_at_ms: u64,
    ) -> Result<EncryptedMessage, String> {
        if !self
            .snapshot
            .conversations
            .iter()
            .any(|conversation| conversation.conversation_id == conversation_id)
        {
            return Err(format!(
                "send_receive_new_encrypted_message: missing conversation {conversation_id}"
            ));
        }
        if message_id.is_empty() {
            return Err("send_receive_new_encrypted_message: message id is empty".to_string());
        }
        let key = account_message_key(&self.snapshot.identity_and_keys.message_key);
        let mut nonce_bytes = vec![0u8; crypto::aead::NONCE_SIZE];
        OsRng.fill_bytes(&mut nonce_bytes);
        let mut nonce_array = [0u8; crypto::aead::NONCE_SIZE];
        nonce_array.copy_from_slice(&nonce_bytes);
        let ciphertext = crypto::aead::seal(
            &key,
            &crypto::aead::Nonce::from_bytes(nonce_array),
            &message_ad(conversation_id, message_id),
            body,
        )
        .map_err(|error| format!("send_receive_new_encrypted_message: encrypt: {error}"))?;
        Ok(EncryptedMessage {
            conversation_id: conversation_id.to_string(),
            message_id: message_id.to_string(),
            sender_id: self.snapshot.identity_and_keys.account_id.clone(),
            sent_at_ms,
            nonce: nonce_bytes,
            ciphertext,
        })
    }

    pub fn receive_encrypted_message(&mut self, wire: EncryptedMessage) -> Result<Vec<u8>, String> {
        if wire.nonce.len() != crypto::aead::NONCE_SIZE {
            return Err("send_receive_new_encrypted_message: invalid nonce".to_string());
        }
        let mut nonce_array = [0u8; crypto::aead::NONCE_SIZE];
        nonce_array.copy_from_slice(&wire.nonce);
        let body = crypto::aead::open(
            &account_message_key(&self.snapshot.identity_and_keys.message_key),
            &crypto::aead::Nonce::from_bytes(nonce_array),
            &message_ad(&wire.conversation_id, &wire.message_id),
            &wire.ciphertext,
        )
        .map_err(|_| "send_receive_new_encrypted_message: decrypt failed".to_string())?;
        let mut next = self.snapshot.clone();
        let conversation = next
            .conversations
            .iter_mut()
            .find(|conversation| conversation.conversation_id == wire.conversation_id)
            .ok_or_else(|| {
                format!(
                    "send_receive_new_encrypted_message: missing conversation {}",
                    wire.conversation_id
                )
            })?;
        if conversation
            .messages
            .iter()
            .any(|message| message.message_id == wire.message_id)
        {
            return Err("send_receive_new_encrypted_message: duplicate message id".to_string());
        }
        conversation.messages.push(MessageRecord {
            message_id: wire.message_id,
            owner_account_id: next.identity_and_keys.account_id.clone(),
            author_id: wire.sender_id,
            body: body.clone(),
            sent_at_ms: wire.sent_at_ms,
        });
        self.commit(next)?;
        Ok(body)
    }

    pub fn exercise_membership_authority(
        &self,
        membership_id: &str,
        challenge: &[u8],
    ) -> Result<[u8; 32], String> {
        let membership = self
            .snapshot
            .memberships
            .iter()
            .find(|membership| membership.membership_id == membership_id)
            .ok_or_else(|| format!("exercise_membership_authority: missing {membership_id}"))?;
        if challenge.is_empty() || membership.authority_key.is_empty() {
            return Err("exercise_membership_authority: missing challenge or key".to_string());
        }
        Ok(Sha256::new()
            .chain_update(b"osl-membership-authority-v1\0")
            .chain_update(membership_id.as_bytes())
            .chain_update(&membership.authority_key)
            .chain_update(challenge)
            .finalize()
            .into())
    }

    pub fn authorize_recovery_device(
        &mut self,
        recovery_secret: &[u8],
        device: DeviceRecord,
    ) -> Result<(), String> {
        if !secret_eq(
            &self.snapshot.device_recovery.recovery_secret,
            recovery_secret,
        ) {
            return Err("complete_recovery_device_authority: recovery secret mismatch".to_string());
        }
        if device.device_id.is_empty()
            || device.public_key.is_empty()
            || !device.authorized
            || self
                .snapshot
                .device_recovery
                .devices
                .iter()
                .any(|present| present.device_id == device.device_id)
        {
            return Err(
                "complete_recovery_device_authority: invalid or duplicate device".to_string(),
            );
        }
        let mut next = self.snapshot.clone();
        next.device_recovery.devices.push(device);
        self.commit(next)
    }

    fn commit(&mut self, next: AccountSnapshot) -> Result<(), String> {
        next.validate()?;
        let bytes = seal_archive(&next, self.archive_passphrase.as_str())?;
        crate::atomic_file::write_recoverable(&self.state_file, &bytes, "restored account-state")?;
        self.snapshot = next;
        Ok(())
    }
}

fn account_message_key(material: &[u8]) -> crypto::aead::Key {
    let bytes: [u8; 32] = Sha256::new()
        .chain_update(b"osl-restored-account-message-v1\0")
        .chain_update(material)
        .finalize()
        .into();
    crypto::aead::Key::from_bytes(bytes)
}

fn message_ad(conversation_id: &str, message_id: &str) -> Vec<u8> {
    let mut ad = Vec::with_capacity(conversation_id.len() + message_id.len() + 32);
    ad.extend_from_slice(b"osl-restored-message-v1\0");
    ad.extend_from_slice(conversation_id.as_bytes());
    ad.push(0);
    ad.extend_from_slice(message_id.as_bytes());
    ad
}

fn secret_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter()
        .zip(right)
        .fold(0u8, |different, (a, b)| different | (a ^ b))
        == 0
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AcceptanceMode {
    PackagedRoundTrip,
    ChecksumOnly,
    ManifestOnly,
    OfflineReaderOnly,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FixedOracle {
    pub class_counts: ClassCounts,
    pub class_sha256: BTreeMap<String, String>,
    pub message_bodies: BTreeMap<String, Vec<u8>>,
    pub attachment_bytes: BTreeMap<String, Vec<u8>>,
    pub foreign_owner_canary_ids: BTreeSet<String>,
}

impl FixedOracle {
    pub fn before_export(
        source: &AccountSnapshot,
        foreign_owner_canary_ids: impl IntoIterator<Item = String>,
    ) -> Result<Self, String> {
        let snapshot = source.owned_only();
        snapshot.validate()?;
        let mut class_sha256 = BTreeMap::new();
        class_sha256.insert(
            "identity_and_keys".to_string(),
            value_digest(&snapshot.identity_and_keys)?,
        );
        class_sha256.insert("friends".to_string(), value_digest(&snapshot.friends)?);
        class_sha256.insert(
            "conversations".to_string(),
            value_digest(&snapshot.conversations)?,
        );
        class_sha256.insert(
            "attachments".to_string(),
            value_digest(&snapshot.attachments)?,
        );
        class_sha256.insert("settings".to_string(), value_digest(&snapshot.settings)?);
        class_sha256.insert(
            "memberships".to_string(),
            value_digest(&snapshot.memberships)?,
        );
        class_sha256.insert(
            "device_recovery".to_string(),
            value_digest(&snapshot.device_recovery)?,
        );
        let message_bodies = snapshot
            .conversations
            .iter()
            .flat_map(|conversation| &conversation.messages)
            .map(|message| (message.message_id.clone(), message.body.clone()))
            .collect();
        let attachment_bytes = snapshot
            .attachments
            .iter()
            .map(|attachment| (attachment.attachment_id.clone(), attachment.bytes.clone()))
            .collect();
        Ok(Self {
            class_counts: snapshot.counts(),
            class_sha256,
            message_bodies,
            attachment_bytes,
            foreign_owner_canary_ids: foreign_owner_canary_ids.into_iter().collect(),
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AcceptanceEvidence {
    pub mode: AcceptanceMode,
    pub clean_packaged_install: bool,
    pub original_local_state_available: bool,
    pub source_dependencies: Vec<String>,
    pub disclosure: ExportDisclosure,
    pub payment_records_present: bool,
    pub authenticated_account_id: Option<String>,
    pub restored_snapshot: Option<AccountSnapshot>,
    pub opened_messages: BTreeMap<String, Vec<u8>>,
    pub opened_attachments: BTreeMap<String, Vec<u8>>,
    pub new_encrypted_message_sent: bool,
    pub new_encrypted_message_received: bool,
    pub membership_authority_exercised: bool,
    pub recovery_device_authority_completed: bool,
}

/// The acceptance result is post-import product behavior. Digest fields in the
/// fixed oracle help pinpoint a class mismatch but never grant acceptance.
pub fn verify_packaged_round_trip(
    oracle: &FixedOracle,
    evidence: &AcceptanceEvidence,
) -> Result<(), String> {
    match evidence.mode {
        AcceptanceMode::PackagedRoundTrip => {}
        AcceptanceMode::ChecksumOnly => {
            return Err("broken post-import function: checksum_only".to_string())
        }
        AcceptanceMode::ManifestOnly => {
            return Err("broken post-import function: manifest_only".to_string())
        }
        AcceptanceMode::OfflineReaderOnly => {
            return Err("broken post-import function: offline_reader_only".to_string())
        }
    }
    if !evidence.clean_packaged_install || evidence.original_local_state_available {
        return Err(
            "stale dependency: restore was not isolated from original local state".to_string(),
        );
    }
    if !evidence.source_dependencies.is_empty() {
        return Err(format!(
            "stale dependency: {}",
            evidence.source_dependencies.join(", ")
        ));
    }
    require_disclosure(
        &evidence.disclosure.named_exclusions,
        &evidence.disclosure.notice,
    )?;
    if evidence.payment_records_present {
        return Err("hidden exclusion: payment records were present".to_string());
    }
    let snapshot = evidence
        .restored_snapshot
        .as_ref()
        .ok_or_else(|| "broken post-import function: inspect_exact_state".to_string())?;
    if evidence.authenticated_account_id.as_deref()
        != Some(snapshot.identity_and_keys.account_id.as_str())
    {
        return Err("broken post-import function: authenticate_restored_identity".to_string());
    }
    let observed_counts = snapshot.counts();
    compare_count(
        "identity_and_keys",
        oracle.class_counts.identity_and_keys,
        observed_counts.identity_and_keys,
    )?;
    compare_count(
        "friends",
        oracle.class_counts.friends,
        observed_counts.friends,
    )?;
    compare_count(
        "conversations",
        oracle.class_counts.conversations,
        observed_counts.conversations,
    )?;
    compare_count(
        "attachments",
        oracle.class_counts.attachments,
        observed_counts.attachments,
    )?;
    compare_count(
        "settings",
        oracle.class_counts.settings,
        observed_counts.settings,
    )?;
    compare_count(
        "memberships",
        oracle.class_counts.memberships,
        observed_counts.memberships,
    )?;
    compare_count(
        "device_recovery",
        oracle.class_counts.devices,
        observed_counts.devices,
    )?;
    for (class, expected) in &oracle.class_sha256 {
        let actual = snapshot_class_digest(snapshot, class)?;
        if &actual != expected {
            return Err(format!("missing or changed required class: {class}"));
        }
    }
    for (id, body) in &oracle.message_bodies {
        if evidence.opened_messages.get(id) != Some(body) {
            return Err(format!(
                "broken post-import function: open_every_message ({id})"
            ));
        }
    }
    for (id, bytes) in &oracle.attachment_bytes {
        if evidence.opened_attachments.get(id) != Some(bytes) {
            return Err(format!(
                "broken post-import function: open_every_attachment ({id})"
            ));
        }
    }
    if !evidence.new_encrypted_message_sent || !evidence.new_encrypted_message_received {
        return Err("broken post-import function: send_receive_new_encrypted_message".to_string());
    }
    if !evidence.membership_authority_exercised {
        return Err("broken post-import function: exercise_membership_authority".to_string());
    }
    if !evidence.recovery_device_authority_completed {
        return Err("broken post-import function: complete_recovery_device_authority".to_string());
    }
    let serialized = serde_json::to_vec(snapshot)
        .map_err(|error| format!("acceptance snapshot serialize: {error}"))?;
    let text = String::from_utf8_lossy(&serialized);
    for canary in &oracle.foreign_owner_canary_ids {
        if text.contains(canary) {
            return Err(format!("foreign-owner canary leaked: {canary}"));
        }
    }
    Ok(())
}

fn compare_count(class: &str, expected: usize, actual: usize) -> Result<(), String> {
    if expected == actual {
        Ok(())
    } else {
        Err(format!(
            "missing or changed required class: {class} (expected {expected}, got {actual})"
        ))
    }
}

fn value_digest<T: Serialize>(value: &T) -> Result<String, String> {
    let bytes =
        serde_json::to_vec(value).map_err(|error| format!("oracle serialize failed: {error}"))?;
    Ok(hex_digest(&bytes))
}

fn snapshot_class_digest(snapshot: &AccountSnapshot, class: &str) -> Result<String, String> {
    match class {
        "identity_and_keys" => value_digest(&snapshot.identity_and_keys),
        "friends" => value_digest(&snapshot.friends),
        "conversations" => value_digest(&snapshot.conversations),
        "attachments" => value_digest(&snapshot.attachments),
        "settings" => value_digest(&snapshot.settings),
        "memberships" => value_digest(&snapshot.memberships),
        "device_recovery" => value_digest(&snapshot.device_recovery),
        unknown => Err(format!("unknown oracle class: {unknown}")),
    }
}

fn hex_digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Fail closed if the red proof starves any required class, journey, or
/// negative archive. The error always names the missing starvation target.
pub fn require_complete_proof(
    classes: impl IntoIterator<Item = String>,
    journeys: impl IntoIterator<Item = String>,
    mutants: impl IntoIterator<Item = String>,
) -> Result<(), String> {
    require_set("class starvation", &REQUIRED_CLASSES, classes)?;
    require_set("journey starvation", &REQUIRED_JOURNEYS, journeys)?;
    require_set("mutant starvation", &REQUIRED_MUTANTS, mutants)
}

fn require_set<const N: usize>(
    label: &str,
    expected: &[&str; N],
    actual: impl IntoIterator<Item = String>,
) -> Result<(), String> {
    let actual: BTreeSet<String> = actual.into_iter().collect();
    for required in expected {
        if !actual.contains(*required) {
            return Err(format!("{label}: {required}"));
        }
    }
    for unknown in &actual {
        if !expected.contains(&unknown.as_str()) {
            return Err(format!("unknown {label}: {unknown}"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const PASSPHRASE: &str = "task-6572-independent-passphrase";

    fn fixture() -> AccountSnapshot {
        AccountSnapshot {
            identity_and_keys: IdentityAndKeys {
                account_id: "owner-6572".to_string(),
                display_name: "Archive Owner".to_string(),
                authentication_secret: vec![1, 2, 3, 4],
                identity_private_key: vec![5; 48],
                identity_public_key: vec![6; 48],
                message_key: vec![7; 32],
            },
            friends: vec![FriendRecord {
                friend_id: "friend-6572".to_string(),
                owner_account_id: "owner-6572".to_string(),
                display_name: "Friend".to_string(),
                public_key: vec![8; 32],
            }],
            conversations: vec![ConversationRecord {
                conversation_id: "conversation-6572".to_string(),
                owner_account_id: "owner-6572".to_string(),
                member_ids: vec!["friend-6572".to_string()],
                messages: vec![MessageRecord {
                    message_id: "message-6572".to_string(),
                    owner_account_id: "owner-6572".to_string(),
                    author_id: "friend-6572".to_string(),
                    body: vec![0, 255, 17, 99],
                    sent_at_ms: 6572,
                }],
            }],
            attachments: vec![AttachmentRecord {
                attachment_id: "attachment-6572".to_string(),
                owner_account_id: "owner-6572".to_string(),
                message_id: "message-6572".to_string(),
                filename: "unpredictable.bin".to_string(),
                media_type: "application/octet-stream".to_string(),
                bytes: vec![13, 0, 255, 22, 91],
            }],
            settings: BTreeMap::from([("theme".to_string(), b"midnight".to_vec())]),
            memberships: vec![MembershipRecord {
                membership_id: "membership-6572".to_string(),
                owner_account_id: "owner-6572".to_string(),
                role: "owner".to_string(),
                authority_key: vec![9; 32],
            }],
            device_recovery: DeviceRecoveryState {
                owner_account_id: "owner-6572".to_string(),
                recovery_secret: vec![10; 32],
                devices: vec![DeviceRecord {
                    device_id: "device-6572".to_string(),
                    public_key: vec![11; 32],
                    authorized: true,
                }],
            },
        }
    }

    fn envelope(bytes: &[u8]) -> ArchiveEnvelope {
        serde_json::from_slice(bytes).expect("fixture envelope")
    }

    fn encode(envelope: &ArchiveEnvelope) -> Vec<u8> {
        serde_json::to_vec(envelope).expect("encode mutant")
    }

    fn metadata_mutant(bytes: &[u8], change: impl FnOnce(&mut ArchiveMetadata)) -> Vec<u8> {
        let mut envelope = envelope(bytes);
        let key = derive_archive_key(PASSPHRASE, &envelope.kdf.salt, envelope.kdf.rounds)
            .expect("derive fixture key");
        let mut metadata: ArchiveMetadata =
            open_part(&key, &[], "metadata", &envelope.metadata).expect("open metadata");
        change(&mut metadata);
        envelope.metadata = seal_part(&key, &[], "metadata", &metadata).expect("reseal metadata");
        encode(&envelope)
    }

    fn evidence(snapshot: &AccountSnapshot, mode: AcceptanceMode) -> AcceptanceEvidence {
        AcceptanceEvidence {
            mode,
            clean_packaged_install: true,
            original_local_state_available: false,
            source_dependencies: Vec::new(),
            disclosure: ExportDisclosure::default(),
            payment_records_present: false,
            authenticated_account_id: Some(snapshot.identity_and_keys.account_id.clone()),
            restored_snapshot: Some(snapshot.clone()),
            opened_messages: snapshot
                .conversations
                .iter()
                .flat_map(|conversation| &conversation.messages)
                .map(|message| (message.message_id.clone(), message.body.clone()))
                .collect(),
            opened_attachments: snapshot
                .attachments
                .iter()
                .map(|attachment| (attachment.attachment_id.clone(), attachment.bytes.clone()))
                .collect(),
            new_encrypted_message_sent: true,
            new_encrypted_message_received: true,
            membership_authority_exercised: true,
            recovery_device_authority_completed: true,
        }
    }

    #[test]
    fn task_6573_every_required_mutant_exits_one_with_a_named_reason() {
        let source = fixture();
        let oracle = FixedOracle::before_export(&source, Vec::<String>::new()).expect("oracle");
        let archive = seal_archive(&source, PASSPHRASE).expect("archive");
        let mut observed = BTreeSet::new();

        for class in REQUIRED_CLASSES {
            let mutant = format!("omit_{class}");
            let mut changed = envelope(&archive);
            changed.classes.remove(class);
            let error = open_archive(&encode(&changed), PASSPHRASE).unwrap_err();
            assert!(error.contains(class), "{mutant}: {error}");
            println!("TASK6573_EXIT=1 mutant={mutant} reason={error}");
            observed.insert(mutant);
        }
        for class in REQUIRED_CLASSES {
            let mutant = format!("corrupt_{class}");
            let mut changed = envelope(&archive);
            changed
                .classes
                .get_mut(class)
                .expect("required class")
                .ciphertext[0] ^= 0x80;
            let error = open_archive(&encode(&changed), PASSPHRASE).unwrap_err();
            assert!(error.contains(class), "{mutant}: {error}");
            println!("TASK6573_EXIT=1 mutant={mutant} reason={error}");
            observed.insert(mutant);
        }

        let stale = metadata_mutant(&archive, |metadata| {
            metadata
                .source_dependencies
                .push("original-profile/cache/database/device-secret".to_string());
        });
        let error = open_archive(&stale, PASSPHRASE).unwrap_err();
        assert!(error.contains("stale dependency"));
        println!("TASK6573_EXIT=1 mutant=depends_on_original_state reason={error}");
        observed.insert("depends_on_original_state".to_string());

        for (mutant, mode) in [
            ("checksum_only", AcceptanceMode::ChecksumOnly),
            ("manifest_only", AcceptanceMode::ManifestOnly),
            ("offline_reader_only", AcceptanceMode::OfflineReaderOnly),
        ] {
            let error = verify_packaged_round_trip(&oracle, &evidence(&source, mode)).unwrap_err();
            assert!(error.contains(mutant), "{mutant}: {error}");
            println!("TASK6573_EXIT=1 mutant={mutant} reason={error}");
            observed.insert(mutant.to_string());
        }

        let hidden = metadata_mutant(&archive, |metadata| {
            metadata.deliberate_exclusions.clear();
        });
        let error = open_archive(&hidden, PASSPHRASE).unwrap_err();
        assert!(error.contains("hidden exclusion"));
        println!("TASK6573_EXIT=1 mutant=omit_exclusion reason={error}");
        observed.insert("omit_exclusion".to_string());

        let mut unknown = envelope(&archive);
        unknown.classes.insert(
            "unknown_future_class".to_string(),
            unknown.classes.get("friends").expect("friends").clone(),
        );
        let error = open_archive(&encode(&unknown), PASSPHRASE).unwrap_err();
        assert!(error.contains("unknown archive class"));
        println!("TASK6573_EXIT=1 mutant=add_unknown_class reason={error}");
        observed.insert("add_unknown_class".to_string());

        require_complete_proof(
            REQUIRED_CLASSES.into_iter().map(str::to_string),
            REQUIRED_JOURNEYS.into_iter().map(str::to_string),
            observed,
        )
        .expect("all mutants exercised");
        println!("TASK6573_MUTANTS={}", REQUIRED_MUTANTS.len());
    }

    #[test]
    fn task_6573_starvation_removal_names_every_absent_item() {
        let classes = REQUIRED_CLASSES
            .into_iter()
            .map(str::to_string)
            .collect::<Vec<_>>();
        let journeys = REQUIRED_JOURNEYS
            .into_iter()
            .map(str::to_string)
            .collect::<Vec<_>>();
        let mutants = REQUIRED_MUTANTS
            .into_iter()
            .map(str::to_string)
            .collect::<Vec<_>>();

        for missing in &classes {
            let present = classes
                .iter()
                .filter(|item| *item != missing)
                .cloned()
                .collect::<Vec<_>>();
            let error = require_complete_proof(present, journeys.clone(), mutants.clone())
                .expect_err("class starvation must fail");
            assert!(error.contains(missing), "{missing}: {error}");
            println!("TASK6573_EXIT=1 starvation=class absent={missing} reason={error}");
        }
        for missing in &journeys {
            let present = journeys
                .iter()
                .filter(|item| *item != missing)
                .cloned()
                .collect::<Vec<_>>();
            let error = require_complete_proof(classes.clone(), present, mutants.clone())
                .expect_err("journey starvation must fail");
            assert!(error.contains(missing), "{missing}: {error}");
            println!("TASK6573_EXIT=1 starvation=journey absent={missing} reason={error}");
        }
        for missing in &mutants {
            let present = mutants
                .iter()
                .filter(|item| *item != missing)
                .cloned()
                .collect::<Vec<_>>();
            let error = require_complete_proof(classes.clone(), journeys.clone(), present)
                .expect_err("mutant starvation must fail");
            assert!(error.contains(missing), "{missing}: {error}");
            println!("TASK6573_EXIT=1 starvation=mutant absent={missing} reason={error}");
        }
        println!("TASK6573_STARVED_CLASSES={}", classes.len());
        println!("TASK6573_STARVED_JOURNEYS={}", journeys.len());
        println!("TASK6573_STARVED_MUTANTS={}", mutants.len());
    }
}
