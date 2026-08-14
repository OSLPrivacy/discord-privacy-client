//! Durable, serialized membership authority for group chats and Enclaves.
//!
//! Every shipping removal-shaped entry point is represented by [`WriterRoute`]
//! and converges on one transaction.  In particular, no route can persist an
//! ownerless Enclave, and the check happens before state, pending work, or the
//! recoverable file is changed.

use crypto::ed25519;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub use crate::membership_size_rules::{
    GROUP_CHAT_FULL_ERROR as GROUP_CHAT_FULL_MESSAGE, GROUP_CHAT_MAX_PEOPLE as GROUP_CHAT_TOTAL_CAP,
};
pub const ENCLAVE_REMOVAL_PROGRESS_N: usize = 500;
pub const LAST_ENCLAVE_OWNER_MESSAGE: &str = "An enclave's last owner must remain";
pub const ENCLAVE_REMOVAL_WARNING: &str = "Removal takes time and is not immediate.";

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PlaceKind {
    GroupChat,
    Enclave,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WriterRoute {
    Leave,
    OwnerRemoval,
    SelfRemoval,
    BatchRemoval,
    DirectMembershipWriter,
}

/// Complete inventory of production membership-removal writers. New shipping
/// writers must be added here and call [`MembershipService::remove`].
pub const SHIPPING_WRITER_ROUTES: [WriterRoute; 5] = [
    WriterRoute::Leave,
    WriterRoute::OwnerRemoval,
    WriterRoute::SelfRemoval,
    WriterRoute::BatchRemoval,
    WriterRoute::DirectMembershipWriter,
];

pub fn shipping_writer_inventory() -> &'static [WriterRoute] {
    &SHIPPING_WRITER_ROUTES
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SignedRoster {
    format_version: u8,
    pub place_id: String,
    pub kind: PlaceKind,
    pub members: BTreeMap<String, [u8; ed25519::PUBLIC_KEY_SIZE]>,
    pub owners: BTreeSet<String>,
    pub roster_version: u64,
    pub channel_epoch: u64,
    pub channel_key_hash: [u8; 32],
    /// Durable delivery planning generation. It advances in the same commit as
    /// membership and key authority, never during a refused admission.
    pub delivery_state_version: u64,
    /// One admission allowance reservation for every current member. Keeping
    /// this in the signed roster makes person-21 refusal atomic with allowance
    /// accounting instead of relying on a later compensating write.
    pub allowance_reservations: u64,
    pub pending_operations: Vec<String>,
    consumed_request_ids: BTreeSet<String>,
    signer_public: [u8; ed25519::PUBLIC_KEY_SIZE],
    signature: Vec<u8>,
}

impl SignedRoster {
    pub fn signer_public(&self) -> &[u8; ed25519::PUBLIC_KEY_SIZE] {
        &self.signer_public
    }

    pub fn signature(&self) -> &[u8] {
        &self.signature
    }

    pub fn verify_signature(&self) -> Result<(), MembershipError> {
        if self.signature.len() != ed25519::SIGNATURE_SIZE {
            return Err(MembershipError::InvalidDurableRoster);
        }
        let mut bytes = [0_u8; ed25519::SIGNATURE_SIZE];
        bytes.copy_from_slice(&self.signature);
        let valid = ed25519::verify(
            &ed25519::PublicKey::from_bytes(self.signer_public),
            &self.signing_bytes()?,
            &ed25519::Signature::from_bytes(bytes),
        )
        .map_err(|_| MembershipError::InvalidDurableRoster)?;
        if valid {
            Ok(())
        } else {
            Err(MembershipError::InvalidDurableRoster)
        }
    }

    fn signing_bytes(&self) -> Result<Vec<u8>, MembershipError> {
        let mut unsigned = self.clone();
        unsigned.signature.clear();
        serde_json::to_vec(&unsigned).map_err(|e| MembershipError::Persistence(e.to_string()))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JoinRequest {
    pub place_id: String,
    pub member_id: String,
    pub member_public: [u8; ed25519::PUBLIC_KEY_SIZE],
    pub as_owner: bool,
    pub request_id: String,
    signature: [u8; ed25519::SIGNATURE_SIZE],
}

impl JoinRequest {
    pub fn signed(
        place_id: impl Into<String>,
        member_id: impl Into<String>,
        as_owner: bool,
        request_id: impl Into<String>,
        secret: &ed25519::SecretKey,
    ) -> Self {
        let mut request = Self {
            place_id: place_id.into(),
            member_id: member_id.into(),
            member_public: *ed25519::derive_public(secret).as_bytes(),
            as_owner,
            request_id: request_id.into(),
            signature: [0; ed25519::SIGNATURE_SIZE],
        };
        request.signature = *ed25519::sign(secret, &request.signing_bytes()).as_bytes();
        request
    }

    fn signing_bytes(&self) -> Vec<u8> {
        request_bytes(
            b"join",
            &self.place_id,
            &self.member_id,
            self.as_owner,
            &self.request_id,
            &[],
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RemovalRequest {
    pub place_id: String,
    pub actor_id: String,
    pub targets: Vec<String>,
    pub request_id: String,
    pub route: WriterRoute,
    signature: [u8; ed25519::SIGNATURE_SIZE],
}

impl RemovalRequest {
    pub fn signed(
        place_id: impl Into<String>,
        actor_id: impl Into<String>,
        targets: Vec<String>,
        request_id: impl Into<String>,
        route: WriterRoute,
        secret: &ed25519::SecretKey,
    ) -> Self {
        let mut request = Self {
            place_id: place_id.into(),
            actor_id: actor_id.into(),
            targets,
            request_id: request_id.into(),
            route,
            signature: [0; ed25519::SIGNATURE_SIZE],
        };
        request.signature = *ed25519::sign(secret, &request.signing_bytes()).as_bytes();
        request
    }

    fn signing_bytes(&self) -> Vec<u8> {
        let route = serde_json::to_vec(&self.route).unwrap_or_default();
        request_bytes(
            b"remove",
            &self.place_id,
            &self.actor_id,
            false,
            &self.request_id,
            &[route, serde_json::to_vec(&self.targets).unwrap_or_default()].concat(),
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MutationReceipt {
    pub member_count: usize,
    pub roster_version: u64,
    pub channel_epoch: u64,
    pub delivery_state_version: u64,
    pub allowance_reservations: u64,
    /// Present only for removal from an Enclave whose pre-removal roster is
    /// above measured N. This is progress/warning metadata, never "full".
    pub removal_progress: Option<RemovalProgressDisclosure>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RemovalProgressDisclosure {
    pub measured_n: usize,
    pub warning: &'static str,
}

#[derive(Debug, thiserror::Error, Eq, PartialEq)]
pub enum MembershipError {
    #[error("{GROUP_CHAT_FULL_MESSAGE}")]
    GroupChatFull,
    #[error("{LAST_ENCLAVE_OWNER_MESSAGE}")]
    LastEnclaveOwner,
    #[error("membership request authentication failed")]
    Authentication,
    #[error("membership request was already consumed")]
    Replay,
    #[error("membership request names the wrong place")]
    WrongPlace,
    #[error("membership member is absent")]
    MemberAbsent,
    #[error("membership member already exists")]
    MemberExists,
    #[error("membership actor is not an owner")]
    NotOwner,
    #[error("invalid durable membership roster")]
    InvalidDurableRoster,
    #[error("membership persistence failed: {0}")]
    Persistence(String),
}

struct Inner {
    roster: SignedRoster,
    signer_secret: ed25519::SecretKey,
}

pub struct MembershipService {
    path: PathBuf,
    inner: Mutex<Inner>,
}

impl MembershipService {
    pub fn create(
        path: impl Into<PathBuf>,
        place_id: impl Into<String>,
        kind: PlaceKind,
        creator_id: impl Into<String>,
        creator_public: ed25519::PublicKey,
        signer_secret: ed25519::SecretKey,
    ) -> Result<Self, MembershipError> {
        let path = path.into();
        let place_id = place_id.into();
        let creator_id = creator_id.into();
        let mut members = BTreeMap::new();
        members.insert(creator_id.clone(), *creator_public.as_bytes());
        let mut roster = SignedRoster {
            format_version: 1,
            place_id,
            kind,
            members,
            owners: BTreeSet::from([creator_id]),
            roster_version: 1,
            channel_epoch: 1,
            channel_key_hash: [0; 32],
            delivery_state_version: 1,
            allowance_reservations: 1,
            pending_operations: Vec::new(),
            consumed_request_ids: BTreeSet::new(),
            signer_public: *ed25519::derive_public(&signer_secret).as_bytes(),
            signature: Vec::new(),
        };
        roster.channel_key_hash = next_key_hash(&roster);
        sign_roster(&mut roster, &signer_secret)?;
        validate_roster(&roster)?;
        persist(&path, &roster)?;
        Ok(Self {
            path,
            inner: Mutex::new(Inner {
                roster,
                signer_secret,
            }),
        })
    }

    pub fn reopen(
        path: impl Into<PathBuf>,
        signer_secret: ed25519::SecretKey,
    ) -> Result<Self, MembershipError> {
        let path = path.into();
        let bytes =
            std::fs::read(&path).map_err(|e| MembershipError::Persistence(e.to_string()))?;
        let roster: SignedRoster =
            serde_json::from_slice(&bytes).map_err(|_| MembershipError::InvalidDurableRoster)?;
        validate_roster(&roster)?;
        roster.verify_signature()?;
        if roster.signer_public != *ed25519::derive_public(&signer_secret).as_bytes() {
            return Err(MembershipError::Authentication);
        }
        Ok(Self {
            path,
            inner: Mutex::new(Inner {
                roster,
                signer_secret,
            }),
        })
    }

    pub fn snapshot(&self) -> SignedRoster {
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .roster
            .clone()
    }

    pub fn durable_bytes(&self) -> Result<Vec<u8>, MembershipError> {
        std::fs::read(&self.path).map_err(|e| MembershipError::Persistence(e.to_string()))
    }

    /// Serialized admission transaction. Group-chat capacity is checked while
    /// holding the same mutex as the durable commit, closing the 19+2 race.
    pub fn join(&self, request: &JoinRequest) -> Result<MutationReceipt, MembershipError> {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if request.place_id != inner.roster.place_id {
            return Err(MembershipError::WrongPlace);
        }
        authenticate(
            &ed25519::PublicKey::from_bytes(request.member_public),
            &request.signing_bytes(),
            &request.signature,
        )?;
        if inner
            .roster
            .consumed_request_ids
            .contains(&request.request_id)
        {
            return Err(MembershipError::Replay);
        }
        if inner.roster.members.contains_key(&request.member_id) {
            return Err(MembershipError::MemberExists);
        }
        let candidate_count = inner.roster.members.len().saturating_add(1);
        match inner.roster.kind {
            PlaceKind::GroupChat => {
                crate::membership_size_rules::enforce_group_chat_candidate_size(candidate_count)
                    .map_err(|_| MembershipError::GroupChatFull)?
            }
            PlaceKind::Enclave => {
                crate::membership_size_rules::enforce_enclave_candidate_size(candidate_count)
                    .expect("Enclave admission has no maximum")
            }
        }
        let mut next = inner.roster.clone();
        next.members
            .insert(request.member_id.clone(), request.member_public);
        if request.as_owner {
            next.owners.insert(request.member_id.clone());
        }
        next.consumed_request_ids.insert(request.request_id.clone());
        commit(&self.path, &mut inner, next)
    }

    /// The sole mutation gateway for every route in [`SHIPPING_WRITER_ROUTES`].
    pub fn remove(&self, request: &RemovalRequest) -> Result<MutationReceipt, MembershipError> {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if request.place_id != inner.roster.place_id {
            return Err(MembershipError::WrongPlace);
        }
        let actor_key = inner
            .roster
            .members
            .get(&request.actor_id)
            .ok_or(MembershipError::MemberAbsent)?;
        authenticate(
            &ed25519::PublicKey::from_bytes(*actor_key),
            &request.signing_bytes(),
            &request.signature,
        )?;
        if inner
            .roster
            .consumed_request_ids
            .contains(&request.request_id)
        {
            return Err(MembershipError::Replay);
        }
        let targets = normalized_targets(request)?;
        if matches!(
            request.route,
            WriterRoute::OwnerRemoval | WriterRoute::BatchRemoval
        ) && !inner.roster.owners.contains(&request.actor_id)
        {
            return Err(MembershipError::NotOwner);
        }
        if targets
            .iter()
            .any(|target| !inner.roster.members.contains_key(target))
        {
            return Err(MembershipError::MemberAbsent);
        }
        if inner.roster.kind == PlaceKind::Enclave {
            let owners_left = inner
                .roster
                .owners
                .iter()
                .filter(|owner| !targets.contains(*owner))
                .count();
            if owners_left == 0 {
                return Err(MembershipError::LastEnclaveOwner);
            }
        }
        let prior_count = inner.roster.members.len();
        let mut next = inner.roster.clone();
        for target in &targets {
            next.members.remove(target);
            next.owners.remove(target);
        }
        next.consumed_request_ids.insert(request.request_id.clone());
        let mut receipt = commit(&self.path, &mut inner, next)?;
        receipt.removal_progress = (inner.roster.kind == PlaceKind::Enclave
            && prior_count > ENCLAVE_REMOVAL_PROGRESS_N)
            .then_some(RemovalProgressDisclosure {
                measured_n: ENCLAVE_REMOVAL_PROGRESS_N,
                warning: ENCLAVE_REMOVAL_WARNING,
            });
        Ok(receipt)
    }
}

fn normalized_targets(request: &RemovalRequest) -> Result<BTreeSet<String>, MembershipError> {
    let targets: BTreeSet<String> = match request.route {
        WriterRoute::Leave | WriterRoute::SelfRemoval => BTreeSet::from([request.actor_id.clone()]),
        WriterRoute::OwnerRemoval
        | WriterRoute::BatchRemoval
        | WriterRoute::DirectMembershipWriter => request.targets.iter().cloned().collect(),
    };
    if targets.is_empty() {
        Err(MembershipError::MemberAbsent)
    } else {
        Ok(targets)
    }
}

fn authenticate(
    public: &ed25519::PublicKey,
    bytes: &[u8],
    signature: &[u8; ed25519::SIGNATURE_SIZE],
) -> Result<(), MembershipError> {
    let ok = ed25519::verify(public, bytes, &ed25519::Signature::from_bytes(*signature))
        .map_err(|_| MembershipError::Authentication)?;
    if ok {
        Ok(())
    } else {
        Err(MembershipError::Authentication)
    }
}

fn commit(
    path: &Path,
    inner: &mut Inner,
    mut next: SignedRoster,
) -> Result<MutationReceipt, MembershipError> {
    next.roster_version = next
        .roster_version
        .checked_add(1)
        .ok_or(MembershipError::InvalidDurableRoster)?;
    next.channel_epoch = next
        .channel_epoch
        .checked_add(1)
        .ok_or(MembershipError::InvalidDurableRoster)?;
    next.delivery_state_version = next.roster_version;
    next.allowance_reservations =
        u64::try_from(next.members.len()).map_err(|_| MembershipError::InvalidDurableRoster)?;
    next.channel_key_hash = next_key_hash(&next);
    sign_roster(&mut next, &inner.signer_secret)?;
    validate_roster(&next)?;
    persist(path, &next)?;
    let receipt = MutationReceipt {
        member_count: next.members.len(),
        roster_version: next.roster_version,
        channel_epoch: next.channel_epoch,
        delivery_state_version: next.delivery_state_version,
        allowance_reservations: next.allowance_reservations,
        removal_progress: None,
    };
    inner.roster = next;
    Ok(receipt)
}

fn sign_roster(
    roster: &mut SignedRoster,
    secret: &ed25519::SecretKey,
) -> Result<(), MembershipError> {
    roster.signature.clear();
    roster.signature = ed25519::sign(secret, &roster.signing_bytes()?)
        .as_bytes()
        .to_vec();
    Ok(())
}

fn validate_roster(roster: &SignedRoster) -> Result<(), MembershipError> {
    if roster.format_version != 1
        || roster.place_id.is_empty()
        || roster.members.is_empty()
        || !roster
            .owners
            .iter()
            .all(|owner| roster.members.contains_key(owner))
        || (roster.kind == PlaceKind::Enclave && roster.owners.is_empty())
        || roster.delivery_state_version != roster.roster_version
        || roster.allowance_reservations != roster.members.len() as u64
    {
        return Err(MembershipError::InvalidDurableRoster);
    }
    match roster.kind {
        PlaceKind::GroupChat => {
            crate::membership_size_rules::enforce_group_chat_candidate_size(roster.members.len())
                .map_err(|_| MembershipError::InvalidDurableRoster)?;
        }
        PlaceKind::Enclave => {
            crate::membership_size_rules::enforce_enclave_candidate_size(roster.members.len())
                .expect("Enclave admission has no maximum");
        }
    }
    Ok(())
}

fn persist(path: &Path, roster: &SignedRoster) -> Result<(), MembershipError> {
    let bytes =
        serde_json::to_vec(roster).map_err(|e| MembershipError::Persistence(e.to_string()))?;
    crate::recoverable_file::write_recoverable(path, &bytes)
        .map_err(|e| MembershipError::Persistence(e.to_string()))
}

fn next_key_hash(roster: &SignedRoster) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(b"OSL/membership-channel-key-hash/v1");
    h.update(roster.place_id.as_bytes());
    h.update(roster.roster_version.to_be_bytes());
    h.update(roster.channel_epoch.to_be_bytes());
    h.update(roster.channel_key_hash);
    for id in roster.members.keys() {
        h.update((id.len() as u64).to_be_bytes());
        h.update(id.as_bytes());
    }
    h.finalize().into()
}

fn request_bytes(
    domain: &[u8],
    place: &str,
    actor: &str,
    flag: bool,
    id: &str,
    tail: &[u8],
) -> Vec<u8> {
    let mut out = Vec::new();
    for part in [
        domain,
        place.as_bytes(),
        actor.as_bytes(),
        &[flag as u8],
        id.as_bytes(),
        tail,
    ] {
        out.extend_from_slice(&(part.len() as u64).to_be_bytes());
        out.extend_from_slice(part);
    }
    out
}
