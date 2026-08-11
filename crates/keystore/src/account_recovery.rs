//! Independent account-root recovery authority.
//!
//! The ordinary account root authorizes devices and rosters.  It deliberately
//! cannot authorize its own replacement: the recovery authority is generated
//! separately, pinned by the service as a public key, and its private seed is
//! written only to the recovery kit.  Local instances consume the same signed,
//! monotonic state and remember every compromised root so no later signature
//! by one can become valid again.

use crate::identity::{
    verify_signed_device_list, AccountRootKey, DeviceListError, SignedDeviceList, StoredDeviceList,
};
use crypto::ed25519;
use sha2::{Digest, Sha256};
use std::{fmt, fs, path::Path};
use zeroize::Zeroizing;

const RECOVERY_STATE_DOMAIN: &[u8] = b"OSL-account-recovery-state-v1";
const RECOVERY_KIT_DOMAIN: &[u8] = b"OSL-account-recovery-kit-v1";
const ACCOUNT_ROSTER_DOMAIN: &[u8] = b"OSL-account-roster-v1";
const SAFETY_NUMBER_DOMAIN: &[u8] = b"OSL-account-root-safety-number-v1";

pub const RECOVERY_PRIVATE_KEY_BYTES: usize = ed25519::SECRET_KEY_SIZE;

/// The only value that contains the independent recovery private key.
#[derive(Clone)]
pub struct RecoveryKit {
    recovery_secret: ed25519::SecretKey,
    recovery_public: ed25519::PublicKey,
    initial_root: [u8; ed25519::PUBLIC_KEY_SIZE],
}

impl RecoveryKit {
    /// Generate independently of the supplied active account root.  The root
    /// is only bound into the signed genesis state; it is not key material.
    pub fn generate(active_root: ed25519::PublicKey) -> Self {
        let (recovery_secret, recovery_public) = ed25519::generate_keypair();
        Self {
            recovery_secret,
            recovery_public,
            initial_root: *active_root.as_bytes(),
        }
    }

    pub fn public_authority(&self) -> [u8; ed25519::PUBLIC_KEY_SIZE] {
        *self.recovery_public.as_bytes()
    }

    pub fn initial_signed_state(&self) -> RecoveryDeclaration {
        self.sign(self.initial_root, 0, self.initial_root)
    }

    pub fn declare_recovery(
        &self,
        compromised_root: ed25519::PublicKey,
        recovery_epoch: u64,
        replacement_root: ed25519::PublicKey,
    ) -> RecoveryDeclaration {
        self.sign(
            *compromised_root.as_bytes(),
            recovery_epoch,
            *replacement_root.as_bytes(),
        )
    }

    /// Persist the private authority in the recovery-kit artifact.  No service
    /// constructor accepts these bytes, so they cannot be uploaded through the
    /// recovery protocol state surface.
    pub fn save(&self, path: &Path) -> Result<(), RecoveryError> {
        let mut bytes = Zeroizing::new(Vec::with_capacity(
            RECOVERY_KIT_DOMAIN.len() + ed25519::PUBLIC_KEY_SIZE * 2 + ed25519::SECRET_KEY_SIZE,
        ));
        bytes.extend_from_slice(RECOVERY_KIT_DOMAIN);
        bytes.extend_from_slice(&self.initial_root);
        bytes.extend_from_slice(self.recovery_public.as_bytes());
        bytes.extend_from_slice(self.recovery_secret.as_bytes());
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(RecoveryError::RecoveryKitIo)?;
        }
        fs::write(path, bytes.as_slice()).map_err(RecoveryError::RecoveryKitIo)
    }

    fn sign(
        &self,
        compromised_root: [u8; ed25519::PUBLIC_KEY_SIZE],
        recovery_epoch: u64,
        replacement_root: [u8; ed25519::PUBLIC_KEY_SIZE],
    ) -> RecoveryDeclaration {
        let canonical =
            canonical_recovery_state(compromised_root, recovery_epoch, replacement_root);
        RecoveryDeclaration {
            compromised_root,
            recovery_epoch,
            replacement_root,
            recovery_signature: *ed25519::sign(&self.recovery_secret, &canonical).as_bytes(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecoveryDeclaration {
    pub compromised_root: [u8; ed25519::PUBLIC_KEY_SIZE],
    pub recovery_epoch: u64,
    pub replacement_root: [u8; ed25519::PUBLIC_KEY_SIZE],
    pub recovery_signature: [u8; ed25519::SIGNATURE_SIZE],
}

impl RecoveryDeclaration {
    /// Model the exact adversarial case where the compromised active root tries
    /// to recover itself without the independent authority.  Verification must
    /// reject this signature even though it is a valid account-root signature.
    pub fn signed_by_active_root_only(
        root: &AccountRootKey,
        recovery_epoch: u64,
        replacement_root: ed25519::PublicKey,
    ) -> Self {
        let compromised_root = *root.public_key().as_bytes();
        let replacement_root = *replacement_root.as_bytes();
        let canonical =
            canonical_recovery_state(compromised_root, recovery_epoch, replacement_root);
        Self {
            compromised_root,
            recovery_epoch,
            replacement_root,
            recovery_signature: root.sign_account_authority_payload(&canonical),
        }
    }

    pub fn without_independent_signature(mut self) -> Self {
        self.recovery_signature = [0; ed25519::SIGNATURE_SIZE];
        self
    }
}

/// Complete service-side account recovery state.  Its two fields are both
/// public protocol data; there is no recovery-secret field or setter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecoveryServiceState {
    pinned_recovery_authority: [u8; ed25519::PUBLIC_KEY_SIZE],
    current_signed_state: RecoveryDeclaration,
}

impl RecoveryServiceState {
    pub fn new(
        pinned_recovery_authority: [u8; ed25519::PUBLIC_KEY_SIZE],
        genesis: RecoveryDeclaration,
    ) -> Result<Self, RecoveryError> {
        verify_recovery_signature(pinned_recovery_authority, &genesis)?;
        if genesis.recovery_epoch != 0 || genesis.compromised_root != genesis.replacement_root {
            return Err(RecoveryError::InvalidGenesis);
        }
        Ok(Self {
            pinned_recovery_authority,
            current_signed_state: genesis,
        })
    }

    pub fn accept_recovery(
        &mut self,
        declaration: RecoveryDeclaration,
    ) -> Result<(), RecoveryError> {
        validate_next_recovery(
            self.pinned_recovery_authority,
            &self.current_signed_state,
            &declaration,
        )?;
        self.current_signed_state = declaration;
        Ok(())
    }

    pub fn pinned_recovery_authority(&self) -> [u8; ed25519::PUBLIC_KEY_SIZE] {
        self.pinned_recovery_authority
    }

    pub fn current_signed_state(&self) -> &RecoveryDeclaration {
        &self.current_signed_state
    }

    pub fn active_root(&self) -> [u8; ed25519::PUBLIC_KEY_SIZE] {
        self.current_signed_state.replacement_root
    }

    pub fn recovery_epoch(&self) -> u64 {
        self.current_signed_state.recovery_epoch
    }

    /// Canonical service persistence: pinned public authority plus current
    /// independently signed state, and nothing else.
    pub fn public_state_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(
            RECOVERY_STATE_DOMAIN.len()
                + ed25519::PUBLIC_KEY_SIZE * 3
                + 8
                + ed25519::SIGNATURE_SIZE,
        );
        out.extend_from_slice(RECOVERY_STATE_DOMAIN);
        out.extend_from_slice(&self.pinned_recovery_authority);
        out.extend_from_slice(&self.current_signed_state.compromised_root);
        out.extend_from_slice(&self.current_signed_state.recovery_epoch.to_be_bytes());
        out.extend_from_slice(&self.current_signed_state.replacement_root);
        out.extend_from_slice(&self.current_signed_state.recovery_signature);
        out
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignedRoster {
    pub root_public: [u8; ed25519::PUBLIC_KEY_SIZE],
    pub revision: u64,
    pub members: Vec<String>,
    pub signature: [u8; ed25519::SIGNATURE_SIZE],
}

impl SignedRoster {
    pub fn sign(root: &AccountRootKey, revision: u64, members: Vec<String>) -> Self {
        let root_public = *root.public_key().as_bytes();
        let canonical = canonical_roster(root_public, revision, &members);
        Self {
            root_public,
            revision,
            members,
            signature: root.sign_account_authority_payload(&canonical),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct StoredRoster {
    revision: u64,
}

/// Recovery-sensitive local state for one OSL installation.
pub struct LocalOslInstance {
    instance_label: String,
    pinned_recovery_authority: [u8; ed25519::PUBLIC_KEY_SIZE],
    current_signed_state: RecoveryDeclaration,
    revoked_roots: Vec<[u8; ed25519::PUBLIC_KEY_SIZE]>,
    device_list: Option<StoredDeviceList>,
    roster: Option<StoredRoster>,
    safety_number: String,
    safety_number_verified: bool,
}

impl LocalOslInstance {
    pub fn new(
        instance_label: impl Into<String>,
        service: &RecoveryServiceState,
        device_list: SignedDeviceList,
        roster: SignedRoster,
    ) -> Result<Self, RecoveryError> {
        let instance_label = instance_label.into();
        if device_list.root_public != service.active_root() {
            return Err(RecoveryError::DeviceListWrongRoot);
        }
        let stored_device_list = StoredDeviceList::accept_next(None, device_list)
            .map_err(RecoveryError::InvalidDeviceList)?;
        verify_roster(&roster)?;
        if roster.root_public != service.active_root() {
            return Err(RecoveryError::RosterWrongRoot);
        }
        Ok(Self {
            safety_number: safety_number(&instance_label, service.active_root()),
            instance_label,
            pinned_recovery_authority: service.pinned_recovery_authority(),
            current_signed_state: service.current_signed_state().clone(),
            revoked_roots: Vec::new(),
            device_list: Some(stored_device_list),
            roster: Some(StoredRoster {
                revision: roster.revision,
            }),
            safety_number_verified: true,
        })
    }

    pub fn accept_recovery(
        &mut self,
        declaration: &RecoveryDeclaration,
    ) -> Result<(), RecoveryError> {
        validate_next_recovery(
            self.pinned_recovery_authority,
            &self.current_signed_state,
            declaration,
        )?;
        self.revoked_roots.push(declaration.compromised_root);
        self.current_signed_state = declaration.clone();
        self.device_list = None;
        self.roster = None;
        self.safety_number = safety_number(&self.instance_label, declaration.replacement_root);
        self.safety_number_verified = false;
        Ok(())
    }

    pub fn accept_device_list(&mut self, list: SignedDeviceList) -> Result<(), RecoveryError> {
        if self.revoked_roots.contains(&list.root_public) {
            return Err(RecoveryError::DeviceListFromCompromisedRoot);
        }
        if list.root_public != self.current_signed_state.replacement_root {
            return Err(RecoveryError::DeviceListWrongRoot);
        }
        verify_signed_device_list(&list).map_err(RecoveryError::InvalidDeviceList)?;
        let stored = StoredDeviceList::accept_next(self.device_list.as_ref(), list)
            .map_err(RecoveryError::InvalidDeviceList)?;
        self.device_list = Some(stored);
        Ok(())
    }

    pub fn accept_roster(&mut self, roster: SignedRoster) -> Result<(), RecoveryError> {
        if self.revoked_roots.contains(&roster.root_public) {
            return Err(RecoveryError::RosterFromCompromisedRoot);
        }
        if roster.root_public != self.current_signed_state.replacement_root {
            return Err(RecoveryError::RosterWrongRoot);
        }
        verify_roster(&roster)?;
        if self
            .roster
            .as_ref()
            .is_some_and(|stored| roster.revision <= stored.revision)
        {
            return Err(RecoveryError::RosterRevisionMustGoUp);
        }
        self.roster = Some(StoredRoster {
            revision: roster.revision,
        });
        Ok(())
    }

    pub fn active_root(&self) -> [u8; ed25519::PUBLIC_KEY_SIZE] {
        self.current_signed_state.replacement_root
    }

    pub fn recovery_epoch(&self) -> u64 {
        self.current_signed_state.recovery_epoch
    }

    pub fn safety_number(&self) -> &str {
        &self.safety_number
    }

    pub fn safety_number_verified(&self) -> bool {
        self.safety_number_verified
    }

    /// Re-verification is an explicit local act after recovery. Publishing or
    /// accepting new account state never calls this method implicitly.
    pub fn reverify_safety_number(&mut self, compared: &str) -> Result<(), RecoveryError> {
        if compared != self.safety_number {
            return Err(RecoveryError::SafetyNumberMismatch);
        }
        self.safety_number_verified = true;
        Ok(())
    }

    pub fn has_device_list(&self) -> bool {
        self.device_list.is_some()
    }

    pub fn device_count(&self) -> usize {
        self.device_list
            .as_ref()
            .map_or(0, StoredDeviceList::device_count)
    }

    pub fn has_roster(&self) -> bool {
        self.roster.is_some()
    }
}

#[derive(Debug)]
pub enum RecoveryError {
    InvalidRecoverySignature,
    InvalidGenesis,
    CompromisedRootNotActive,
    RecoveryEpochMustGoUp { current: u64, proposed: u64 },
    ReplacementMatchesCompromisedRoot,
    DeviceListFromCompromisedRoot,
    DeviceListWrongRoot,
    RosterFromCompromisedRoot,
    RosterWrongRoot,
    InvalidRosterSignature,
    RosterRevisionMustGoUp,
    SafetyNumberMismatch,
    InvalidDeviceList(DeviceListError),
    RecoveryKitIo(std::io::Error),
}

impl fmt::Display for RecoveryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRecoverySignature => f.write_str("independent recovery signature invalid"),
            Self::InvalidGenesis => f.write_str("recovery genesis state invalid"),
            Self::CompromisedRootNotActive => {
                f.write_str("recovery declaration does not name the active root")
            }
            Self::RecoveryEpochMustGoUp { current, proposed } => write!(
                f,
                "recovery epoch must go up (current {current}, proposed {proposed})"
            ),
            Self::ReplacementMatchesCompromisedRoot => {
                f.write_str("replacement root must differ from compromised root")
            }
            Self::DeviceListFromCompromisedRoot => {
                f.write_str("device list signature from compromised root rejected")
            }
            Self::DeviceListWrongRoot => f.write_str("device list is not from active root"),
            Self::RosterFromCompromisedRoot => {
                f.write_str("roster signature from compromised root rejected")
            }
            Self::RosterWrongRoot => f.write_str("roster is not from active root"),
            Self::InvalidRosterSignature => f.write_str("roster signature invalid"),
            Self::RosterRevisionMustGoUp => f.write_str("roster revision must go up"),
            Self::SafetyNumberMismatch => f.write_str("safety number re-verification mismatch"),
            Self::InvalidDeviceList(error) => write!(f, "invalid device list: {error}"),
            Self::RecoveryKitIo(error) => write!(f, "recovery kit I/O failed: {error}"),
        }
    }
}

impl std::error::Error for RecoveryError {}

fn validate_next_recovery(
    pinned_recovery_authority: [u8; ed25519::PUBLIC_KEY_SIZE],
    current: &RecoveryDeclaration,
    next: &RecoveryDeclaration,
) -> Result<(), RecoveryError> {
    verify_recovery_signature(pinned_recovery_authority, next)?;
    if next.compromised_root != current.replacement_root {
        return Err(RecoveryError::CompromisedRootNotActive);
    }
    if next.recovery_epoch <= current.recovery_epoch {
        return Err(RecoveryError::RecoveryEpochMustGoUp {
            current: current.recovery_epoch,
            proposed: next.recovery_epoch,
        });
    }
    if next.replacement_root == next.compromised_root {
        return Err(RecoveryError::ReplacementMatchesCompromisedRoot);
    }
    Ok(())
}

fn verify_recovery_signature(
    pinned_recovery_authority: [u8; ed25519::PUBLIC_KEY_SIZE],
    declaration: &RecoveryDeclaration,
) -> Result<(), RecoveryError> {
    let valid = ed25519::verify(
        &ed25519::PublicKey::from_bytes(pinned_recovery_authority),
        &canonical_recovery_state(
            declaration.compromised_root,
            declaration.recovery_epoch,
            declaration.replacement_root,
        ),
        &ed25519::Signature::from_bytes(declaration.recovery_signature),
    )
    .unwrap_or(false);
    if valid {
        Ok(())
    } else {
        Err(RecoveryError::InvalidRecoverySignature)
    }
}

fn verify_roster(roster: &SignedRoster) -> Result<(), RecoveryError> {
    let valid = ed25519::verify(
        &ed25519::PublicKey::from_bytes(roster.root_public),
        &canonical_roster(roster.root_public, roster.revision, &roster.members),
        &ed25519::Signature::from_bytes(roster.signature),
    )
    .unwrap_or(false);
    if valid {
        Ok(())
    } else {
        Err(RecoveryError::InvalidRosterSignature)
    }
}

fn canonical_recovery_state(
    compromised_root: [u8; ed25519::PUBLIC_KEY_SIZE],
    recovery_epoch: u64,
    replacement_root: [u8; ed25519::PUBLIC_KEY_SIZE],
) -> Vec<u8> {
    let mut out = Vec::with_capacity(RECOVERY_STATE_DOMAIN.len() + 32 + 8 + 32);
    out.extend_from_slice(RECOVERY_STATE_DOMAIN);
    out.extend_from_slice(&compromised_root);
    out.extend_from_slice(&recovery_epoch.to_be_bytes());
    out.extend_from_slice(&replacement_root);
    out
}

fn canonical_roster(
    root_public: [u8; ed25519::PUBLIC_KEY_SIZE],
    revision: u64,
    members: &[String],
) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(ACCOUNT_ROSTER_DOMAIN);
    out.extend_from_slice(&root_public);
    out.extend_from_slice(&revision.to_be_bytes());
    out.extend_from_slice(&(members.len() as u32).to_be_bytes());
    for member in members {
        out.extend_from_slice(&(member.len() as u32).to_be_bytes());
        out.extend_from_slice(member.as_bytes());
    }
    out
}

fn safety_number(instance_label: &str, active_root: [u8; ed25519::PUBLIC_KEY_SIZE]) -> String {
    let mut hash = Sha256::new();
    hash.update(SAFETY_NUMBER_DOMAIN);
    hash.update((instance_label.len() as u32).to_be_bytes());
    hash.update(instance_label.as_bytes());
    hash.update(active_root);
    let digest = hash.finalize();
    let mut digits = String::with_capacity(35);
    for block in 0..6 {
        let offset = block * 4;
        let value = u32::from_be_bytes([
            digest[offset],
            digest[offset + 1],
            digest[offset + 2],
            digest[offset + 3],
        ]) % 100_000;
        if block > 0 {
            digits.push(' ');
        }
        use std::fmt::Write as _;
        let _ = write!(digits, "{value:05}");
    }
    digits
}
