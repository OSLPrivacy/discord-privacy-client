//! The signed membership ladder a departure walks.
//!
//! Groups and enclaves share one event format — `ipc::space_roster`'s
//! `SignedMembershipEvent` — on purpose. The place identity is inside the
//! signed bytes, so an event signed for one place cannot be replayed into
//! another, and both kinds inherit the same rules: exact-successor epochs, no
//! gaps, idempotent replays, and `actor == subject` for a self-removal.
//!
//! The actor's identity key is never taken from the event. A client resolves
//! it from its own roster by the member id, which is the digest of that key;
//! an event carrying a substituted key therefore resolves to a member id the
//! roster does not contain and is refused before any signature check.

use std::collections::{BTreeMap, BTreeSet};

use crypto::ed25519;
use ipc::space_roster::{
    MembershipEventApply, MembershipEventError, MembershipEventKind, MembershipEventLog, SpaceEpoch,
    SpaceId,
};
use serde::{Deserialize, Serialize};

use crate::authority::{RoleHolders, RoleTable};
use crate::ids::{MemberId, RoleId};

/// One member as a client's roster holds them.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MemberRecord {
    pub member_id: MemberId,
    pub handle: String,
    pub display_name: String,
    /// The identity key this roster will verify that member's events against.
    #[serde(with = "crate::keys::hex_bytes")]
    pub identity_key: Vec<u8>,
}

impl MemberRecord {
    pub fn identity_public(&self) -> Result<ed25519::PublicKey, PlaceMembershipError> {
        let mut bytes = [0_u8; ed25519::PUBLIC_KEY_SIZE];
        if self.identity_key.len() != bytes.len() {
            return Err(PlaceMembershipError::MalformedRosterKey);
        }
        bytes.copy_from_slice(&self.identity_key);
        Ok(ed25519::PublicKey::from_bytes(bytes))
    }
}

/// A persisted signed event, kept so a restart replays and re-verifies the
/// ladder rather than trusting a cached roster.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PersistedEvent {
    pub epoch: u64,
    pub kind: String,
    #[serde(with = "crate::keys::hex_bytes")]
    pub subject: Vec<u8>,
    #[serde(with = "crate::keys::hex_bytes")]
    pub signature: Vec<u8>,
}

pub fn kind_code(kind: MembershipEventKind) -> &'static str {
    match kind {
        MembershipEventKind::Create => "create",
        MembershipEventKind::Invite => "invite",
        MembershipEventKind::Join => "join",
        MembershipEventKind::Leave => "leave",
        MembershipEventKind::Remove => "remove",
        MembershipEventKind::RoleChange(_) => "role_change",
    }
}

fn kind_from_code(code: &str) -> Result<MembershipEventKind, PlaceMembershipError> {
    match code {
        "create" => Ok(MembershipEventKind::Create),
        "invite" => Ok(MembershipEventKind::Invite),
        "join" => Ok(MembershipEventKind::Join),
        "leave" => Ok(MembershipEventKind::Leave),
        "remove" => Ok(MembershipEventKind::Remove),
        _ => Err(PlaceMembershipError::UnknownEventKind),
    }
}

/// A signed membership event as it travels between clients.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WireEvent {
    #[serde(with = "crate::keys::hex_bytes")]
    pub place_event_id: Vec<u8>,
    pub epoch: u64,
    pub kind: String,
    #[serde(with = "crate::keys::hex_bytes")]
    pub subject: Vec<u8>,
    #[serde(with = "crate::keys::hex_bytes")]
    pub signature: Vec<u8>,
}

impl WireEvent {
    pub fn from_signed(event: &ipc::space_roster::SignedMembershipEvent) -> Self {
        Self {
            place_event_id: event.space_id.as_bytes().to_vec(),
            epoch: event.epoch.get(),
            kind: kind_code(event.kind).to_string(),
            subject: event.subject.as_bytes().to_vec(),
            signature: event.signature_bytes().to_vec(),
        }
    }

    fn to_signed(
        &self,
    ) -> Result<ipc::space_roster::SignedMembershipEvent, PlaceMembershipError> {
        let mut place = [0_u8; SpaceId::LENGTH];
        if self.place_event_id.len() != place.len() {
            return Err(PlaceMembershipError::MalformedEvent);
        }
        place.copy_from_slice(&self.place_event_id);
        let mut subject = [0_u8; ed25519::PUBLIC_KEY_SIZE];
        if self.subject.len() != subject.len() {
            return Err(PlaceMembershipError::MalformedEvent);
        }
        subject.copy_from_slice(&self.subject);
        let mut signature = [0_u8; ed25519::SIGNATURE_SIZE];
        if self.signature.len() != signature.len() {
            return Err(PlaceMembershipError::MalformedEvent);
        }
        signature.copy_from_slice(&self.signature);
        Ok(ipc::space_roster::SignedMembershipEvent::from_parts(
            SpaceId::from_persisted(place),
            SpaceEpoch::from_persisted(self.epoch),
            kind_from_code(&self.kind)?,
            ed25519::PublicKey::from_bytes(subject),
            signature,
        ))
    }
}

/// A client's view of one place's membership.
pub struct PlaceMembership {
    place_event_id: SpaceId,
    log: MembershipEventLog,
    members: BTreeMap<MemberId, MemberRecord>,
    roles: RoleTable,
    holders: RoleHolders,
    applied: Vec<PersistedEvent>,
    /// Members introduced alongside a create/join event but not yet on the
    /// roster. They become roster entries only when their signed event
    /// applies.
    pending: BTreeMap<MemberId, MemberRecord>,
}

impl PlaceMembership {
    /// Starts an empty ladder for a place whose replicated identity is
    /// `place_event_id`. Members arrive only through applied events.
    pub fn new(place_event_id: SpaceId, roles: RoleTable) -> Self {
        Self {
            place_event_id,
            log: MembershipEventLog::new(place_event_id),
            members: BTreeMap::new(),
            roles,
            holders: RoleHolders::new(),
            applied: Vec::new(),
            pending: BTreeMap::new(),
        }
    }

    pub fn place_event_id(&self) -> SpaceId {
        self.place_event_id
    }

    pub fn epoch(&self) -> u64 {
        self.log.epoch().get()
    }

    pub fn next_epoch(&self) -> Result<SpaceEpoch, PlaceMembershipError> {
        self.log
            .epoch()
            .advance()
            .map_err(|_| PlaceMembershipError::EpochExhausted)
    }

    pub fn roles(&self) -> &RoleTable {
        &self.roles
    }

    pub fn holders(&self) -> &RoleHolders {
        &self.holders
    }

    pub fn holders_mut(&mut self) -> &mut RoleHolders {
        &mut self.holders
    }

    pub fn member(&self, member_id: MemberId) -> Option<&MemberRecord> {
        self.members.get(&member_id)
    }

    pub fn contains(&self, member_id: MemberId) -> bool {
        self.members.contains_key(&member_id)
    }

    /// The exact roster, as member handles, in a stable order.
    pub fn roster_handles(&self) -> Vec<String> {
        let mut handles: Vec<String> = self
            .members
            .values()
            .map(|record| record.handle.clone())
            .collect();
        handles.sort();
        handles
    }

    pub fn roster_ids(&self) -> BTreeSet<MemberId> {
        self.members.keys().copied().collect()
    }

    pub fn persisted_events(&self) -> &[PersistedEvent] {
        &self.applied
    }

    /// The pending record a client would seed a new member's roster entry from.
    ///
    /// A join/invite has to carry the new member's identity key alongside the
    /// event; the event itself only names a subject key, and the roster needs
    /// the handle and display name too.
    pub fn stage_member(&mut self, record: MemberRecord) {
        self.pending.insert(record.member_id, record);
    }

    /// Applies one signed event.
    ///
    /// The actor is resolved from this client's own roster (or, for a
    /// create/join that introduces a member, from the staged records) by the
    /// member id, which is the digest of the identity key. The event never
    /// nominates the key it is checked against.
    pub fn apply(&mut self, wire: &WireEvent) -> Result<AppliedEvent, PlaceMembershipError> {
        if wire.place_event_id != self.place_event_id.as_bytes() {
            return Err(PlaceMembershipError::WrongPlace);
        }
        let event = wire.to_signed()?;
        let kind = kind_from_code(&wire.kind)?;

        // Whose key verifies this event? For a self-event it is the subject —
        // but resolved through the roster, so a substituted key is a member id
        // this client does not know.
        let subject_id = {
            let mut bytes = [0_u8; ed25519::PUBLIC_KEY_SIZE];
            if wire.subject.len() != bytes.len() {
                return Err(PlaceMembershipError::MalformedEvent);
            }
            bytes.copy_from_slice(&wire.subject);
            MemberId::from_identity_key(&ed25519::PublicKey::from_bytes(bytes))
        };

        let actor_public = match kind {
            MembershipEventKind::Create | MembershipEventKind::Join => self
                .pending
                .get(&subject_id)
                .or_else(|| self.members.get(&subject_id))
                .ok_or(PlaceMembershipError::UnknownActor)?
                .identity_public()?,
            MembershipEventKind::Leave => self
                .members
                .get(&subject_id)
                .ok_or(PlaceMembershipError::SubjectNotInRoster)?
                .identity_public()?,
            _ => return Err(PlaceMembershipError::UnsupportedEventKind),
        };

        let applied = self.log.apply(event, &actor_public)?;
        if applied == MembershipEventApply::Duplicate {
            return Ok(AppliedEvent::Duplicate);
        }

        match kind {
            MembershipEventKind::Create | MembershipEventKind::Join => {
                let record = self
                    .pending
                    .remove(&subject_id)
                    .ok_or(PlaceMembershipError::UnknownActor)?;
                self.members.insert(subject_id, record);
            }
            MembershipEventKind::Leave => {
                // Exactly one member leaves. Nothing else in the roster is
                // touched, and the holders table loses only their rows.
                self.members.remove(&subject_id);
                self.holders.remove(&subject_id);
            }
            _ => return Err(PlaceMembershipError::UnsupportedEventKind),
        }

        self.applied.push(PersistedEvent {
            epoch: wire.epoch,
            kind: wire.kind.clone(),
            subject: wire.subject.clone(),
            signature: wire.signature.clone(),
        });
        Ok(AppliedEvent::Applied)
    }

    /// Replays a persisted ladder from scratch, re-verifying every signature.
    ///
    /// This is what a restart does: the roster is *derived* from the signed
    /// events again, so a client that had been handed a doctored roster
    /// snapshot cannot carry it across a restart.
    pub fn replay(
        place_event_id: SpaceId,
        roles: RoleTable,
        holders: RoleHolders,
        directory: &[MemberRecord],
        events: &[PersistedEvent],
    ) -> Result<Self, PlaceMembershipError> {
        let mut membership = Self::new(place_event_id, roles);
        for record in directory {
            membership.stage_member(record.clone());
        }
        for event in events {
            let wire = WireEvent {
                place_event_id: place_event_id.as_bytes().to_vec(),
                epoch: event.epoch,
                kind: event.kind.clone(),
                subject: event.subject.clone(),
                signature: event.signature.clone(),
            };
            membership.apply(&wire)?;
        }
        // Role holdings survive only for members the replayed ladder kept.
        membership.holders = holders
            .into_iter()
            .filter(|(member, _)| membership.members.contains_key(member))
            .collect();
        Ok(membership)
    }

    /// Role ids held by a member, for reporting.
    pub fn role_ids(&self, member: MemberId) -> Vec<RoleId> {
        let mut ids: Vec<RoleId> = self
            .holders
            .get(&member)
            .map(|set| set.iter().copied().collect())
            .unwrap_or_default();
        ids.sort();
        ids
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AppliedEvent {
    Applied,
    Duplicate,
}

#[derive(Debug, thiserror::Error)]
pub enum PlaceMembershipError {
    #[error("membership event is for a different place")]
    WrongPlace,
    #[error("membership event is malformed")]
    MalformedEvent,
    #[error("roster holds a malformed identity key")]
    MalformedRosterKey,
    #[error("membership event kind is unknown")]
    UnknownEventKind,
    #[error("this ladder does not carry that membership event kind")]
    UnsupportedEventKind,
    #[error("no roster entry resolves the actor for this event")]
    UnknownActor,
    #[error("self-removal names a subject this roster does not contain")]
    SubjectNotInRoster,
    #[error("place membership epoch is exhausted")]
    EpochExhausted,
    #[error("membership event refused: {0}")]
    Refused(#[from] MembershipEventError),
}
