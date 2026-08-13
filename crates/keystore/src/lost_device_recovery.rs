//! Lost-all-devices recovery with rotating, single-use authorities.
//!
//! This protocol is deliberately separate from ordinary device pairing. A
//! pairing request proves possession of an already-authorized device; a lost
//! device recovery request proves possession of the current recovery kit.
//! Successful recovery consumes that kit authority and publishes a different
//! successor authority at exactly one higher epoch. "Single use" therefore
//! prevents replay and races, rather than imposing a lifetime recovery limit.

use crate::identity::{DevicePrivateKeys, DevicePublicKeys};
use crypto::ed25519;
use std::{
    collections::BTreeMap,
    fmt, fs,
    fs::OpenOptions,
    io::Write,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use zeroize::Zeroizing;

/// Published wire schema. All integers are unsigned, 64-bit, big-endian.
/// Every key is the 32-byte RFC 8032 Ed25519 encoding and every signature is
/// the 64-byte RFC 8032 encoding.
pub const LOST_DEVICE_RECOVERY_DECLARATION_SCHEMA: &str =
    "osl-lost-all-devices-recovery-declaration/v1";
pub const LOST_DEVICE_RECOVERY_DECLARATION_DOMAIN: &[u8] =
    b"OSL-lost-all-devices-recovery-declaration-v1";
pub const LOST_DEVICE_RECOVERY_KIT_SCHEMA: &str = "osl-lost-all-devices-recovery-kit/v1";
pub const LOST_DEVICE_RECOVERY_KIT_DOMAIN: &[u8] = b"OSL-lost-all-devices-recovery-kit-v1";
pub const LOST_DEVICE_RECOVERY_STATE_DOMAIN: &[u8] = b"OSL-lost-all-devices-recovery-state-v1";
pub const ORDINARY_PAIRING_EXISTING_DEVICE_REQUIRED: &str =
    "Confirm this on the device you already have.";
const LOST_DEVICE_ROSTER_DOMAIN: &[u8] = b"OSL-lost-all-devices-roster-v1";
const LOST_DEVICE_MESSAGE_DOMAIN: &[u8] = b"OSL-lost-all-devices-message-v1";

pub const LOST_DEVICE_RECOVERY_KEY_BYTES: usize = ed25519::PUBLIC_KEY_SIZE;
pub const LOST_DEVICE_RECOVERY_SIGNATURE_BYTES: usize = ed25519::SIGNATURE_SIZE;
pub const LOST_DEVICE_RECOVERY_KIT_BYTES: usize =
    LOST_DEVICE_RECOVERY_KIT_DOMAIN.len() + 8 + ed25519::PUBLIC_KEY_SIZE + ed25519::SECRET_KEY_SIZE;

/// The independently verifiable declaration accepted by the service.
///
/// Canonical signed bytes are exactly:
/// `DOMAIN || recovery_epoch_be_u64 || replacement_device_key || successor_recovery_authority`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LostDeviceRecoveryDeclaration {
    pub recovery_epoch: u64,
    pub replacement_device_key: [u8; ed25519::PUBLIC_KEY_SIZE],
    pub successor_recovery_authority: [u8; ed25519::PUBLIC_KEY_SIZE],
    pub kit_authority_signature: [u8; ed25519::SIGNATURE_SIZE],
}

/// Public canonicalization shared by the production client, external signers,
/// and independent verifiers. It contains no service lookup or bearer token.
pub fn canonical_lost_device_recovery_declaration_bytes(
    recovery_epoch: u64,
    replacement_device_key: [u8; ed25519::PUBLIC_KEY_SIZE],
    successor_recovery_authority: [u8; ed25519::PUBLIC_KEY_SIZE],
) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(
        LOST_DEVICE_RECOVERY_DECLARATION_DOMAIN.len() + 8 + ed25519::PUBLIC_KEY_SIZE * 2,
    );
    bytes.extend_from_slice(LOST_DEVICE_RECOVERY_DECLARATION_DOMAIN);
    bytes.extend_from_slice(&recovery_epoch.to_be_bytes());
    bytes.extend_from_slice(&replacement_device_key);
    bytes.extend_from_slice(&successor_recovery_authority);
    bytes
}

/// Offline verification of domain, epoch, replacement key, successor
/// authority, and kit-authority signature. It never consults the service.
pub fn verify_lost_device_recovery_declaration(
    kit_authority: [u8; ed25519::PUBLIC_KEY_SIZE],
    declaration: &LostDeviceRecoveryDeclaration,
) -> Result<(), LostDeviceRecoveryError> {
    let signed = canonical_lost_device_recovery_declaration_bytes(
        declaration.recovery_epoch,
        declaration.replacement_device_key,
        declaration.successor_recovery_authority,
    );
    let valid = ed25519::verify(
        &ed25519::PublicKey::from_bytes(kit_authority),
        &signed,
        &ed25519::Signature::from_bytes(declaration.kit_authority_signature),
    )
    .unwrap_or(false);
    if valid {
        Ok(())
    } else {
        Err(LostDeviceRecoveryError::InvalidSignature)
    }
}

/// Secret recovery artifact. The current epoch is stored beside its authority
/// so a clean replacement profile needs no process state from a lost device.
#[derive(Clone)]
pub struct LostDeviceRecoveryKit {
    recovery_epoch: u64,
    authority_secret: ed25519::SecretKey,
    authority_public: ed25519::PublicKey,
}

impl LostDeviceRecoveryKit {
    pub fn generate(recovery_epoch: u64) -> Self {
        let (authority_secret, authority_public) = ed25519::generate_keypair();
        Self {
            recovery_epoch,
            authority_secret,
            authority_public,
        }
    }

    pub fn from_secret(
        recovery_epoch: u64,
        authority_secret: [u8; ed25519::SECRET_KEY_SIZE],
    ) -> Self {
        let authority_secret = ed25519::SecretKey::from_bytes(authority_secret);
        let authority_public = ed25519::derive_public(&authority_secret);
        Self {
            recovery_epoch,
            authority_secret,
            authority_public,
        }
    }

    pub fn recovery_epoch(&self) -> u64 {
        self.recovery_epoch
    }

    pub fn public_authority(&self) -> [u8; ed25519::PUBLIC_KEY_SIZE] {
        *self.authority_public.as_bytes()
    }

    pub fn to_bytes(&self) -> Zeroizing<Vec<u8>> {
        let mut bytes = Zeroizing::new(Vec::with_capacity(LOST_DEVICE_RECOVERY_KIT_BYTES));
        bytes.extend_from_slice(LOST_DEVICE_RECOVERY_KIT_DOMAIN);
        bytes.extend_from_slice(&self.recovery_epoch.to_be_bytes());
        bytes.extend_from_slice(self.authority_public.as_bytes());
        bytes.extend_from_slice(self.authority_secret.as_bytes());
        bytes
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, LostDeviceRecoveryError> {
        if bytes.len() != LOST_DEVICE_RECOVERY_KIT_BYTES
            || !bytes.starts_with(LOST_DEVICE_RECOVERY_KIT_DOMAIN)
        {
            return Err(LostDeviceRecoveryError::InvalidKit);
        }
        let mut cursor = LOST_DEVICE_RECOVERY_KIT_DOMAIN.len();
        let epoch = u64::from_be_bytes(
            bytes[cursor..cursor + 8]
                .try_into()
                .map_err(|_| LostDeviceRecoveryError::InvalidKit)?,
        );
        cursor += 8;
        let public: [u8; ed25519::PUBLIC_KEY_SIZE] = bytes
            [cursor..cursor + ed25519::PUBLIC_KEY_SIZE]
            .try_into()
            .map_err(|_| LostDeviceRecoveryError::InvalidKit)?;
        cursor += ed25519::PUBLIC_KEY_SIZE;
        let secret: [u8; ed25519::SECRET_KEY_SIZE] = bytes
            [cursor..cursor + ed25519::SECRET_KEY_SIZE]
            .try_into()
            .map_err(|_| LostDeviceRecoveryError::InvalidKit)?;
        let kit = Self::from_secret(epoch, secret);
        if kit.public_authority() != public {
            return Err(LostDeviceRecoveryError::InvalidKit);
        }
        Ok(kit)
    }

    /// Open a kit written by [`Self::save_protected`].
    ///
    /// A wrong passphrase, a wrong domain or one altered byte returns
    /// `KitProtection` and no key material at all.
    pub fn open_protected(path: &Path, passphrase: &str) -> Result<Self, LostDeviceRecoveryError> {
        let sealed =
            fs::read(path).map_err(|error| LostDeviceRecoveryError::KitIo(error.to_string()))?;
        let body = crate::secret_at_rest::open_with_passphrase(
            LOST_DEVICE_RECOVERY_KIT_DOMAIN,
            passphrase,
            &sealed,
        )
        .map_err(|error| LostDeviceRecoveryError::KitProtection(error.to_string()))?;
        let kit = Self::from_bytes(&body)?;
        crate::secret_trace::record(
            crate::secret_trace::SecretOp::Read,
            crate::secret_trace::SecretClass::RecoveryAuthority,
            crate::secret_trace::Protection::UserDerivedAead,
            "keystore::lost_device_recovery::LostDeviceRecoveryKit::open_protected",
            path,
            sealed.len(),
        );
        Ok(kit)
    }

    /// Create a new file; never overwrite another kit.
    ///
    /// TASK 5402: this used to be `save`, writing [`Self::to_bytes`] straight
    /// to disk — the thirty-two-byte Ed25519 recovery-authority seed in the
    /// clear, in the artifact a user is told to keep somewhere durable and
    /// off-device. It is now authenticated ciphertext under an Argon2id key
    /// derived from `passphrase` and from nothing that is stored beside it.
    /// User-derived rather than device-sealed, because this kit's whole job is
    /// to still open when every device is gone.
    pub fn save_protected(
        &self,
        path: &Path,
        passphrase: &str,
    ) -> Result<(), LostDeviceRecoveryError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| LostDeviceRecoveryError::KitIo(error.to_string()))?;
        }
        let sealed = crate::secret_at_rest::seal_with_passphrase(
            LOST_DEVICE_RECOVERY_KIT_DOMAIN,
            passphrase,
            &self.to_bytes(),
        )
        .map_err(|error| LostDeviceRecoveryError::KitProtection(error.to_string()))?;
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|error| LostDeviceRecoveryError::KitIo(error.to_string()))?;
        file.write_all(&sealed)
            .and_then(|_| file.sync_all())
            .map_err(|error| LostDeviceRecoveryError::KitIo(error.to_string()))?;
        crate::secret_trace::record(
            crate::secret_trace::SecretOp::Write,
            crate::secret_trace::SecretClass::RecoveryAuthority,
            crate::secret_trace::Protection::UserDerivedAead,
            "keystore::lost_device_recovery::LostDeviceRecoveryKit::save_protected",
            path,
            sealed.len(),
        );
        Ok(())
    }

    pub fn prepare_replacement(
        &self,
        replacement_device_key: [u8; ed25519::PUBLIC_KEY_SIZE],
    ) -> Result<PreparedLostDeviceRecovery, LostDeviceRecoveryError> {
        let next_epoch = self
            .recovery_epoch
            .checked_add(1)
            .ok_or(LostDeviceRecoveryError::RecoveryEpochOverflow)?;
        let successor_kit = Self::generate(next_epoch);
        let successor_recovery_authority = successor_kit.public_authority();
        let signed = canonical_lost_device_recovery_declaration_bytes(
            next_epoch,
            replacement_device_key,
            successor_recovery_authority,
        );
        let declaration = LostDeviceRecoveryDeclaration {
            recovery_epoch: next_epoch,
            replacement_device_key,
            successor_recovery_authority,
            kit_authority_signature: *ed25519::sign(&self.authority_secret, &signed).as_bytes(),
        };
        Ok(PreparedLostDeviceRecovery {
            declaration,
            successor_kit,
        })
    }
}

pub struct PreparedLostDeviceRecovery {
    pub declaration: LostDeviceRecoveryDeclaration,
    pub successor_kit: LostDeviceRecoveryKit,
}

/// Recovery control state: exactly the current authority and monotonic epoch.
/// There is intentionally no success count, terminal epoch, or lifetime cap.
#[derive(Clone, Debug, PartialEq, Eq)]
struct LostDeviceRecoveryState {
    current_authority: [u8; ed25519::PUBLIC_KEY_SIZE],
    recovery_epoch: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecoveryAuthorization {
    pub recovery_epoch: u64,
    pub replacement_device_key: [u8; ed25519::PUBLIC_KEY_SIZE],
    pub successor_recovery_authority: [u8; ed25519::PUBLIC_KEY_SIZE],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecoveryMessage {
    pub sender_account: String,
    pub body: String,
}

struct LostDeviceRecoveryServiceInner {
    recovery: LostDeviceRecoveryState,
    authorized_device_keys: Vec<[u8; ed25519::PUBLIC_KEY_SIZE]>,
    published_declaration: Option<LostDeviceRecoveryDeclaration>,
    inboxes: BTreeMap<[u8; ed25519::PUBLIC_KEY_SIZE], Vec<RecoveryMessage>>,
}

/// Cloneable running-service handle. One lock covers compare, consume, roster
/// replacement, authority rotation, epoch publication, and transition receipt.
#[derive(Clone)]
pub struct LostDeviceRecoveryService {
    inner: Arc<Mutex<LostDeviceRecoveryServiceInner>>,
}

impl LostDeviceRecoveryService {
    /// The running service issues and saves the initial kit, then retains only
    /// its public authority and epoch. Initial device keys model the three old
    /// packaged profiles and are not needed by the recovery endpoint.
    pub fn bootstrap(
        initial_device_keys: Vec<[u8; ed25519::PUBLIC_KEY_SIZE]>,
        issued_kit_path: &Path,
        issued_kit_passphrase: &str,
    ) -> Result<(Self, LostDeviceRecoveryKit), LostDeviceRecoveryError> {
        let kit = LostDeviceRecoveryKit::generate(0);
        kit.save_protected(issued_kit_path, issued_kit_passphrase)?;
        let service = Self {
            inner: Arc::new(Mutex::new(LostDeviceRecoveryServiceInner {
                recovery: LostDeviceRecoveryState {
                    current_authority: kit.public_authority(),
                    recovery_epoch: 0,
                },
                authorized_device_keys: initial_device_keys,
                published_declaration: None,
                inboxes: BTreeMap::new(),
            })),
        };
        Ok((service, kit))
    }

    /// The lost-all-devices endpoint. It accepts only the signed declaration;
    /// there is no old-device private key, confirmation, process-state handle,
    /// or bearer lookup parameter.
    pub fn authorize_replacement(
        &self,
        declaration: LostDeviceRecoveryDeclaration,
    ) -> Result<RecoveryAuthorization, LostDeviceRecoveryError> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| LostDeviceRecoveryError::ServiceUnavailable)?;
        verify_lost_device_recovery_declaration(inner.recovery.current_authority, &declaration)?;
        let expected_epoch = inner
            .recovery
            .recovery_epoch
            .checked_add(1)
            .ok_or(LostDeviceRecoveryError::RecoveryEpochOverflow)?;
        if declaration.recovery_epoch != expected_epoch {
            return Err(LostDeviceRecoveryError::RecoveryEpochNotNext {
                current: inner.recovery.recovery_epoch,
                proposed: declaration.recovery_epoch,
            });
        }
        if declaration.replacement_device_key == [0; ed25519::PUBLIC_KEY_SIZE] {
            return Err(LostDeviceRecoveryError::InvalidReplacementDeviceKey);
        }
        if declaration.successor_recovery_authority == [0; ed25519::PUBLIC_KEY_SIZE] {
            return Err(LostDeviceRecoveryError::MissingSuccessorAuthority);
        }
        if declaration.successor_recovery_authority == inner.recovery.current_authority {
            return Err(LostDeviceRecoveryError::SuccessorAuthorityUnchanged);
        }

        // Atomic commit under the same lock used by every competing request.
        // There is no retaining/replay table: rotation itself invalidates the
        // consumed kit because its signature no longer verifies as current.
        inner.recovery = LostDeviceRecoveryState {
            current_authority: declaration.successor_recovery_authority,
            recovery_epoch: declaration.recovery_epoch,
        };
        inner.authorized_device_keys = vec![declaration.replacement_device_key];
        inner.published_declaration = Some(declaration.clone());
        inner.inboxes.clear();

        Ok(RecoveryAuthorization {
            recovery_epoch: declaration.recovery_epoch,
            replacement_device_key: declaration.replacement_device_key,
            successor_recovery_authority: declaration.successor_recovery_authority,
        })
    }

    pub fn recovery_epoch(&self) -> u64 {
        self.inner
            .lock()
            .expect("recovery service mutex poisoned")
            .recovery
            .recovery_epoch
    }

    pub fn current_authority(&self) -> [u8; ed25519::PUBLIC_KEY_SIZE] {
        self.inner
            .lock()
            .expect("recovery service mutex poisoned")
            .recovery
            .current_authority
    }

    pub fn recovery_state_bytes(&self) -> Vec<u8> {
        let inner = self.inner.lock().expect("recovery service mutex poisoned");
        let mut bytes = Vec::with_capacity(
            LOST_DEVICE_RECOVERY_STATE_DOMAIN.len() + ed25519::PUBLIC_KEY_SIZE + 8,
        );
        bytes.extend_from_slice(LOST_DEVICE_RECOVERY_STATE_DOMAIN);
        bytes.extend_from_slice(&inner.recovery.current_authority);
        bytes.extend_from_slice(&inner.recovery.recovery_epoch.to_be_bytes());
        bytes
    }

    pub fn authorized_device_keys(&self) -> Vec<[u8; ed25519::PUBLIC_KEY_SIZE]> {
        self.inner
            .lock()
            .expect("recovery service mutex poisoned")
            .authorized_device_keys
            .clone()
    }

    pub fn roster_bytes(&self) -> Vec<u8> {
        let inner = self.inner.lock().expect("recovery service mutex poisoned");
        let mut bytes = Vec::with_capacity(
            LOST_DEVICE_ROSTER_DOMAIN.len()
                + 4
                + inner.authorized_device_keys.len() * ed25519::PUBLIC_KEY_SIZE,
        );
        bytes.extend_from_slice(LOST_DEVICE_ROSTER_DOMAIN);
        bytes.extend_from_slice(&(inner.authorized_device_keys.len() as u32).to_be_bytes());
        for key in &inner.authorized_device_keys {
            bytes.extend_from_slice(key);
        }
        bytes
    }

    pub fn published_declaration(&self) -> Option<LostDeviceRecoveryDeclaration> {
        self.inner
            .lock()
            .expect("recovery service mutex poisoned")
            .published_declaration
            .clone()
    }

    /// Ordinary pairing remains old-device-authorized. Merely possessing the
    /// new device is insufficient; callers must supply an authorized existing
    /// device's private-key-bearing value.
    pub fn ordinary_pair_device(
        &self,
        existing_device: Option<&DevicePrivateKeys>,
        new_device: &DevicePrivateKeys,
    ) -> Result<(), LostDeviceRecoveryError> {
        let Some(existing_device) = existing_device else {
            return Err(LostDeviceRecoveryError::ExistingDeviceRequired);
        };
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| LostDeviceRecoveryError::ServiceUnavailable)?;
        let existing_key = replacement_key(existing_device.public_keys());
        if !inner.authorized_device_keys.contains(&existing_key) {
            return Err(LostDeviceRecoveryError::ExistingDeviceRequired);
        }
        let new_key = replacement_key(new_device.public_keys());
        if !inner.authorized_device_keys.contains(&new_key) {
            inner.authorized_device_keys.push(new_key);
        }
        Ok(())
    }

    pub fn deliver_new_message(
        &self,
        sender_account: impl Into<String>,
        body: impl Into<String>,
    ) -> Result<(), LostDeviceRecoveryError> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| LostDeviceRecoveryError::ServiceUnavailable)?;
        let Some(&only_active) = inner.authorized_device_keys.first() else {
            return Err(LostDeviceRecoveryError::NoAuthorizedDevice);
        };
        if inner.authorized_device_keys.len() != 1 {
            return Err(LostDeviceRecoveryError::RosterNotRecovered);
        }
        let mut authenticated_body = Vec::new();
        authenticated_body.extend_from_slice(LOST_DEVICE_MESSAGE_DOMAIN);
        authenticated_body.extend_from_slice(body.into().as_bytes());
        inner
            .inboxes
            .entry(only_active)
            .or_default()
            .push(RecoveryMessage {
                sender_account: sender_account.into(),
                body: String::from_utf8_lossy(
                    &authenticated_body[LOST_DEVICE_MESSAGE_DOMAIN.len()..],
                )
                .into_owned(),
            });
        Ok(())
    }

    fn take_messages(
        &self,
        device_key: [u8; ed25519::PUBLIC_KEY_SIZE],
    ) -> Result<Vec<RecoveryMessage>, LostDeviceRecoveryError> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| LostDeviceRecoveryError::ServiceUnavailable)?;
        if !inner.authorized_device_keys.contains(&device_key) {
            return Err(LostDeviceRecoveryError::UnauthorizedDevice);
        }
        Ok(inner.inboxes.remove(&device_key).unwrap_or_default())
    }
}

pub fn replacement_key(device: DevicePublicKeys) -> [u8; ed25519::PUBLIC_KEY_SIZE] {
    device.ed25519_public
}

/// A clean packaged replacement installation. It owns only its freshly
/// generated device keys, its empty profile directory, and (after success) the
/// newly saved successor kit.
pub struct PackagedReplacementProfile {
    profile_dir: PathBuf,
    device_keys: DevicePrivateKeys,
    device_key_path: PathBuf,
    successor_kit_path: PathBuf,
    /// The passphrase this profile's successor kit is sealed under. Held only
    /// in memory for the life of the recovery; nothing writes it to the
    /// profile, which is the whole point of TASK 5402's "never persisted in
    /// plaintext beside it".
    successor_kit_passphrase: String,
}

impl PackagedReplacementProfile {
    /// `successor_kit_passphrase` protects the successor recovery kit this
    /// profile will write. It is never stored in the profile.
    pub fn new_clean(
        profile_dir: impl Into<PathBuf>,
        device_name: impl Into<String>,
        successor_kit_passphrase: impl Into<String>,
    ) -> Result<Self, LostDeviceRecoveryError> {
        let sealer = crate::sealer::select_best_sealer();
        Self::new_clean_with_sealer(
            profile_dir,
            device_name,
            successor_kit_passphrase,
            sealer.as_ref(),
        )
    }

    /// [`Self::new_clean`] against an explicit device sealer.
    pub fn new_clean_with_sealer(
        profile_dir: impl Into<PathBuf>,
        device_name: impl Into<String>,
        successor_kit_passphrase: impl Into<String>,
        sealer: &dyn crate::sealer::Sealer,
    ) -> Result<Self, LostDeviceRecoveryError> {
        let profile_dir = profile_dir.into();
        let successor_kit_passphrase = successor_kit_passphrase.into();
        crate::secret_at_rest::validate_kit_passphrase(&successor_kit_passphrase)
            .map_err(|error| LostDeviceRecoveryError::KitProtection(error.to_string()))?;
        if profile_dir.exists() {
            let mut entries = fs::read_dir(&profile_dir)
                .map_err(|error| LostDeviceRecoveryError::ProfileIo(error.to_string()))?;
            if entries.next().is_some() {
                return Err(LostDeviceRecoveryError::ProfileNotClean);
            }
        } else {
            fs::create_dir_all(&profile_dir)
                .map_err(|error| LostDeviceRecoveryError::ProfileIo(error.to_string()))?;
        }
        let device_keys = DevicePrivateKeys::generate_on_device(device_name);
        let device_key_path = profile_dir.join("device-private-keys.bin");
        device_keys
            .save_sealed_private_key_file(&device_key_path, sealer)
            .map_err(|error| LostDeviceRecoveryError::ProfileIo(error.to_string()))?;
        let successor_kit_path = profile_dir.join("recovery-kit.osl");
        Ok(Self {
            profile_dir,
            device_keys,
            device_key_path,
            successor_kit_path,
            successor_kit_passphrase,
        })
    }

    pub fn successor_kit_passphrase(&self) -> &str {
        &self.successor_kit_passphrase
    }

    pub fn replacement_device_key(&self) -> [u8; ed25519::PUBLIC_KEY_SIZE] {
        replacement_key(self.device_keys.public_keys())
    }

    pub fn successor_kit_path(&self) -> &Path {
        &self.successor_kit_path
    }

    pub fn device_key_path(&self) -> &Path {
        &self.device_key_path
    }

    pub fn saved_successor_kit(&self) -> Result<LostDeviceRecoveryKit, LostDeviceRecoveryError> {
        LostDeviceRecoveryKit::open_protected(
            &self.successor_kit_path,
            &self.successor_kit_passphrase,
        )
    }

    pub fn receive_messages(
        &self,
        service: &LostDeviceRecoveryService,
    ) -> Result<Vec<RecoveryMessage>, LostDeviceRecoveryError> {
        service.take_messages(self.replacement_device_key())
    }

    pub fn profile_dir(&self) -> &Path {
        &self.profile_dir
    }
}

pub struct ProductionRecoveryClient;

impl ProductionRecoveryClient {
    pub fn construct_declaration(
        kit: &LostDeviceRecoveryKit,
        profile: &PackagedReplacementProfile,
    ) -> Result<PreparedLostDeviceRecovery, LostDeviceRecoveryError> {
        kit.prepare_replacement(profile.replacement_device_key())
    }

    pub fn submit_prepared(
        service: &LostDeviceRecoveryService,
        profile: &PackagedReplacementProfile,
        prepared: PreparedLostDeviceRecovery,
    ) -> Result<RecoveryAuthorization, LostDeviceRecoveryError> {
        // Save before release so a successful atomic grant can never be a
        // dead-end. A losing unaccepted successor is removed immediately.
        prepared.successor_kit.save_protected(
            profile.successor_kit_path(),
            profile.successor_kit_passphrase(),
        )?;
        match service.authorize_replacement(prepared.declaration) {
            Ok(authorization) => Ok(authorization),
            Err(error) => {
                let _ = fs::remove_file(profile.successor_kit_path());
                Err(error)
            }
        }
    }

    pub fn recover(
        service: &LostDeviceRecoveryService,
        kit: &LostDeviceRecoveryKit,
        profile: &PackagedReplacementProfile,
    ) -> Result<RecoveryAuthorization, LostDeviceRecoveryError> {
        let prepared = Self::construct_declaration(kit, profile)?;
        Self::submit_prepared(service, profile, prepared)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LostDeviceRecoveryError {
    InvalidSignature,
    InvalidKit,
    RecoveryEpochNotNext { current: u64, proposed: u64 },
    RecoveryEpochOverflow,
    InvalidReplacementDeviceKey,
    MissingSuccessorAuthority,
    SuccessorAuthorityUnchanged,
    ExistingDeviceRequired,
    UnauthorizedDevice,
    NoAuthorizedDevice,
    RosterNotRecovered,
    ServiceUnavailable,
    ProfileNotClean,
    KitIo(String),
    /// The kit's at-rest envelope refused: wrong passphrase, wrong domain,
    /// altered bytes, or weakened Argon2id parameters.
    KitProtection(String),
    ProfileIo(String),
}

impl fmt::Display for LostDeviceRecoveryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSignature => f.write_str("invalid kit-authority signature"),
            Self::InvalidKit => f.write_str("invalid recovery kit schema or authority bytes"),
            Self::RecoveryEpochNotNext { current, proposed } => write!(
                f,
                "recovery epoch is not exactly next (current {current}, proposed {proposed})"
            ),
            Self::RecoveryEpochOverflow => f.write_str("recovery epoch cannot advance"),
            Self::InvalidReplacementDeviceKey => f.write_str("replacement device key is invalid"),
            Self::MissingSuccessorAuthority => f.write_str("successor recovery kit is required"),
            Self::SuccessorAuthorityUnchanged => {
                f.write_str("successor recovery authority must rotate")
            }
            Self::ExistingDeviceRequired => f.write_str(ORDINARY_PAIRING_EXISTING_DEVICE_REQUIRED),
            Self::UnauthorizedDevice => f.write_str("device is not authorized"),
            Self::NoAuthorizedDevice => f.write_str("no device is authorized"),
            Self::RosterNotRecovered => f.write_str("recovery roster is not exactly one device"),
            Self::ServiceUnavailable => f.write_str("recovery service unavailable"),
            Self::ProfileNotClean => f.write_str("replacement profile is not clean"),
            Self::KitIo(error) => write!(f, "recovery kit I/O failed: {error}"),
            Self::KitProtection(detail) => {
                write!(f, "recovery kit at-rest protection refused: {detail}")
            }
            Self::ProfileIo(error) => write!(f, "replacement profile I/O failed: {error}"),
        }
    }
}

impl std::error::Error for LostDeviceRecoveryError {}
