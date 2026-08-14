//! Customisable Enclave categories, channels, modes and permission overrides.
//!
//! An Enclave's shape used to be a rendering convention: a category and channel
//! roster fixed by the client that drew it, with access decided by a switch on
//! the channel's kind. Nothing in this crate now holds such a roster, and this
//! module is why: it is an append-only log of **signed** structural
//! operations. Every category and channel is created, renamed, reordered,
//! moved and deleted through an operation that is authenticated to its author
//! and replayed identically by every client, so structure is authoritative
//! rather than a rendering convention.
//!
//! ## Convergence
//!
//! Operations carry a Lamport counter and are identified by the SHA-256 digest
//! of their signing bytes. Replicas fold the union of the operations they hold
//! in the total order `(lamport, op_id)`. Two replicas holding the same set of
//! operations therefore produce byte-identical state, whatever order the
//! operations arrived in. Concurrent edits to the same field resolve
//! last-writer-wins under that same total order, so the winner is a property of
//! the operation set and not of the network.
//!
//! ## Authority
//!
//! An operation is *admitted* only if its signature verifies and its author
//! held the required authority in the state produced by the operations that
//! precede it in the total order. Admission is therefore also deterministic.
//! Refused operations are retained in [`EnclaveLayout::rejections`] so a client
//! can say honestly what it declined and why, instead of silently dropping it.
//!
//! ## Access
//!
//! Exactly one resolver ([`EnclaveLayout::resolve`]) answers read, post and
//! manage for every member and channel. `STEWARDS` is a *display* mode: it is
//! backed by the set of role **ids** that carry authority, never by role names,
//! so renaming a role cannot change who a stewards-only channel admits.
//!
//! ## Limits
//!
//! There is no constant ceiling on categories or channels. The only limit is
//! the serialized size of the signed log, and [`measure_layout_limit`] reports
//! it by actually building a log until the budget is spent — a measured number
//! for the device that ran it, not a documented promise.

use crate::space_roster::SpaceMemberId;
use crypto::ed25519::{self, PublicKey, SecretKey, Signature};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io,
    path::{Path, PathBuf},
};

/// Domain separator for the signing bytes of a structural operation.
pub const LAYOUT_OP_DOMAIN: &[u8] = b"osl.enclave.layout.op.v1";

/// Opaque identity of one Enclave whose layout this log describes.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct EnclaveLayoutId([u8; Self::LENGTH]);

impl EnclaveLayoutId {
    pub const LENGTH: usize = 16;

    pub const fn from_bytes(bytes: [u8; Self::LENGTH]) -> Self {
        Self(bytes)
    }

    pub const fn as_bytes(&self) -> &[u8; Self::LENGTH] {
        &self.0
    }
}

macro_rules! opaque_id {
    ($name:ident, $doc:literal) => {
        #[doc = $doc]
        #[derive(
            Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
        )]
        pub struct $name([u8; Self::LENGTH]);

        impl $name {
            pub const LENGTH: usize = 16;

            pub const fn from_bytes(bytes: [u8; Self::LENGTH]) -> Self {
                Self(bytes)
            }

            pub const fn as_bytes(&self) -> &[u8; Self::LENGTH] {
                &self.0
            }

            /// Short stable label for local storage keys and refusal text.
            pub fn hex(&self) -> String {
                hex::encode(self.0)
            }
        }
    };
}

opaque_id!(
    CategoryId,
    "Opaque identity of one category. Never derived from its name, so renaming\na category cannot split or merge it."
);
opaque_id!(
    ChannelId,
    "Opaque identity of one channel. Never derived from its name, so renaming a\nchannel cannot create a second key domain."
);
opaque_id!(
    RoleId,
    "Opaque identity of one custom role. Authority is carried by this id, never\nby the role's display name."
);
opaque_id!(MessageId, "Opaque identity of one stored message.");

/// The shipped access modes of a channel.
///
/// `Stewards` is a display mode. It denies everyone who does not hold at least
/// one authority-bearing role **id**; it never consults a role's name.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ChannelMode {
    Open,
    ReadOnly,
    Stewards,
}

impl ChannelMode {
    /// Every mode this release ships. Adding a mode must extend this array so
    /// the acceptance matrix covers it.
    pub const SHIPPED: [Self; 3] = [Self::Open, Self::ReadOnly, Self::Stewards];

    /// The label the product shows for this mode.
    pub const fn display_label(self) -> &'static str {
        match self {
            Self::Open => "OPEN",
            Self::ReadOnly => "READ ONLY",
            Self::Stewards => "STEWARDS",
        }
    }

    /// The wire token shared with the renderer.
    pub const fn token(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::ReadOnly => "read-only",
            Self::Stewards => "stewards",
        }
    }

    /// The mode's decision before any override is consulted.
    ///
    /// `authority` is whether the member holds an authority-bearing role id.
    const fn base_allows(self, action: ChannelAction, authority: bool) -> bool {
        match (self, action) {
            (Self::Open, ChannelAction::Read | ChannelAction::Post) => true,
            (Self::ReadOnly, ChannelAction::Read) => true,
            (Self::Stewards, _) => authority,
            (_, ChannelAction::Manage) => authority,
            (Self::ReadOnly, ChannelAction::Post) => authority,
        }
    }
}

/// The three actions the single resolver answers.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ChannelAction {
    Read,
    Post,
    Manage,
}

impl ChannelAction {
    pub const ALL: [Self; 3] = [Self::Read, Self::Post, Self::Manage];

    pub const fn token(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Post => "post",
            Self::Manage => "manage",
        }
    }
}

/// One explicit per-role override bit. Absent means "inherit".
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OverrideBit {
    Allow,
    Deny,
}

/// A per-role override on one channel or category.
///
/// Every field is independently `None` (inherit), `Allow` or `Deny`, so the
/// permission grid has a real tri-state cell per role and action.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct RoleOverride {
    #[serde(default)]
    pub read: Option<OverrideBit>,
    #[serde(default)]
    pub post: Option<OverrideBit>,
    #[serde(default)]
    pub manage: Option<OverrideBit>,
}

impl RoleOverride {
    /// An override that says nothing; every action inherits.
    pub const INHERIT: Self = Self {
        read: None,
        post: None,
        manage: None,
    };

    pub const fn with(mut self, action: ChannelAction, bit: OverrideBit) -> Self {
        match action {
            ChannelAction::Read => self.read = Some(bit),
            ChannelAction::Post => self.post = Some(bit),
            ChannelAction::Manage => self.manage = Some(bit),
        }
        self
    }

    pub const fn bit(&self, action: ChannelAction) -> Option<OverrideBit> {
        match action {
            ChannelAction::Read => self.read,
            ChannelAction::Post => self.post,
            ChannelAction::Manage => self.manage,
        }
    }

    pub const fn is_inherit(&self) -> bool {
        self.read.is_none() && self.post.is_none() && self.manage.is_none()
    }
}

/// Where a resolved decision came from, so the product can show *inherited*
/// access apart from access a role override changed.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "source")]
pub enum AccessSource {
    /// The actor is not a member of this Enclave at all.
    NotAMember,
    /// An explicit override on this channel for one of the actor's roles.
    ChannelOverride { role: RoleId },
    /// An explicit override on the channel's category.
    CategoryOverride { role: RoleId },
    /// No override applied; the channel's shipped mode decided.
    InheritedMode { mode: ChannelMode },
    /// No override applied; enclave-level authority decided (category scope).
    InheritedEnclaveAuthority,
}

impl AccessSource {
    /// The word the product shows next to the cell.
    pub const fn label(&self) -> &'static str {
        match self {
            Self::NotAMember => "not a member",
            Self::ChannelOverride { .. } | Self::CategoryOverride { .. } => "overridden",
            Self::InheritedMode { .. } | Self::InheritedEnclaveAuthority => "inherited",
        }
    }

    pub const fn is_overridden(&self) -> bool {
        matches!(
            self,
            Self::ChannelOverride { .. } | Self::CategoryOverride { .. }
        )
    }

    pub const fn is_inherited(&self) -> bool {
        matches!(
            self,
            Self::InheritedMode { .. } | Self::InheritedEnclaveAuthority
        )
    }
}

/// One resolved cell: the answer and the reason for it.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AccessDecision {
    pub allowed: bool,
    pub source: AccessSource,
}

impl AccessDecision {
    const fn new(allowed: bool, source: AccessSource) -> Self {
        Self { allowed, source }
    }
}

/// A custom role. Authority travels with [`CustomRole::id`]; the name is a
/// label the product renders and may change at any time.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CustomRole {
    pub id: RoleId,
    pub name: String,
    /// Whether holding this role is authority in this Enclave. This is what
    /// `STEWARDS` is backed by.
    pub authority: bool,
}

/// A category as replayed from the signed log.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Category {
    pub id: CategoryId,
    pub name: String,
    pub position: i64,
    pub overrides: BTreeMap<RoleId, RoleOverride>,
}

/// A channel as replayed from the signed log.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Channel {
    pub id: ChannelId,
    pub category: CategoryId,
    pub name: String,
    pub mode: ChannelMode,
    pub position: i64,
    pub overrides: BTreeMap<RoleId, RoleOverride>,
}

/// One stored message. Present so deletion can be shown never to orphan it.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct StoredMessage {
    pub id: MessageId,
    pub channel: ChannelId,
    pub author: SpaceMemberId,
    pub body: String,
    /// Set when the message was relocated out of a deleted channel.
    pub moved_from: Option<ChannelId>,
}

/// What must happen to a nonempty channel's content before it can be deleted.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "disposition")]
pub enum ChannelDisposition {
    /// Relocate every message to a surviving channel the author may manage.
    MoveContentTo { channel: ChannelId },
    /// Destroy every message, on the record, with a receipt.
    BurnContent,
}

/// One structural or content operation over the layout.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "op")]
pub enum LayoutOp {
    AdmitMember {
        key: [u8; 32],
    },
    DefineRole {
        role: RoleId,
        name: String,
        authority: bool,
    },
    RenameRole {
        role: RoleId,
        name: String,
    },
    GrantRole {
        member: SpaceMemberId,
        role: RoleId,
    },
    CreateCategory {
        category: CategoryId,
        name: String,
        position: i64,
    },
    RenameCategory {
        category: CategoryId,
        name: String,
    },
    ReorderCategory {
        category: CategoryId,
        position: i64,
    },
    DeleteCategory {
        category: CategoryId,
        channels_to: Option<CategoryId>,
    },
    SetCategoryOverride {
        category: CategoryId,
        role: RoleId,
        over: RoleOverride,
    },
    CreateChannel {
        channel: ChannelId,
        category: CategoryId,
        name: String,
        mode: ChannelMode,
        position: i64,
    },
    RenameChannel {
        channel: ChannelId,
        name: String,
    },
    ReorderChannel {
        channel: ChannelId,
        position: i64,
    },
    MoveChannel {
        channel: ChannelId,
        category: CategoryId,
        position: i64,
    },
    SetChannelMode {
        channel: ChannelId,
        mode: ChannelMode,
    },
    SetChannelOverride {
        channel: ChannelId,
        role: RoleId,
        over: RoleOverride,
    },
    ClearChannelOverride {
        channel: ChannelId,
        role: RoleId,
    },
    DeleteChannel {
        channel: ChannelId,
        disposition: Option<ChannelDisposition>,
    },
    PostMessage {
        channel: ChannelId,
        message: MessageId,
        body: String,
    },
}

impl LayoutOp {
    /// The structural edit kind, used by the acceptance matrix to prove every
    /// kind of edit is exercised. Content operations return `None`.
    pub const fn edit_kind(&self) -> Option<EditKind> {
        match self {
            Self::CreateCategory { .. } | Self::CreateChannel { .. } => Some(EditKind::Create),
            Self::RenameCategory { .. } | Self::RenameChannel { .. } => Some(EditKind::Rename),
            Self::ReorderCategory { .. } | Self::ReorderChannel { .. } => Some(EditKind::Reorder),
            Self::MoveChannel { .. } => Some(EditKind::Move),
            Self::DeleteCategory { .. } | Self::DeleteChannel { .. } => Some(EditKind::Delete),
            _ => None,
        }
    }
}

/// The five structural edits the product ships.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum EditKind {
    Create,
    Rename,
    Reorder,
    Move,
    Delete,
}

impl EditKind {
    pub const ALL: [Self; 5] = [
        Self::Create,
        Self::Rename,
        Self::Reorder,
        Self::Move,
        Self::Delete,
    ];

    pub const fn token(self) -> &'static str {
        match self {
            Self::Create => "create",
            Self::Rename => "rename",
            Self::Reorder => "reorder",
            Self::Move => "move",
            Self::Delete => "delete",
        }
    }
}

/// An operation with its author, counter and signature.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SignedLayoutOp {
    pub author: [u8; 32],
    pub lamport: u64,
    pub op: LayoutOp,
    /// Ed25519 signature over [`SignedLayoutOp::signing_bytes`]; always 64 bytes.
    pub signature: Vec<u8>,
}

impl SignedLayoutOp {
    /// The exact bytes an author signs. The enclave id is bound in so an
    /// operation cannot be replayed into a different Enclave.
    pub fn signing_bytes(
        enclave: EnclaveLayoutId,
        author: &[u8; 32],
        lamport: u64,
        op: &LayoutOp,
    ) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(160);
        bytes.extend_from_slice(LAYOUT_OP_DOMAIN);
        bytes.extend_from_slice(enclave.as_bytes());
        bytes.extend_from_slice(author);
        bytes.extend_from_slice(&lamport.to_be_bytes());
        bytes.extend_from_slice(
            &serde_json::to_vec(op).expect("layout operations are always serializable"),
        );
        bytes
    }

    /// Signs one operation.
    pub fn sign(enclave: EnclaveLayoutId, secret: &SecretKey, lamport: u64, op: LayoutOp) -> Self {
        let author = *ed25519::derive_public(secret).as_bytes();
        let bytes = Self::signing_bytes(enclave, &author, lamport, &op);
        let signature = ed25519::sign(secret, &bytes);
        Self {
            author,
            lamport,
            op,
            signature: signature.as_bytes().to_vec(),
        }
    }

    /// Stable identity: the digest of the signing bytes. Two clients that
    /// produce byte-identical operations produce the same id, so a duplicate
    /// delivery merges instead of applying twice.
    pub fn op_id(&self, enclave: EnclaveLayoutId) -> [u8; 32] {
        let bytes = Self::signing_bytes(enclave, &self.author, self.lamport, &self.op);
        Sha256::digest(bytes).into()
    }

    /// Verifies authorship. A wrong signature is never admitted.
    pub fn verify(&self, enclave: EnclaveLayoutId) -> bool {
        let Ok(signature) = <[u8; 64]>::try_from(self.signature.as_slice()) else {
            return false;
        };
        let bytes = Self::signing_bytes(enclave, &self.author, self.lamport, &self.op);
        ed25519::verify(
            &PublicKey::from_bytes(self.author),
            &bytes,
            &Signature::from_bytes(signature),
        )
        .unwrap_or(false)
    }

    /// The canonical on-disk record for this operation, used for the log's
    /// measured byte size.
    pub fn canonical_record(&self, enclave: EnclaveLayoutId) -> Vec<u8> {
        let mut bytes = Self::signing_bytes(enclave, &self.author, self.lamport, &self.op);
        bytes.extend_from_slice(&self.signature);
        bytes
    }

    /// The member this operation is attributed to.
    pub fn author_member(&self) -> SpaceMemberId {
        member_id_for_key(&self.author)
    }
}

/// Derives the roster member reference for an identity key.
pub fn member_id_for_key(key: &[u8; 32]) -> SpaceMemberId {
    let digest: [u8; 32] = Sha256::digest(key).into();
    SpaceMemberId::from_identity_key_digest(digest)
        .expect("a SHA-256 digest of a key is never the zero digest")
}

/// The on-disk form of a log. A JSON object cannot key on a tuple, and the
/// verification memo is deliberately not persisted: a log read back from disk
/// is re-verified before it is trusted.
#[derive(Clone, Debug, Deserialize, Serialize)]
struct LayoutLogWire {
    enclave: EnclaveLayoutId,
    founder: [u8; 32],
    ops: Vec<SignedLayoutOp>,
}

/// The append-only signed log for one Enclave's layout.
///
/// Every operation in `ops` has had its signature checked, either by
/// [`SignedLayoutLog::append`] or by [`SignedLayoutLog::verify_all`] after a
/// read from disk. `verified` records that, so replaying the log does not pay
/// for a signature check per projection.
#[derive(Clone, Debug, Eq, PartialEq)]
#[derive(Deserialize, Serialize)]
#[serde(from = "LayoutLogWire", into = "LayoutLogWire")]
pub struct SignedLayoutLog {
    enclave: EnclaveLayoutId,
    founder: [u8; 32],
    ops: BTreeMap<(u64, [u8; 32]), SignedLayoutOp>,
    verified: BTreeSet<[u8; 32]>,
    bytes: u64,
}

impl From<LayoutLogWire> for SignedLayoutLog {
    fn from(wire: LayoutLogWire) -> Self {
        let mut log = Self {
            enclave: wire.enclave,
            founder: wire.founder,
            ops: BTreeMap::new(),
            verified: BTreeSet::new(),
            bytes: 0,
        };
        for signed in wire.ops {
            let id = signed.op_id(log.enclave);
            if log.ops.contains_key(&(signed.lamport, id)) {
                continue;
            }
            log.bytes += signed.canonical_record(log.enclave).len() as u64;
            log.ops.insert((signed.lamport, id), signed);
        }
        log
    }
}

impl From<SignedLayoutLog> for LayoutLogWire {
    fn from(log: SignedLayoutLog) -> Self {
        Self {
            enclave: log.enclave,
            founder: log.founder,
            ops: log.ops.into_values().collect(),
        }
    }
}

impl SignedLayoutLog {
    /// Creates the genesis log. The founder key is the root of authority.
    pub fn new(enclave: EnclaveLayoutId, founder: [u8; 32]) -> Self {
        Self {
            enclave,
            founder,
            ops: BTreeMap::new(),
            verified: BTreeSet::new(),
            bytes: 0,
        }
    }

    pub const fn enclave(&self) -> EnclaveLayoutId {
        self.enclave
    }

    pub const fn founder(&self) -> &[u8; 32] {
        &self.founder
    }

    pub fn founder_member(&self) -> SpaceMemberId {
        member_id_for_key(&self.founder)
    }

    /// Appends one operation, rejecting a bad signature outright.
    ///
    /// Appending is not admission: whether the operation takes effect is
    /// decided deterministically by [`EnclaveLayout::project`].
    pub fn append(&mut self, signed: SignedLayoutOp) -> Result<[u8; 32], LayoutError> {
        let id = signed.op_id(self.enclave);
        let key = (signed.lamport, id);
        if self.ops.contains_key(&key) {
            // The same operation delivered twice is the same operation; it is
            // identified before it is verified so a reconnect does not pay to
            // re-check everything both sides already hold.
            return Ok(id);
        }
        if !signed.verify(self.enclave) {
            return Err(LayoutError::BadSignature);
        }
        self.bytes += signed.canonical_record(self.enclave).len() as u64;
        self.ops.insert(key, signed);
        self.verified.insert(id);
        Ok(id)
    }

    /// Whether this operation's signature has been checked by this replica.
    pub fn is_verified(&self, op_id: &[u8; 32]) -> bool {
        self.verified.contains(op_id)
    }

    /// Re-checks every signature that has not been checked on this replica,
    /// which is every one of them after a read from disk. A log that has been
    /// tampered with is refused outright rather than partially trusted.
    pub fn verify_all(&mut self) -> Result<usize, LayoutError> {
        let mut checked = 0;
        for ((_, id), signed) in &self.ops {
            if self.verified.contains(id) {
                continue;
            }
            if !signed.verify(self.enclave) {
                return Err(LayoutError::BadSignature);
            }
            checked += 1;
        }
        let ids: Vec<[u8; 32]> = self.ops.keys().map(|(_, id)| *id).collect();
        self.verified.extend(ids);
        Ok(checked)
    }

    /// Merges another replica's log. Returns how many operations were new.
    pub fn merge(&mut self, other: &Self) -> Result<usize, LayoutError> {
        if other.enclave != self.enclave || other.founder != self.founder {
            return Err(LayoutError::DifferentEnclave);
        }
        let mut added = 0;
        for signed in other.ops.values() {
            let before = self.ops.len();
            self.append(signed.clone())?;
            if self.ops.len() != before {
                added += 1;
            }
        }
        Ok(added)
    }

    /// The operations in the deterministic total order every replica uses.
    pub fn ordered(&self) -> impl Iterator<Item = (&(u64, [u8; 32]), &SignedLayoutOp)> {
        self.ops.iter()
    }

    pub fn len(&self) -> usize {
        self.ops.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }

    /// The highest Lamport counter seen, so a new operation can follow it.
    pub fn max_lamport(&self) -> u64 {
        self.ops.keys().next_back().map_or(0, |(lamport, _)| *lamport)
    }

    /// The measured serialized size of the whole log.
    pub fn byte_len(&self) -> u64 {
        self.bytes
    }

    /// The whole log as bytes, in total order. `canonical_bytes().len()`
    /// equals [`SignedLayoutLog::byte_len`] by construction.
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(self.bytes as usize);
        for signed in self.ops.values() {
            bytes.extend_from_slice(&signed.canonical_record(self.enclave));
        }
        bytes
    }
}

/// Why one operation was refused.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "reason")]
pub enum RejectionReason {
    BadSignature,
    NotAMember,
    NotAuthorized { action: String },
    UnknownCategory,
    UnknownChannel,
    UnknownRole,
    DuplicateId,
    CategoryNotEmpty { channels: usize },
    ChannelNotEmpty { messages: usize },
    DestinationMissing,
    DestinationNotSafe,
}

/// One refused operation, kept so a client can say what it declined.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RejectedOp {
    pub op_id: [u8; 32],
    pub author: SpaceMemberId,
    pub reason: RejectionReason,
}

/// A record of content destroyed by an explicit burn during deletion.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BurnRecord {
    pub channel: ChannelId,
    pub channel_name: String,
    pub messages: Vec<MessageId>,
}

/// The state every replica folds from the same operation set.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EnclaveLayout {
    enclave: EnclaveLayoutId,
    founder: SpaceMemberId,
    members: BTreeSet<SpaceMemberId>,
    roles: BTreeMap<RoleId, CustomRole>,
    grants: BTreeMap<SpaceMemberId, BTreeSet<RoleId>>,
    categories: BTreeMap<CategoryId, Category>,
    channels: BTreeMap<ChannelId, Channel>,
    messages: Vec<StoredMessage>,
    burns: Vec<BurnRecord>,
    rejections: Vec<RejectedOp>,
    applied: usize,
}

impl EnclaveLayout {
    /// Replays a log. This is the only way state is produced; there is no
    /// mutable side door, so no client can hold state its log cannot justify.
    pub fn project(log: &SignedLayoutLog) -> Self {
        let founder = log.founder_member();
        let mut state = Self {
            enclave: log.enclave(),
            founder,
            members: BTreeSet::from([founder]),
            roles: BTreeMap::new(),
            grants: BTreeMap::new(),
            categories: BTreeMap::new(),
            channels: BTreeMap::new(),
            messages: Vec::new(),
            burns: Vec::new(),
            rejections: Vec::new(),
            applied: 0,
        };
        for ((_, op_id), signed) in log.ordered() {
            state.apply(*op_id, signed, log.is_verified(op_id));
        }
        state
    }

    fn reject(&mut self, op_id: [u8; 32], author: SpaceMemberId, reason: RejectionReason) {
        self.rejections.push(RejectedOp {
            op_id,
            author,
            reason,
        });
    }

    #[allow(clippy::too_many_lines)]
    fn apply(&mut self, op_id: [u8; 32], signed: &SignedLayoutOp, verified: bool) {
        let author = signed.author_member();
        if !verified {
            self.reject(op_id, author, RejectionReason::BadSignature);
            return;
        }
        if !self.members.contains(&author) {
            self.reject(op_id, author, RejectionReason::NotAMember);
            return;
        }

        match &signed.op {
            LayoutOp::AdmitMember { key } => {
                if !self.has_enclave_authority(author) {
                    self.reject(op_id, author, not_authorized("admit-member"));
                    return;
                }
                self.members.insert(member_id_for_key(key));
            }
            LayoutOp::DefineRole {
                role,
                name,
                authority,
            } => {
                if !self.has_enclave_authority(author) {
                    self.reject(op_id, author, not_authorized("define-role"));
                    return;
                }
                if self.roles.contains_key(role) {
                    self.reject(op_id, author, RejectionReason::DuplicateId);
                    return;
                }
                self.roles.insert(
                    *role,
                    CustomRole {
                        id: *role,
                        name: name.clone(),
                        authority: *authority,
                    },
                );
            }
            LayoutOp::RenameRole { role, name } => {
                if !self.has_enclave_authority(author) {
                    self.reject(op_id, author, not_authorized("rename-role"));
                    return;
                }
                let Some(existing) = self.roles.get_mut(role) else {
                    self.reject(op_id, author, RejectionReason::UnknownRole);
                    return;
                };
                // Only the label changes. Authority stays bound to the id, so
                // a STEWARDS channel is unaffected by any rename.
                existing.name = name.clone();
            }
            LayoutOp::GrantRole { member, role } => {
                if !self.has_enclave_authority(author) {
                    self.reject(op_id, author, not_authorized("grant-role"));
                    return;
                }
                if !self.roles.contains_key(role) {
                    self.reject(op_id, author, RejectionReason::UnknownRole);
                    return;
                }
                if !self.members.contains(member) {
                    self.reject(op_id, author, RejectionReason::NotAMember);
                    return;
                }
                self.grants.entry(*member).or_default().insert(*role);
            }
            LayoutOp::CreateCategory {
                category,
                name,
                position,
            } => {
                if !self.has_enclave_authority(author) {
                    self.reject(op_id, author, not_authorized("create-category"));
                    return;
                }
                if self.categories.contains_key(category) {
                    self.reject(op_id, author, RejectionReason::DuplicateId);
                    return;
                }
                self.categories.insert(
                    *category,
                    Category {
                        id: *category,
                        name: name.clone(),
                        position: *position,
                        overrides: BTreeMap::new(),
                    },
                );
            }
            LayoutOp::RenameCategory { category, name } => {
                if !self.may_manage_category(*category, author) {
                    self.reject(op_id, author, not_authorized("rename-category"));
                    return;
                }
                let Some(existing) = self.categories.get_mut(category) else {
                    self.reject(op_id, author, RejectionReason::UnknownCategory);
                    return;
                };
                existing.name = name.clone();
            }
            LayoutOp::ReorderCategory { category, position } => {
                if !self.may_manage_category(*category, author) {
                    self.reject(op_id, author, not_authorized("reorder-category"));
                    return;
                }
                let Some(existing) = self.categories.get_mut(category) else {
                    self.reject(op_id, author, RejectionReason::UnknownCategory);
                    return;
                };
                existing.position = *position;
            }
            LayoutOp::DeleteCategory {
                category,
                channels_to,
            } => {
                if !self.may_manage_category(*category, author) {
                    self.reject(op_id, author, not_authorized("delete-category"));
                    return;
                }
                if !self.categories.contains_key(category) {
                    self.reject(op_id, author, RejectionReason::UnknownCategory);
                    return;
                }
                let held: Vec<ChannelId> = self
                    .channels
                    .values()
                    .filter(|channel| channel.category == *category)
                    .map(|channel| channel.id)
                    .collect();
                if !held.is_empty() {
                    let Some(destination) = channels_to else {
                        self.reject(
                            op_id,
                            author,
                            RejectionReason::CategoryNotEmpty {
                                channels: held.len(),
                            },
                        );
                        return;
                    };
                    if destination == category || !self.categories.contains_key(destination) {
                        self.reject(op_id, author, RejectionReason::DestinationNotSafe);
                        return;
                    }
                    for channel in held {
                        if let Some(existing) = self.channels.get_mut(&channel) {
                            existing.category = *destination;
                        }
                    }
                }
                self.categories.remove(category);
            }
            LayoutOp::SetCategoryOverride {
                category,
                role,
                over,
            } => {
                if !self.may_manage_category(*category, author) {
                    self.reject(op_id, author, not_authorized("set-category-override"));
                    return;
                }
                if !self.roles.contains_key(role) {
                    self.reject(op_id, author, RejectionReason::UnknownRole);
                    return;
                }
                let Some(existing) = self.categories.get_mut(category) else {
                    self.reject(op_id, author, RejectionReason::UnknownCategory);
                    return;
                };
                if over.is_inherit() {
                    existing.overrides.remove(role);
                } else {
                    existing.overrides.insert(*role, *over);
                }
            }
            LayoutOp::CreateChannel {
                channel,
                category,
                name,
                mode,
                position,
            } => {
                if !self.categories.contains_key(category) {
                    self.reject(op_id, author, RejectionReason::UnknownCategory);
                    return;
                }
                if !self.may_manage_category(*category, author) {
                    self.reject(op_id, author, not_authorized("create-channel"));
                    return;
                }
                if self.channels.contains_key(channel) {
                    self.reject(op_id, author, RejectionReason::DuplicateId);
                    return;
                }
                self.channels.insert(
                    *channel,
                    Channel {
                        id: *channel,
                        category: *category,
                        name: name.clone(),
                        mode: *mode,
                        position: *position,
                        overrides: BTreeMap::new(),
                    },
                );
            }
            LayoutOp::RenameChannel { channel, name } => {
                if !self.guard_channel_manage(op_id, author, *channel, "rename-channel") {
                    return;
                }
                self.channels
                    .get_mut(channel)
                    .expect("guard proved the channel exists")
                    .name = name.clone();
            }
            LayoutOp::ReorderChannel { channel, position } => {
                if !self.guard_channel_manage(op_id, author, *channel, "reorder-channel") {
                    return;
                }
                self.channels
                    .get_mut(channel)
                    .expect("guard proved the channel exists")
                    .position = *position;
            }
            LayoutOp::MoveChannel {
                channel,
                category,
                position,
            } => {
                if !self.guard_channel_manage(op_id, author, *channel, "move-channel") {
                    return;
                }
                if !self.categories.contains_key(category) {
                    self.reject(op_id, author, RejectionReason::UnknownCategory);
                    return;
                }
                if !self.may_manage_category(*category, author) {
                    self.reject(op_id, author, not_authorized("move-channel-destination"));
                    return;
                }
                let existing = self
                    .channels
                    .get_mut(channel)
                    .expect("guard proved the channel exists");
                existing.category = *category;
                existing.position = *position;
            }
            LayoutOp::SetChannelMode { channel, mode } => {
                if !self.guard_channel_manage(op_id, author, *channel, "set-channel-mode") {
                    return;
                }
                self.channels
                    .get_mut(channel)
                    .expect("guard proved the channel exists")
                    .mode = *mode;
            }
            LayoutOp::SetChannelOverride {
                channel,
                role,
                over,
            } => {
                if !self.guard_channel_manage(op_id, author, *channel, "set-channel-override") {
                    return;
                }
                if !self.roles.contains_key(role) {
                    self.reject(op_id, author, RejectionReason::UnknownRole);
                    return;
                }
                let existing = self
                    .channels
                    .get_mut(channel)
                    .expect("guard proved the channel exists");
                if over.is_inherit() {
                    existing.overrides.remove(role);
                } else {
                    existing.overrides.insert(*role, *over);
                }
            }
            LayoutOp::ClearChannelOverride { channel, role } => {
                if !self.guard_channel_manage(op_id, author, *channel, "clear-channel-override") {
                    return;
                }
                self.channels
                    .get_mut(channel)
                    .expect("guard proved the channel exists")
                    .overrides
                    .remove(role);
            }
            LayoutOp::DeleteChannel {
                channel,
                disposition,
            } => {
                if !self.guard_channel_manage(op_id, author, *channel, "delete-channel") {
                    return;
                }
                let held: Vec<MessageId> = self
                    .messages
                    .iter()
                    .filter(|message| message.channel == *channel)
                    .map(|message| message.id)
                    .collect();
                if !held.is_empty() {
                    let Some(disposition) = disposition else {
                        // A nonempty channel never disappears by default: the
                        // author must name a destination or burn on purpose.
                        self.reject(
                            op_id,
                            author,
                            RejectionReason::ChannelNotEmpty {
                                messages: held.len(),
                            },
                        );
                        return;
                    };
                    match disposition {
                        ChannelDisposition::MoveContentTo { channel: target } => {
                            if target == channel {
                                self.reject(op_id, author, RejectionReason::DestinationNotSafe);
                                return;
                            }
                            if !self.channels.contains_key(target) {
                                self.reject(op_id, author, RejectionReason::DestinationMissing);
                                return;
                            }
                            if !self.resolve(*target, author, ChannelAction::Manage).allowed {
                                self.reject(op_id, author, RejectionReason::DestinationNotSafe);
                                return;
                            }
                            for message in &mut self.messages {
                                if message.channel == *channel {
                                    message.moved_from = Some(*channel);
                                    message.channel = *target;
                                }
                            }
                        }
                        ChannelDisposition::BurnContent => {
                            let name = self
                                .channels
                                .get(channel)
                                .map(|existing| existing.name.clone())
                                .unwrap_or_default();
                            self.messages.retain(|message| message.channel != *channel);
                            self.burns.push(BurnRecord {
                                channel: *channel,
                                channel_name: name,
                                messages: held,
                            });
                        }
                    }
                }
                self.channels.remove(channel);
            }
            LayoutOp::PostMessage {
                channel,
                message,
                body,
            } => {
                if !self.channels.contains_key(channel) {
                    self.reject(op_id, author, RejectionReason::UnknownChannel);
                    return;
                }
                if !self.resolve(*channel, author, ChannelAction::Post).allowed {
                    self.reject(op_id, author, not_authorized("post"));
                    return;
                }
                if self.messages.iter().any(|held| held.id == *message) {
                    self.reject(op_id, author, RejectionReason::DuplicateId);
                    return;
                }
                self.messages.push(StoredMessage {
                    id: *message,
                    channel: *channel,
                    author,
                    body: body.clone(),
                    moved_from: None,
                });
            }
        }
        self.applied += 1;
    }

    fn guard_channel_manage(
        &mut self,
        op_id: [u8; 32],
        author: SpaceMemberId,
        channel: ChannelId,
        action: &str,
    ) -> bool {
        if !self.channels.contains_key(&channel) {
            self.reject(op_id, author, RejectionReason::UnknownChannel);
            return false;
        }
        if !self.resolve(channel, author, ChannelAction::Manage).allowed {
            self.reject(op_id, author, not_authorized(action));
            return false;
        }
        true
    }

    // ----- queries -------------------------------------------------------

    pub const fn enclave(&self) -> EnclaveLayoutId {
        self.enclave
    }

    pub const fn founder(&self) -> SpaceMemberId {
        self.founder
    }

    pub fn is_member(&self, member: SpaceMemberId) -> bool {
        self.members.contains(&member)
    }

    pub fn members(&self) -> &BTreeSet<SpaceMemberId> {
        &self.members
    }

    pub fn roles(&self) -> &BTreeMap<RoleId, CustomRole> {
        &self.roles
    }

    pub fn role(&self, role: RoleId) -> Option<&CustomRole> {
        self.roles.get(&role)
    }

    pub fn roles_of(&self, member: SpaceMemberId) -> BTreeSet<RoleId> {
        self.grants.get(&member).cloned().unwrap_or_default()
    }

    pub fn category(&self, category: CategoryId) -> Option<&Category> {
        self.categories.get(&category)
    }

    pub fn channel(&self, channel: ChannelId) -> Option<&Channel> {
        self.channels.get(&channel)
    }

    pub fn channels(&self) -> &BTreeMap<ChannelId, Channel> {
        &self.channels
    }

    pub fn categories(&self) -> &BTreeMap<CategoryId, Category> {
        &self.categories
    }

    pub fn messages(&self) -> &[StoredMessage] {
        &self.messages
    }

    pub fn burns(&self) -> &[BurnRecord] {
        &self.burns
    }

    pub fn rejections(&self) -> &[RejectedOp] {
        &self.rejections
    }

    pub const fn applied(&self) -> usize {
        self.applied
    }

    /// The set of role ids that carry authority. `STEWARDS` is exactly this.
    pub fn authority_role_ids(&self) -> BTreeSet<RoleId> {
        self.roles
            .values()
            .filter(|role| role.authority)
            .map(|role| role.id)
            .collect()
    }

    /// Whether the member holds enclave-level authority. The founder always
    /// does; everyone else does through an authority-bearing role **id**.
    pub fn has_enclave_authority(&self, member: SpaceMemberId) -> bool {
        if member == self.founder {
            return true;
        }
        self.grants.get(&member).is_some_and(|held| {
            held.iter()
                .any(|role| self.roles.get(role).is_some_and(|role| role.authority))
        })
    }

    /// The authoritative category order: position first, id as the tie-break
    /// so two clients that pick the same position still agree.
    pub fn ordered_categories(&self) -> Vec<&Category> {
        let mut ordered: Vec<&Category> = self.categories.values().collect();
        ordered.sort_by(|left, right| {
            left.position
                .cmp(&right.position)
                .then_with(|| left.id.cmp(&right.id))
        });
        ordered
    }

    /// The authoritative channel order inside one category.
    pub fn ordered_channels(&self, category: CategoryId) -> Vec<&Channel> {
        let mut ordered: Vec<&Channel> = self
            .channels
            .values()
            .filter(|channel| channel.category == category)
            .collect();
        ordered.sort_by(|left, right| {
            left.position
                .cmp(&right.position)
                .then_with(|| left.id.cmp(&right.id))
        });
        ordered
    }

    /// The whole tree in authoritative order. This, not the DOM, is the order.
    pub fn ordered_tree(&self) -> Vec<(CategoryId, Vec<ChannelId>)> {
        self.ordered_categories()
            .into_iter()
            .map(|category| {
                (
                    category.id,
                    self.ordered_channels(category.id)
                        .into_iter()
                        .map(|channel| channel.id)
                        .collect(),
                )
            })
            .collect()
    }

    /// Every message whose channel no longer exists. Deletion is correct only
    /// while this stays empty.
    pub fn orphaned_messages(&self) -> Vec<&StoredMessage> {
        self.messages
            .iter()
            .filter(|message| !self.channels.contains_key(&message.channel))
            .collect()
    }

    pub fn messages_in(&self, channel: ChannelId) -> Vec<&StoredMessage> {
        self.messages
            .iter()
            .filter(|message| message.channel == channel)
            .collect()
    }

    /// The messages a member may actually read, resolved per channel.
    pub fn readable_messages(&self, member: SpaceMemberId) -> Vec<&StoredMessage> {
        self.messages
            .iter()
            .filter(|message| self.resolve(message.channel, member, ChannelAction::Read).allowed)
            .collect()
    }

    // ----- the one resolver ----------------------------------------------

    /// Resolves one member's access to one channel for one action.
    ///
    /// Precedence, deterministic and independent of role iteration order:
    /// 1. an explicit override on the channel for a role the member holds —
    ///    any `Deny` wins over any `Allow`;
    /// 2. otherwise the same rule on the channel's category;
    /// 3. otherwise the channel's shipped mode, where `STEWARDS` consults the
    ///    authority-bearing role **ids**.
    pub fn resolve(
        &self,
        channel: ChannelId,
        member: SpaceMemberId,
        action: ChannelAction,
    ) -> AccessDecision {
        let Some(channel) = self.channels.get(&channel) else {
            return AccessDecision::new(false, AccessSource::NotAMember);
        };
        if !self.members.contains(&member) {
            return AccessDecision::new(false, AccessSource::NotAMember);
        }
        let held = self.roles_of(member);

        if let Some((role, bit)) = pick_override(&channel.overrides, &held, action) {
            return AccessDecision::new(
                bit == OverrideBit::Allow,
                AccessSource::ChannelOverride { role },
            );
        }
        if let Some(category) = self.categories.get(&channel.category) {
            if let Some((role, bit)) = pick_override(&category.overrides, &held, action) {
                return AccessDecision::new(
                    bit == OverrideBit::Allow,
                    AccessSource::CategoryOverride { role },
                );
            }
        }
        let authority = self.has_enclave_authority(member);
        AccessDecision::new(
            channel.mode.base_allows(action, authority),
            AccessSource::InheritedMode { mode: channel.mode },
        )
    }

    /// Category-scoped manage, used to authorise category and create-channel
    /// operations. Same precedence, one level up.
    pub fn resolve_category(
        &self,
        category: CategoryId,
        member: SpaceMemberId,
        action: ChannelAction,
    ) -> AccessDecision {
        if !self.members.contains(&member) {
            return AccessDecision::new(false, AccessSource::NotAMember);
        }
        if let Some(existing) = self.categories.get(&category) {
            let held = self.roles_of(member);
            if let Some((role, bit)) = pick_override(&existing.overrides, &held, action) {
                return AccessDecision::new(
                    bit == OverrideBit::Allow,
                    AccessSource::CategoryOverride { role },
                );
            }
        }
        AccessDecision::new(
            self.has_enclave_authority(member),
            AccessSource::InheritedEnclaveAuthority,
        )
    }

    fn may_manage_category(&self, category: CategoryId, member: SpaceMemberId) -> bool {
        self.resolve_category(category, member, ChannelAction::Manage)
            .allowed
    }

    /// The rows the permission grid renders: every role's cell for one
    /// channel, each labelled inherited or overridden.
    pub fn permission_grid(&self, channel: ChannelId) -> Vec<PermissionGridRow> {
        let mut rows = Vec::new();
        for role in self.roles.values() {
            let representative = self.representative_member_for(role.id);
            let cells = ChannelAction::ALL.map(|action| {
                let decision = representative.map_or_else(
                    || self.hypothetical(channel, role.id, action),
                    |member| self.resolve(channel, member, action),
                );
                PermissionGridCell {
                    action,
                    allowed: decision.allowed,
                    source: decision.source,
                }
            });
            rows.push(PermissionGridRow {
                role: role.id,
                role_name: role.name.clone(),
                authority: role.authority,
                cells: cells.to_vec(),
            });
        }
        rows
    }

    fn representative_member_for(&self, role: RoleId) -> Option<SpaceMemberId> {
        self.grants
            .iter()
            .find(|(member, held)| held.contains(&role) && **member != self.founder)
            .map(|(member, _)| *member)
            .or_else(|| {
                self.grants
                    .iter()
                    .find(|(_, held)| held.contains(&role))
                    .map(|(member, _)| *member)
            })
    }

    /// The decision a member holding exactly this one role would get. Used to
    /// render a grid row for a role nobody holds yet.
    fn hypothetical(
        &self,
        channel: ChannelId,
        role: RoleId,
        action: ChannelAction,
    ) -> AccessDecision {
        let Some(existing) = self.channels.get(&channel) else {
            return AccessDecision::new(false, AccessSource::NotAMember);
        };
        let held = BTreeSet::from([role]);
        if let Some((role, bit)) = pick_override(&existing.overrides, &held, action) {
            return AccessDecision::new(
                bit == OverrideBit::Allow,
                AccessSource::ChannelOverride { role },
            );
        }
        if let Some(category) = self.categories.get(&existing.category) {
            if let Some((role, bit)) = pick_override(&category.overrides, &held, action) {
                return AccessDecision::new(
                    bit == OverrideBit::Allow,
                    AccessSource::CategoryOverride { role },
                );
            }
        }
        let authority = self.roles.get(&role).is_some_and(|role| role.authority);
        AccessDecision::new(
            existing.mode.base_allows(action, authority),
            AccessSource::InheritedMode { mode: existing.mode },
        )
    }
}

/// One row of the rendered permission grid.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PermissionGridRow {
    pub role: RoleId,
    pub role_name: String,
    pub authority: bool,
    pub cells: Vec<PermissionGridCell>,
}

/// One cell of the rendered permission grid.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PermissionGridCell {
    pub action: ChannelAction,
    pub allowed: bool,
    pub source: AccessSource,
}

fn pick_override(
    overrides: &BTreeMap<RoleId, RoleOverride>,
    held: &BTreeSet<RoleId>,
    action: ChannelAction,
) -> Option<(RoleId, OverrideBit)> {
    let mut allow: Option<RoleId> = None;
    for role in held {
        let Some(over) = overrides.get(role) else {
            continue;
        };
        match over.bit(action) {
            // A deny anywhere in the member's roles wins outright, and the
            // lowest role id is reported so every replica names the same one.
            Some(OverrideBit::Deny) => return Some((*role, OverrideBit::Deny)),
            Some(OverrideBit::Allow) => allow = allow.or(Some(*role)),
            None => {}
        }
    }
    allow.map(|role| (role, OverrideBit::Allow))
}

fn not_authorized(action: &str) -> RejectionReason {
    RejectionReason::NotAuthorized {
        action: action.to_owned(),
    }
}

/// Errors a client raises before it signs anything.
#[derive(Clone, Debug, thiserror::Error, Eq, PartialEq)]
pub enum LayoutError {
    #[error("OSL: this layout operation is not signed by its author")]
    BadSignature,
    #[error("OSL: that log belongs to a different Enclave")]
    DifferentEnclave,
    #[error("OSL: no channel with that id is in this Enclave")]
    UnknownChannel,
    #[error("OSL: no category with that id is in this Enclave")]
    UnknownCategory,
    #[error(
        "OSL: \"{channel_name}\" still holds {messages} messages. Choose a channel to move them to, or burn them on purpose."
    )]
    DestinationRequired { channel_name: String, messages: usize },
    #[error("OSL: that destination cannot receive the messages")]
    DestinationNotSafe,
    #[error(
        "OSL: this device measured room for {measured_channels} channels in a {budget_bytes}-byte enclave layout log and it now holds {log_bytes} bytes"
    )]
    LogBudgetExhausted {
        measured_channels: usize,
        log_bytes: u64,
        budget_bytes: u64,
    },
    #[error("OSL: the operation was refused: {0}")]
    Refused(String),
}

/// Each member's own collapse state. This never enters the signed log: a
/// category folded shut on one device says nothing to the Enclave.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct CollapseProfile {
    pub collapsed: BTreeSet<CategoryId>,
}

impl CollapseProfile {
    pub fn collapse(&mut self, category: CategoryId) {
        self.collapsed.insert(category);
    }

    pub fn expand(&mut self, category: CategoryId) {
        self.collapsed.remove(&category);
    }

    pub fn toggle(&mut self, category: CategoryId) -> bool {
        if self.collapsed.contains(&category) {
            self.collapsed.remove(&category);
            false
        } else {
            self.collapsed.insert(category);
            true
        }
    }

    pub fn is_collapsed(&self, category: CategoryId) -> bool {
        self.collapsed.contains(&category)
    }
}

/// The per-device file that holds the shared signed layout log.
pub fn layout_log_path(dir: &Path, member: SpaceMemberId) -> PathBuf {
    dir.join(format!(
        "enclave_layout_{}.json",
        hex::encode(member.as_bytes())
    ))
}

/// The per-device file that holds one member's collapse state.
pub fn collapse_profile_path(dir: &Path, member: SpaceMemberId) -> PathBuf {
    dir.join(format!(
        "enclave_collapse_{}.json",
        hex::encode(member.as_bytes())
    ))
}

pub fn save_collapse_profile(
    dir: &Path,
    member: SpaceMemberId,
    profile: &CollapseProfile,
) -> io::Result<()> {
    fs::create_dir_all(dir)?;
    let path = collapse_profile_path(dir, member);
    let bytes = serde_json::to_vec(profile).map_err(io::Error::other)?;
    fs::write(path, bytes)
}

pub fn load_collapse_profile(dir: &Path, member: SpaceMemberId) -> io::Result<CollapseProfile> {
    let path = collapse_profile_path(dir, member);
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(io::Error::other),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(CollapseProfile::default()),
        Err(error) => Err(error),
    }
}

/// A measured, not documented, ceiling on the signed log.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct LayoutBudget {
    pub log_bytes: u64,
}

impl LayoutBudget {
    pub const fn new(log_bytes: u64) -> Self {
        Self { log_bytes }
    }
}

impl Default for LayoutBudget {
    fn default() -> Self {
        Self::new(1 << 20)
    }
}

/// One client's replica: its key, its log, its own collapse state.
pub struct EnclaveClient {
    secret: SecretKey,
    public: [u8; 32],
    member: SpaceMemberId,
    log: SignedLayoutLog,
    lamport: u64,
    collapse: CollapseProfile,
    budget: LayoutBudget,
}

impl EnclaveClient {
    /// The founding client: its key is the root of the Enclave's authority.
    pub fn founder(enclave: EnclaveLayoutId, secret: SecretKey) -> Self {
        let public = *ed25519::derive_public(&secret).as_bytes();
        Self {
            member: member_id_for_key(&public),
            log: SignedLayoutLog::new(enclave, public),
            secret,
            public,
            lamport: 0,
            collapse: CollapseProfile::default(),
            budget: LayoutBudget::default(),
        }
    }

    /// Another member's client, which starts from the same genesis.
    pub fn joining(enclave: EnclaveLayoutId, founder: [u8; 32], secret: SecretKey) -> Self {
        let public = *ed25519::derive_public(&secret).as_bytes();
        Self {
            member: member_id_for_key(&public),
            log: SignedLayoutLog::new(enclave, founder),
            secret,
            public,
            lamport: 0,
            collapse: CollapseProfile::default(),
            budget: LayoutBudget::default(),
        }
    }

    pub fn with_budget(mut self, budget: LayoutBudget) -> Self {
        self.budget = budget;
        self
    }

    pub const fn member(&self) -> SpaceMemberId {
        self.member
    }

    pub const fn public_key(&self) -> &[u8; 32] {
        &self.public
    }

    pub const fn log(&self) -> &SignedLayoutLog {
        &self.log
    }

    pub fn layout(&self) -> EnclaveLayout {
        EnclaveLayout::project(&self.log)
    }

    pub const fn collapse(&self) -> &CollapseProfile {
        &self.collapse
    }

    pub fn collapse_mut(&mut self) -> &mut CollapseProfile {
        &mut self.collapse
    }

    pub fn set_collapse(&mut self, profile: CollapseProfile) {
        self.collapse = profile;
    }

    /// Signs and appends an operation.
    ///
    /// Signing is not admission. The operation is appended once it verifies,
    /// and whether it takes effect is decided by the projection every replica
    /// runs — so a client cannot buy authority by skipping its own check.
    pub fn submit(&mut self, op: LayoutOp) -> Result<SignedLayoutOp, LayoutError> {
        self.guard_budget(&op)?;
        self.lamport = self.lamport.max(self.log.max_lamport()) + 1;
        let signed = SignedLayoutOp::sign(self.log.enclave(), &self.secret, self.lamport, op);
        self.log.append(signed.clone())?;
        Ok(signed)
    }

    /// The friendly path a delete button takes: refuses before signing when a
    /// nonempty channel has neither a destination nor an explicit burn.
    pub fn delete_channel(
        &mut self,
        channel: ChannelId,
        disposition: Option<ChannelDisposition>,
    ) -> Result<SignedLayoutOp, LayoutError> {
        let layout = self.layout();
        let existing = layout.channel(channel).ok_or(LayoutError::UnknownChannel)?;
        let held = layout.messages_in(channel).len();
        if held > 0 {
            match disposition {
                None => {
                    return Err(LayoutError::DestinationRequired {
                        channel_name: existing.name.clone(),
                        messages: held,
                    })
                }
                Some(ChannelDisposition::MoveContentTo { channel: target }) => {
                    if target == channel || layout.channel(target).is_none() {
                        return Err(LayoutError::DestinationNotSafe);
                    }
                    if !layout.resolve(target, self.member, ChannelAction::Manage).allowed {
                        return Err(LayoutError::DestinationNotSafe);
                    }
                }
                Some(ChannelDisposition::BurnContent) => {}
            }
        }
        self.submit(LayoutOp::DeleteChannel {
            channel,
            disposition,
        })
    }

    /// Merges another replica's log into this one.
    pub fn sync_from(&mut self, other: &SignedLayoutLog) -> Result<usize, LayoutError> {
        let added = self.log.merge(other)?;
        self.lamport = self.lamport.max(self.log.max_lamport());
        Ok(added)
    }

    /// Writes this device's state: the shared signed log, and — in a separate
    /// file that no other member ever receives — this member's collapse state.
    pub fn save(&self, dir: &Path) -> io::Result<()> {
        fs::create_dir_all(dir)?;
        let bytes = serde_json::to_vec(&self.log).map_err(io::Error::other)?;
        fs::write(layout_log_path(dir, self.member), bytes)?;
        save_collapse_profile(dir, self.member, &self.collapse)
    }

    /// Reopens a client from disk after a restart. Nothing is carried over in
    /// memory: the log and the collapse profile are both read back.
    pub fn reopen(dir: &Path, secret: SecretKey) -> io::Result<Self> {
        let public = *ed25519::derive_public(&secret).as_bytes();
        let member = member_id_for_key(&public);
        let bytes = fs::read(layout_log_path(dir, member))?;
        let mut log: SignedLayoutLog = serde_json::from_slice(&bytes).map_err(io::Error::other)?;
        log.verify_all().map_err(io::Error::other)?;
        let collapse = load_collapse_profile(dir, member)?;
        Ok(Self {
            secret,
            public,
            member,
            lamport: log.max_lamport(),
            log,
            collapse,
            budget: LayoutBudget::default(),
        })
    }

    pub fn save_collapse(&self, dir: &Path) -> io::Result<()> {
        save_collapse_profile(dir, self.member, &self.collapse)
    }

    fn guard_budget(&self, op: &LayoutOp) -> Result<(), LayoutError> {
        // The record is the signing bytes plus a 64-byte signature, so its
        // size is known exactly without signing a throwaway copy.
        let record = SignedLayoutOp::signing_bytes(
            self.log.enclave(),
            &self.public,
            self.lamport + 1,
            op,
        )
        .len() as u64
            + ed25519::SIGNATURE_SIZE as u64;
        let projected = self.log.byte_len() + record;
        if projected > self.budget.log_bytes {
            return Err(LayoutError::LogBudgetExhausted {
                measured_channels: self.layout().channels().len(),
                log_bytes: self.log.byte_len(),
                budget_bytes: self.budget.log_bytes,
            });
        }
        Ok(())
    }
}

/// What one device measured, by building a real signed log until the budget
/// was spent. Nothing here is a constant.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MeasuredLayoutLimit {
    pub budget_bytes: u64,
    pub categories: usize,
    pub channels: usize,
    pub log_bytes: u64,
    pub bytes_per_channel: u64,
}

impl MeasuredLayoutLimit {
    /// The sentence the product shows. It says *measured*, and it says what it
    /// was measured against, because the number is a property of this device
    /// and this budget rather than a promise.
    pub fn disclosure(&self) -> String {
        format!(
            "Measured on this device: {} categories and {} channels fit in a {}-byte enclave layout log ({} bytes per channel measured over {} bytes). This is a measured limit, not a fixed ceiling.",
            self.categories, self.channels, self.budget_bytes, self.bytes_per_channel, self.log_bytes
        )
    }
}

/// Measures how much layout actually fits in `budget_bytes`.
///
/// This builds a real Enclave with real signatures and stops when the log
/// outgrows the budget. The result is what the run produced; there is no
/// documented ceiling anywhere for it to report instead.
pub fn measure_layout_limit(budget_bytes: u64) -> MeasuredLayoutLimit {
    let enclave = EnclaveLayoutId::from_bytes([0x6d; EnclaveLayoutId::LENGTH]);
    let mut client = EnclaveClient::founder(enclave, SecretKey::from_bytes([0x51; 32]))
        .with_budget(LayoutBudget::new(budget_bytes));

    let mut categories = 0_usize;
    let mut channels = 0_usize;
    let mut counter: u64 = 0;
    let mut category_ids: Vec<CategoryId> = Vec::new();

    // One category first, so channels have somewhere to live.
    let seed = CategoryId::from_bytes(counter_bytes(counter));
    counter += 1;
    if client
        .submit(LayoutOp::CreateCategory {
            category: seed,
            name: "measured".to_owned(),
            position: 0,
        })
        .is_ok()
    {
        categories += 1;
        category_ids.push(seed);
    }

    let bytes_after_first_category = client.log().byte_len();

    loop {
        let id = ChannelId::from_bytes(counter_bytes(counter));
        counter += 1;
        let outcome = client.submit(LayoutOp::CreateChannel {
            channel: id,
            category: seed,
            name: format!("c{channels}"),
            mode: ChannelMode::Open,
            position: channels as i64,
        });
        if outcome.is_err() {
            break;
        }
        channels += 1;
    }

    let channel_bytes = client.log().byte_len() - bytes_after_first_category;
    let bytes_per_channel = if channels == 0 {
        0
    } else {
        channel_bytes / channels as u64
    };

    // Now measure categories against the same budget in a fresh log, so the
    // two numbers are each a real measurement rather than a leftover.
    let mut category_client = EnclaveClient::founder(enclave, SecretKey::from_bytes([0x51; 32]))
        .with_budget(LayoutBudget::new(budget_bytes));
    let mut measured_categories = 0_usize;
    let mut category_counter: u64 = 1 << 40;
    loop {
        let id = CategoryId::from_bytes(counter_bytes(category_counter));
        category_counter += 1;
        let outcome = category_client.submit(LayoutOp::CreateCategory {
            category: id,
            name: format!("g{measured_categories}"),
            position: measured_categories as i64,
        });
        if outcome.is_err() {
            break;
        }
        measured_categories += 1;
    }
    categories = categories.max(measured_categories);

    MeasuredLayoutLimit {
        budget_bytes,
        categories,
        channels,
        log_bytes: client.log().byte_len(),
        bytes_per_channel,
    }
}

fn counter_bytes(counter: u64) -> [u8; 16] {
    let mut bytes = [0_u8; 16];
    bytes[..8].copy_from_slice(&counter.to_be_bytes());
    bytes[8..].copy_from_slice(&counter.wrapping_mul(0x9e37_79b9_7f4a_7c15).to_be_bytes());
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    fn enclave() -> EnclaveLayoutId {
        EnclaveLayoutId::from_bytes([1; EnclaveLayoutId::LENGTH])
    }

    #[test]
    fn a_tampered_operation_never_verifies() {
        let mut signed = SignedLayoutOp::sign(
            enclave(),
            &SecretKey::from_bytes([9; 32]),
            1,
            LayoutOp::CreateCategory {
                category: CategoryId::from_bytes([2; 16]),
                name: "Real".to_owned(),
                position: 0,
            },
        );
        assert!(signed.verify(enclave()));
        signed.op = LayoutOp::CreateCategory {
            category: CategoryId::from_bytes([2; 16]),
            name: "Forged".to_owned(),
            position: 0,
        };
        assert!(!signed.verify(enclave()));
    }

    #[test]
    fn every_shipped_mode_has_a_distinct_base_row() {
        let rows: Vec<[bool; 3]> = ChannelMode::SHIPPED
            .iter()
            .map(|mode| {
                [
                    mode.base_allows(ChannelAction::Read, false),
                    mode.base_allows(ChannelAction::Post, false),
                    mode.base_allows(ChannelAction::Manage, false),
                ]
            })
            .collect();
        assert_eq!(rows, [[true, true, false], [true, false, false], [false, false, false]]);
    }

    #[test]
    fn collapse_state_is_absent_from_the_signed_log() {
        let mut client = EnclaveClient::founder(enclave(), SecretKey::from_bytes([3; 32]));
        let category = CategoryId::from_bytes([7; 16]);
        client
            .submit(LayoutOp::CreateCategory {
                category,
                name: "Ops".to_owned(),
                position: 0,
            })
            .expect("founder may create a category");
        client.collapse_mut().collapse(category);
        let bytes = client.log().canonical_bytes();
        assert!(!String::from_utf8_lossy(&bytes).contains("collapse"));
    }
}
