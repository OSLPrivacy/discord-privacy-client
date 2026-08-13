//! The multi-client world the departure runs in.
//!
//! Every member here is a separate client with its own keys, its own sealed
//! state file and its own copy of the membership ladder. Nothing is shared
//! between them except the wire values one hands to another, so "the remaining
//! clients converge" is a statement about independent replicas rather than
//! about one object seen twice.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crypto::aead;
use crypto::sender_keys::PhysicalDeviceId;
use ipc::space_roster::{MembershipEventKind, SignedMembershipEvent, SpaceEpoch, SpaceId};
use serde::{Deserialize, Serialize};

use crate::authority::{
    evaluate_departure, grant_role, Authority, DepartureAuthority, RoleGrant, RoleHolders, RoleTable,
};
use crate::client::{
    file_digest, Client, DepartureNote, HistoryRecord, LivePlace, PlaceKind, PlaceListEntry,
};
use crate::enclave::{
    accept_channel_key, admit, distribute_channel_key, next_channel_key, open_channel_message,
    seal_channel_message, Admission, ChannelId, ChannelMemberList, EnclaveChannel, HeldChannelKey,
};
use crate::group::GroupSession;
use crate::ids::{MemberId, PlaceHandle, RoleId};
use crate::keys::MemberKeys;
use crate::membership::{MemberRecord, PlaceMembership, WireEvent};

// ---------------------------------------------------------------------------
// Fixture
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Fixture {
    pub task: u32,
    pub leaver: String,
    pub members: Vec<FixtureMember>,
    pub places: Vec<FixturePlace>,
    pub steps: Vec<FixtureStep>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct FixtureMember {
    pub handle: String,
    pub name: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct FixturePlace {
    pub handle: String,
    pub kind: PlaceKind,
    pub name: String,
    pub roles: Vec<FixtureRole>,
    pub members: Vec<FixturePlaceMember>,
    #[serde(default)]
    pub channels: Vec<FixtureChannel>,
    #[serde(default)]
    pub seed: Vec<FixtureMessage>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct FixtureRole {
    /// The stable key the role id is derived from. Never shown to anyone.
    pub key: String,
    /// Display text. The gate does not read it; the fixture ships one role
    /// labelled "Owner" with no authority on purpose.
    pub label: String,
    pub authorities: Vec<Authority>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct FixturePlaceMember {
    pub handle: String,
    pub roles: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct FixtureChannel {
    pub handle: String,
    pub name: String,
    /// `"open"`, or a list of member handles.
    pub members: FixtureChannelMembers,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum FixtureChannelMembers {
    Open(String),
    Limited(Vec<String>),
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct FixtureMessage {
    pub sender: String,
    pub mark: String,
    pub body: String,
    #[serde(default)]
    pub channel: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum FixtureStep {
    /// One departure attempt, driven by an activation of the rendered menu.
    Leave {
        step: String,
        place: String,
        delete_local_history: bool,
    },
    /// Transfer of a role id that carries the required authority.
    TransferAuthority {
        step: String,
        place: String,
        role: String,
        to: String,
    },
    /// Emit the leaver's sidebar model at this point in the sequence.
    Model { step: String, point: String },
}

// ---------------------------------------------------------------------------
// The sidebar model handed to the interface
// ---------------------------------------------------------------------------

/// One place row as the interface renders it.
///
/// Member handles, never member ids: the model has to be identical across two
/// independent runs with independent keys, so the check can hold the rendered
/// menu and the live engine state to the same value.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PlaceMenuModel {
    pub handle: String,
    pub kind: PlaceKind,
    pub name: String,
    pub leave_action: String,
    pub leave_enabled: bool,
    pub refusal_code: Option<String>,
    pub authority_verdict: String,
    pub held_authority_role_labels: Vec<String>,
    pub other_authority_holders: Vec<String>,
    pub retained_messages: usize,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SidebarModel {
    pub point: String,
    pub leaver: String,
    pub places: Vec<PlaceMenuModel>,
}

/// The leaver's post-departure view: what is gone from the list and what is
/// honestly still on the device.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DepartedPlaceModel {
    pub handle: String,
    pub name: String,
    pub kind: PlaceKind,
    pub in_place_list: bool,
    pub retained_messages: usize,
    pub deletion_choice_applied: bool,
    pub left_at_epoch: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct LeaverViewModel {
    pub leaver: String,
    pub place_list: Vec<String>,
    pub departed: Vec<DepartedPlaceModel>,
}

// ---------------------------------------------------------------------------
// Departure request + outcome
// ---------------------------------------------------------------------------

/// A departure request as it arrives from an activated menu item.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MenuActivation {
    pub step: String,
    pub place: String,
    pub menu_action: String,
    pub actor: String,
    pub delete_local_history: bool,
    /// What the rendered item said about itself. Recorded so the report can be
    /// compared against the live gate rather than trusted.
    pub rendered_enabled: bool,
    pub rendered_label: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum DepartureOutcome {
    Refused {
        code: String,
        detail: String,
    },
    Completed {
        epoch_before: u64,
        epoch_after: u64,
    },
}

// ---------------------------------------------------------------------------
// Report rows
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RosterView {
    pub client: String,
    pub roster: Vec<String>,
    pub epoch: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RekeyRow {
    pub channel: String,
    pub leaver_was_reader: bool,
    pub rekeyed: bool,
    pub key_id_before: String,
    pub key_id_after: String,
    pub key_epoch_before: u64,
    pub key_epoch_after: u64,
    pub wrapped_for: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RotationRow {
    pub place: String,
    pub client: String,
    pub chain_id_before: Option<u32>,
    pub chain_id_after: u32,
    pub distributed_to: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DepartureReport {
    pub step: String,
    pub place: String,
    pub kind: PlaceKind,
    pub activation: MenuActivation,
    pub model_at_attempt: PlaceMenuModel,
    pub outcome: DepartureOutcome,
    pub roster_before: Vec<String>,
    pub roster_after: Vec<String>,
    pub removed: Vec<String>,
    pub other_members_changed: Vec<String>,
    pub signed_leave_event: Option<WireEvent>,
    pub rotations: Vec<RotationRow>,
    pub rekeys: Vec<RekeyRow>,
    pub keys_wrapped_for_leaver: usize,
    pub leaver_held_keys_digest_before: String,
    pub leaver_held_keys_digest_after: String,
    pub leaver_place_list_after: Vec<String>,
    pub leaver_retained_messages: usize,
    pub deletion_choice_applied: bool,
    pub state_bytes_before: Vec<(String, usize, String)>,
    pub state_bytes_after: Vec<(String, usize, String)>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DeliveryRow {
    pub client: String,
    pub decrypted: bool,
    pub mark: Option<String>,
    pub error: Option<String>,
    pub admission: Option<Admission>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ContentRound {
    pub place: String,
    pub kind: PlaceKind,
    pub channel: Option<String>,
    pub sender: String,
    pub mark: String,
    pub deliveries: Vec<DeliveryRow>,
}

// ---------------------------------------------------------------------------
// World
// ---------------------------------------------------------------------------

pub struct PlaceDefinition {
    pub handle: PlaceHandle,
    pub kind: PlaceKind,
    pub name: String,
    pub place_event_id: SpaceId,
    pub group_id: Vec<u8>,
    pub roles: RoleTable,
    pub role_keys: BTreeMap<String, RoleId>,
    pub channels: Vec<EnclaveChannel>,
    pub member_handles: Vec<String>,
}

pub struct World {
    pub directory: PathBuf,
    pub fixture: Fixture,
    pub members: BTreeMap<String, MemberKeys>,
    pub clients: BTreeMap<String, Client>,
    pub places: BTreeMap<String, PlaceDefinition>,
}

impl World {
    pub fn build(fixture: Fixture, directory: &Path) -> Result<Self, WorldError> {
        std::fs::create_dir_all(directory)?;
        let mut members = BTreeMap::new();
        let mut clients = BTreeMap::new();
        for member in &fixture.members {
            let keys = MemberKeys::generate(&member.handle, &member.name);
            let mut at_rest = [0_u8; aead::KEY_SIZE];
            at_rest.copy_from_slice(&crypto::random::random_bytes(aead::KEY_SIZE));
            clients.insert(
                member.handle.clone(),
                Client::new(
                    keys.member_id,
                    &member.handle,
                    PhysicalDeviceId::random(),
                    directory.join(format!("{}.osl", member.handle)),
                    aead::Key::from_bytes(at_rest),
                ),
            );
            members.insert(member.handle.clone(), keys);
        }

        let mut world = Self {
            directory: directory.to_path_buf(),
            fixture,
            members,
            clients,
            places: BTreeMap::new(),
        };
        let places = world.fixture.places.clone();
        for place in &places {
            world.install_place(place)?;
        }
        for place in &places {
            for message in &place.seed {
                world.broadcast(
                    &place.handle,
                    &message.sender,
                    message.channel.as_deref(),
                    &message.mark,
                    &message.body,
                )?;
            }
        }
        world.save_all()?;
        Ok(world)
    }

    fn install_place(&mut self, place: &FixturePlace) -> Result<(), WorldError> {
        let handle = PlaceHandle::new(place.handle.clone());
        let place_event_id = SpaceId::generate();
        let group_id = crypto::random::random_bytes(32);

        let mut roles = RoleTable::new();
        let mut role_keys = BTreeMap::new();
        for role in &place.roles {
            let id = RoleId::derive(&place.handle, &role.key);
            role_keys.insert(role.key.clone(), id);
            roles.insert(
                id,
                RoleGrant::new(
                    id,
                    role.label.clone(),
                    role.authorities.iter().copied().collect(),
                ),
            );
        }

        let mut holders = RoleHolders::new();
        let mut directory_records = Vec::new();
        for member in &place.members {
            let keys = self
                .members
                .get(&member.handle)
                .ok_or_else(|| WorldError::UnknownMember(member.handle.clone()))?;
            directory_records.push(MemberRecord {
                member_id: keys.member_id,
                handle: keys.handle.clone(),
                display_name: keys.display_name.clone(),
                identity_key: keys.identity_public.as_bytes().to_vec(),
            });
            let mut held = BTreeSet::new();
            for role_key in &member.roles {
                held.insert(
                    *role_keys
                        .get(role_key)
                        .ok_or_else(|| WorldError::UnknownRole(role_key.clone()))?,
                );
            }
            holders.insert(keys.member_id, held);
        }

        let mut channels = Vec::new();
        for channel in &place.channels {
            let mut id = [0_u8; ipc::space_roster::SpaceChannelId::LENGTH];
            let id_len = id.len();
            id.copy_from_slice(&crypto::random::random_bytes(id_len));
            let mut key_id = [0_u8; 16];
            key_id.copy_from_slice(&crypto::random::random_bytes(16));
            let member_list = match &channel.members {
                FixtureChannelMembers::Open(_) => ChannelMemberList::OpenToEnclave,
                FixtureChannelMembers::Limited(handles) => ChannelMemberList::Limited {
                    members: handles
                        .iter()
                        .map(|handle| {
                            self.members
                                .get(handle)
                                .map(|keys| keys.member_id)
                                .ok_or_else(|| WorldError::UnknownMember(handle.clone()))
                        })
                        .collect::<Result<BTreeSet<_>, _>>()?,
                },
            };
            channels.push(EnclaveChannel {
                channel_id: ChannelId::from_bytes(id),
                handle: channel.handle.clone(),
                name: channel.name.clone(),
                member_list,
                key_epoch: 1,
                key_id,
            });
        }

        // Every member's client gets its own independent ladder.
        for member in &place.members {
            let client = self
                .clients
                .get_mut(&member.handle)
                .ok_or_else(|| WorldError::UnknownMember(member.handle.clone()))?;
            let mut membership = PlaceMembership::new(place_event_id, roles.clone());
            for record in &directory_records {
                membership.stage_member(record.clone());
            }
            *membership.holders_mut() = holders.clone();
            client.place_list.push(PlaceListEntry {
                handle: handle.clone(),
                kind: place.kind,
                name: place.name.clone(),
            });
            client.places.insert(
                handle.clone(),
                LivePlace {
                    handle: handle.clone(),
                    kind: place.kind,
                    name: place.name.clone(),
                    group_id: group_id.clone(),
                    membership,
                    channels: channels.clone(),
                    held_channel_keys: BTreeMap::new(),
                    group_session: match place.kind {
                        PlaceKind::Group => {
                            Some(GroupSession::new(group_id.clone(), keys_device(client)))
                        }
                        PlaceKind::Enclave => None,
                    },
                    directory: directory_records.clone(),
                },
            );
        }

        // Create, then one join per further member — each self-signed, each
        // applied by every client in the same order.
        for (index, member) in place.members.iter().enumerate() {
            let keys = &self.members[&member.handle];
            let epoch = SpaceEpoch::from_persisted(index as u64 + 1);
            let kind = if index == 0 {
                MembershipEventKind::Create
            } else {
                MembershipEventKind::Join
            };
            let event = SignedMembershipEvent::sign(
                place_event_id,
                epoch,
                kind,
                keys.identity_public,
                &keys.identity_secret,
            );
            let wire = WireEvent::from_signed(&event);
            for other in &place.members {
                let client = self.clients.get_mut(&other.handle).expect("member client");
                let live = client.places.get_mut(&handle).expect("live place");
                live.membership.apply(&wire)?;
            }
        }

        self.places.insert(
            place.handle.clone(),
            PlaceDefinition {
                handle: handle.clone(),
                kind: place.kind,
                name: place.name.clone(),
                place_event_id,
                group_id: group_id.clone(),
                roles,
                role_keys,
                channels,
                member_handles: place.members.iter().map(|m| m.handle.clone()).collect(),
            },
        );

        match place.kind {
            PlaceKind::Group => {
                self.redistribute_group(&place.handle)?;
            }
            PlaceKind::Enclave => {
                let channel_handles: Vec<String> = self.places[&place.handle]
                    .channels
                    .iter()
                    .map(|channel| channel.handle.clone())
                    .collect();
                for channel in channel_handles {
                    self.rekey_channel(&place.handle, &channel, false, false)?;
                }
            }
        }
        Ok(())
    }

    /// Every member of a group place installs (or keeps) a sender chain and
    /// distributes it to the roster they currently hold.
    fn redistribute_group(&mut self, place: &str) -> Result<Vec<RotationRow>, WorldError> {
        let holders: Vec<String> = self.place_clients(place);
        let mut rows = Vec::new();
        for sender_handle in holders {
            let row = self.distribute_from(place, &sender_handle, false)?;
            rows.push(row);
        }
        Ok(rows)
    }

    /// One member's distribution. `rotate` forces a fresh rotation root first —
    /// that is the group half of a departure.
    fn distribute_from(
        &mut self,
        place: &str,
        sender_handle: &str,
        rotate: bool,
    ) -> Result<RotationRow, WorldError> {
        let handle = PlaceHandle::new(place.to_string());
        let World {
            members, clients, ..
        } = self;

        let sender_id = members[sender_handle].member_id;
        let roster: BTreeSet<MemberId> = {
            let client = clients.get(sender_handle).expect("sender client");
            let live = client.places.get(&handle).expect("sender place");
            live.roster()
        };
        let recipients: Vec<(MemberId, &crypto::ml_kem_768::EncapsulationKey)> = members
            .values()
            .filter(|keys| roster.contains(&keys.member_id) && keys.member_id != sender_id)
            .map(|keys| (keys.member_id, &keys.kem_public))
            .collect();

        let (chain_id_before, chain_id_after, distributions) = {
            let client = clients.get_mut(sender_handle).expect("sender client");
            let live = client.places.get_mut(&handle).expect("sender place");
            let session = live
                .group_session
                .as_mut()
                .ok_or(WorldError::NotAGroupPlace)?;
            let before = session.current_chain_id();
            if rotate {
                session.rotate_sender_chain()?;
            } else {
                session.ensure_sender_chain()?;
            }
            let after = session.current_chain_id().expect("chain installed");
            let distributions = session.distribute(sender_id, &recipients)?;
            (before, after, distributions)
        };

        let mut distributed_to = Vec::new();
        for distribution in &distributions {
            let recipient_handle = members
                .values()
                .find(|keys| keys.member_id == distribution.wrap.recipient)
                .map(|keys| keys.handle.clone())
                .ok_or(WorldError::UnknownRecipient)?;
            distributed_to.push(recipient_handle.clone());
            let kem_secret = &members[&recipient_handle].kem_secret;
            let client = clients
                .get_mut(&recipient_handle)
                .expect("recipient client");
            if let Some(live) = client.places.get_mut(&handle) {
                let session = live
                    .group_session
                    .as_mut()
                    .ok_or(WorldError::NotAGroupPlace)?;
                session.accept(distribution, kem_secret)?;
            }
        }
        distributed_to.sort();

        Ok(RotationRow {
            place: place.to_string(),
            client: sender_handle.to_string(),
            chain_id_before,
            chain_id_after,
            distributed_to,
        })
    }

    /// Generates a fresh content key for one channel and wraps it to the
    /// channel's current readers only.
    fn rekey_channel(
        &mut self,
        place: &str,
        channel_handle: &str,
        leaver_was_reader: bool,
        rekeyed: bool,
    ) -> Result<RekeyRow, WorldError> {
        let handle = PlaceHandle::new(place.to_string());
        // Readers are computed against the roster any remaining client holds.
        let roster = self.roster_of(place)?;

        // The definition is the shared truth for channel identity; every client
        // updates its own copy from the distribution it receives.
        let definition = self
            .places
            .get_mut(place)
            .ok_or_else(|| WorldError::UnknownPlace(place.to_string()))?;
        let channel = definition
            .channels
            .iter_mut()
            .find(|channel| channel.handle == channel_handle)
            .ok_or_else(|| WorldError::UnknownChannel(channel_handle.to_string()))?;
        let key_id_before = hex::encode(channel.key_id);
        let key_epoch_before = channel.key_epoch;

        let (key_epoch, key_id, key) = next_channel_key(channel);
        channel.key_epoch = key_epoch;
        channel.key_id = key_id;
        let channel_snapshot = channel.clone();
        let readers = channel_snapshot.readers(&roster);

        let World {
            members, clients, ..
        } = self;
        let recipients: Vec<(MemberId, &crypto::ml_kem_768::EncapsulationKey)> = members
            .values()
            .filter(|keys| readers.contains(&keys.member_id))
            .map(|keys| (keys.member_id, &keys.kem_public))
            .collect();
        let distributions = distribute_channel_key(&channel_snapshot, &key, &recipients)?;

        let mut wrapped_for = Vec::new();
        for distribution in &distributions {
            let recipient_handle = members
                .values()
                .find(|keys| keys.member_id == distribution.wrap.recipient)
                .map(|keys| keys.handle.clone())
                .ok_or(WorldError::UnknownRecipient)?;
            wrapped_for.push(recipient_handle.clone());
            let kem_secret = &members[&recipient_handle].kem_secret;
            let client = clients
                .get_mut(&recipient_handle)
                .expect("recipient client");
            if let Some(live) = client.places.get_mut(&handle) {
                let held = accept_channel_key(distribution, &channel_snapshot, kem_secret)?;
                if let Some(local) = live.channel_mut(channel_handle) {
                    local.key_epoch = channel_snapshot.key_epoch;
                    local.key_id = channel_snapshot.key_id;
                }
                live.held_channel_keys
                    .insert(channel_handle.to_string(), held);
            }
        }
        wrapped_for.sort();

        Ok(RekeyRow {
            channel: channel_handle.to_string(),
            leaver_was_reader,
            rekeyed,
            key_id_before,
            key_id_after: hex::encode(key_id),
            key_epoch_before,
            key_epoch_after: key_epoch,
            wrapped_for,
        })
    }

    fn roster_of(&self, place: &str) -> Result<BTreeSet<MemberId>, WorldError> {
        let handle = PlaceHandle::new(place.to_string());
        for client in self.clients.values() {
            if let Some(live) = client.places.get(&handle) {
                if live.membership.contains(client.member) {
                    return Ok(live.roster());
                }
            }
        }
        Err(WorldError::UnknownPlace(place.to_string()))
    }

    /// Handles of the clients that still hold this place as members.
    fn place_clients(&self, place: &str) -> Vec<String> {
        let handle = PlaceHandle::new(place.to_string());
        let mut out: Vec<String> = self
            .clients
            .values()
            .filter(|client| {
                client
                    .places
                    .get(&handle)
                    .is_some_and(|live| live.membership.contains(client.member))
            })
            .map(|client| client.handle.clone())
            .collect();
        out.sort();
        out
    }

    pub fn save_all(&self) -> Result<(), WorldError> {
        for client in self.clients.values() {
            client.save()?;
        }
        Ok(())
    }

    pub fn state_bytes(&self) -> Result<Vec<(String, usize, String)>, WorldError> {
        let mut out = Vec::new();
        for client in self.clients.values() {
            let (length, digest) = file_digest(&client.path)?;
            out.push((client.handle.clone(), length, digest));
        }
        out.sort();
        Ok(out)
    }

    /// Drops every in-memory client and rebuilds it from its sealed file.
    pub fn restart(&mut self) -> Result<Vec<RosterView>, WorldError> {
        let mut rebuilt = BTreeMap::new();
        for (handle, client) in std::mem::take(&mut self.clients) {
            let path = client.path.clone();
            let key = client.key.clone();
            drop(client);
            rebuilt.insert(handle, Client::restart(path, key)?);
        }
        self.clients = rebuilt;
        Ok(self.roster_views())
    }

    pub fn roster_views(&self) -> Vec<RosterView> {
        let mut views = Vec::new();
        for client in self.clients.values() {
            for live in client.places.values() {
                if !live.membership.contains(client.member) {
                    continue;
                }
                views.push(RosterView {
                    client: format!("{}@{}", client.handle, live.handle),
                    roster: live.membership.roster_handles(),
                    epoch: live.membership.epoch(),
                });
            }
        }
        views.sort_by(|left, right| left.client.cmp(&right.client));
        views
    }

    // -----------------------------------------------------------------
    // Content
    // -----------------------------------------------------------------

    /// Sends one marked message and records what every client made of it.
    pub fn broadcast(
        &mut self,
        place: &str,
        sender_handle: &str,
        channel_handle: Option<&str>,
        mark: &str,
        body: &str,
    ) -> Result<ContentRound, WorldError> {
        let kind = self
            .places
            .get(place)
            .ok_or_else(|| WorldError::UnknownPlace(place.to_string()))?
            .kind;
        match kind {
            PlaceKind::Group => self.broadcast_group(place, sender_handle, mark, body),
            PlaceKind::Enclave => {
                let channel = channel_handle.ok_or(WorldError::ChannelRequired)?;
                self.broadcast_channel(place, channel, sender_handle, mark, body)
            }
        }
    }

    fn broadcast_group(
        &mut self,
        place: &str,
        sender_handle: &str,
        mark: &str,
        body: &str,
    ) -> Result<ContentRound, WorldError> {
        let handle = PlaceHandle::new(place.to_string());
        let plaintext = format!("{mark}|{body}");
        let World {
            members, clients, ..
        } = self;
        let sender_keys = &members[sender_handle];
        let message = {
            let client = clients.get_mut(sender_handle).expect("sender client");
            let live = client.places.get_mut(&handle).expect("sender place");
            let session = live
                .group_session
                .as_mut()
                .ok_or(WorldError::NotAGroupPlace)?;
            session.send(sender_keys, plaintext.as_bytes())?
        };

        let mut deliveries = Vec::new();
        let recipient_handles: Vec<String> = clients.keys().cloned().collect();
        for recipient in recipient_handles {
            if recipient == sender_handle {
                continue;
            }
            let client = clients.get_mut(&recipient).expect("client");
            let member = client.member;
            let Some(live) = client.places.get_mut(&handle) else {
                continue;
            };
            let admission = if live.membership.contains(member) {
                Admission::Allowed
            } else {
                Admission::DeniedNotOnRoster
            };
            let session = live
                .group_session
                .as_mut()
                .ok_or(WorldError::NotAGroupPlace)?;
            let attempt = session.receive(
                &message,
                &sender_keys.x25519_public,
                &sender_keys.kem_public,
            );
            match attempt {
                Ok(bytes) => {
                    let text = String::from_utf8_lossy(&bytes).to_string();
                    client.history.push(HistoryRecord {
                        place: handle.clone(),
                        channel: None,
                        mark: mark.to_string(),
                        body: body.to_string(),
                    });
                    deliveries.push(DeliveryRow {
                        client: recipient,
                        decrypted: true,
                        mark: text.split('|').next().map(str::to_string),
                        error: None,
                        admission: Some(admission),
                    });
                }
                Err(error) => deliveries.push(DeliveryRow {
                    client: recipient,
                    decrypted: false,
                    mark: None,
                    error: Some(error.to_string()),
                    admission: Some(admission),
                }),
            }
        }

        // The sender keeps their own copy too.
        let client = clients.get_mut(sender_handle).expect("sender client");
        client.history.push(HistoryRecord {
            place: handle.clone(),
            channel: None,
            mark: mark.to_string(),
            body: body.to_string(),
        });
        deliveries.sort_by(|left, right| left.client.cmp(&right.client));

        Ok(ContentRound {
            place: place.to_string(),
            kind: PlaceKind::Group,
            channel: None,
            sender: sender_handle.to_string(),
            mark: mark.to_string(),
            deliveries,
        })
    }

    fn broadcast_channel(
        &mut self,
        place: &str,
        channel_handle: &str,
        sender_handle: &str,
        mark: &str,
        body: &str,
    ) -> Result<ContentRound, WorldError> {
        let handle = PlaceHandle::new(place.to_string());
        let plaintext = format!("{mark}|{body}");
        let channel = self
            .places
            .get(place)
            .ok_or_else(|| WorldError::UnknownPlace(place.to_string()))?
            .channels
            .iter()
            .find(|channel| channel.handle == channel_handle)
            .ok_or_else(|| WorldError::UnknownChannel(channel_handle.to_string()))?
            .clone();

        let sender_id = self.members[sender_handle].member_id;
        let message = {
            let client = self.clients.get(sender_handle).expect("sender client");
            let live = client.places.get(&handle).expect("sender place");
            let held = live
                .held_channel_keys
                .get(channel_handle)
                .ok_or(WorldError::NoChannelKey)?;
            seal_channel_message(&channel, sender_id, held, plaintext.as_bytes())?
        };

        let roster = self.roster_of(place)?;
        let mut deliveries = Vec::new();
        let recipient_handles: Vec<String> = self.clients.keys().cloned().collect();
        for recipient in recipient_handles {
            if recipient == sender_handle {
                continue;
            }
            let client = self.clients.get_mut(&recipient).expect("client");
            let member = client.member;
            let Some(live) = client.places.get_mut(&handle) else {
                continue;
            };
            let admission = admit(&roster, &channel, member);
            let held = live.held_channel_keys.get(channel_handle).cloned();
            match held {
                None => deliveries.push(DeliveryRow {
                    client: recipient,
                    decrypted: false,
                    mark: None,
                    error: Some("no channel key held".to_string()),
                    admission: Some(admission),
                }),
                Some(held) => match open_channel_message(&channel, &message, &held) {
                    Ok(bytes) => {
                        let text = String::from_utf8_lossy(&bytes).to_string();
                        client.history.push(HistoryRecord {
                            place: handle.clone(),
                            channel: Some(channel_handle.to_string()),
                            mark: mark.to_string(),
                            body: body.to_string(),
                        });
                        deliveries.push(DeliveryRow {
                            client: recipient,
                            decrypted: true,
                            mark: text.split('|').next().map(str::to_string),
                            error: None,
                            admission: Some(admission),
                        });
                    }
                    Err(error) => deliveries.push(DeliveryRow {
                        client: recipient,
                        decrypted: false,
                        mark: None,
                        error: Some(error.to_string()),
                        admission: Some(admission),
                    }),
                },
            }
        }

        let client = self.clients.get_mut(sender_handle).expect("sender client");
        client.history.push(HistoryRecord {
            place: handle.clone(),
            channel: Some(channel_handle.to_string()),
            mark: mark.to_string(),
            body: body.to_string(),
        });
        deliveries.sort_by(|left, right| left.client.cmp(&right.client));

        Ok(ContentRound {
            place: place.to_string(),
            kind: PlaceKind::Enclave,
            channel: Some(channel_handle.to_string()),
            sender: sender_handle.to_string(),
            mark: mark.to_string(),
            deliveries,
        })
    }

    // -----------------------------------------------------------------
    // Sidebar model
    // -----------------------------------------------------------------

    pub fn sidebar_model(&self, point: &str) -> Result<SidebarModel, WorldError> {
        let leaver = self.fixture.leaver.clone();
        let client = self
            .clients
            .get(&leaver)
            .ok_or_else(|| WorldError::UnknownMember(leaver.clone()))?;
        let mut places = Vec::new();
        for entry in &client.place_list {
            let live = client
                .places
                .get(&entry.handle)
                .ok_or_else(|| WorldError::UnknownPlace(entry.handle.to_string()))?;
            places.push(self.place_menu_model(client, live)?);
        }
        Ok(SidebarModel {
            point: point.to_string(),
            leaver,
            places,
        })
    }

    fn place_menu_model(
        &self,
        client: &Client,
        live: &LivePlace,
    ) -> Result<PlaceMenuModel, WorldError> {
        let verdict = evaluate_departure(
            live.membership.roles(),
            live.membership.holders(),
            client.member,
        );
        let (verdict_name, held_labels, others) = match &verdict {
            DepartureAuthority::Clear => ("clear", Vec::new(), Vec::new()),
            DepartureAuthority::Redundant {
                held_role_ids,
                other_holders,
            } => (
                "redundant",
                held_role_ids
                    .iter()
                    .map(|id| {
                        live.membership
                            .roles()
                            .get(id)
                            .map(|role| role.label.clone())
                            .unwrap_or_default()
                    })
                    .collect(),
                other_holders
                    .iter()
                    .map(|id| self.handle_of(*id))
                    .collect::<Result<Vec<_>, _>>()?,
            ),
            DepartureAuthority::LastHolder { held_role_ids } => (
                "last_holder",
                held_role_ids
                    .iter()
                    .map(|id| {
                        live.membership
                            .roles()
                            .get(id)
                            .map(|role| role.label.clone())
                            .unwrap_or_default()
                    })
                    .collect(),
                Vec::new(),
            ),
        };
        Ok(PlaceMenuModel {
            handle: live.handle.as_str().to_string(),
            kind: live.kind,
            name: live.name.clone(),
            leave_action: live.kind.leave_action().to_string(),
            leave_enabled: verdict.permits_departure(),
            refusal_code: verdict.refusal_code().map(str::to_string),
            authority_verdict: verdict_name.to_string(),
            held_authority_role_labels: held_labels,
            other_authority_holders: others,
            retained_messages: client.retained_history(&live.handle),
        })
    }

    fn handle_of(&self, member: MemberId) -> Result<String, WorldError> {
        self.members
            .values()
            .find(|keys| keys.member_id == member)
            .map(|keys| keys.handle.clone())
            .ok_or(WorldError::UnknownRecipient)
    }

    pub fn leaver_view(&self) -> Result<LeaverViewModel, WorldError> {
        let leaver = self.fixture.leaver.clone();
        let client = self
            .clients
            .get(&leaver)
            .ok_or_else(|| WorldError::UnknownMember(leaver.clone()))?;
        let mut departed = Vec::new();
        for note in &client.departures {
            departed.push(DepartedPlaceModel {
                handle: note.place.as_str().to_string(),
                name: note.place_name.clone(),
                kind: note.kind,
                in_place_list: client
                    .place_list
                    .iter()
                    .any(|entry| entry.handle == note.place),
                retained_messages: client.retained_history(&note.place),
                deletion_choice_applied: note.deletion_choice_applied,
                left_at_epoch: note.left_at_epoch,
            });
        }
        departed.sort_by(|left, right| left.handle.cmp(&right.handle));
        Ok(LeaverViewModel {
            leaver,
            place_list: client.place_handles(),
            departed,
        })
    }

    // -----------------------------------------------------------------
    // Departure
    // -----------------------------------------------------------------

    pub fn transfer_authority(
        &mut self,
        place: &str,
        role_key: &str,
        to: &str,
    ) -> Result<(), WorldError> {
        let handle = PlaceHandle::new(place.to_string());
        let role_id = *self
            .places
            .get(place)
            .ok_or_else(|| WorldError::UnknownPlace(place.to_string()))?
            .role_keys
            .get(role_key)
            .ok_or_else(|| WorldError::UnknownRole(role_key.to_string()))?;
        let recipient = self
            .members
            .get(to)
            .ok_or_else(|| WorldError::UnknownMember(to.to_string()))?
            .member_id;
        for client in self.clients.values_mut() {
            if let Some(live) = client.places.get_mut(&handle) {
                let roles = live.membership.roles().clone();
                grant_role(&roles, live.membership.holders_mut(), recipient, role_id)?;
            }
        }
        self.save_all()?;
        Ok(())
    }

    /// Runs one departure driven by an activation of the rendered menu.
    pub fn depart(&mut self, activation: &MenuActivation) -> Result<DepartureReport, WorldError> {
        let place_handle = PlaceHandle::new(activation.place.clone());
        let leaver_handle = activation.actor.clone();
        let leaver_id = self
            .members
            .get(&leaver_handle)
            .ok_or_else(|| WorldError::UnknownMember(leaver_handle.clone()))?
            .member_id;
        let kind = self
            .places
            .get(&activation.place)
            .ok_or_else(|| WorldError::UnknownPlace(activation.place.clone()))?
            .kind;

        let state_bytes_before = self.state_bytes()?;
        let roster_before = self.roster_of(&activation.place)?;
        let roster_before_handles = self.handles_of(&roster_before)?;
        let model_at_attempt = {
            let client = &self.clients[&leaver_handle];
            let live = client
                .places
                .get(&place_handle)
                .ok_or_else(|| WorldError::UnknownPlace(activation.place.clone()))?;
            self.place_menu_model(client, live)?
        };
        let leaver_keys_digest_before = self.leaver_key_digest(&leaver_handle, &place_handle);

        let refusal = self.refusal_for(activation, &leaver_handle, &place_handle)?;
        if let Some((code, detail)) = refusal {
            // Nothing is written. The bytes on disk are re-measured so the
            // refusal is evidenced rather than asserted.
            let state_bytes_after = self.state_bytes()?;
            return Ok(DepartureReport {
                step: activation.step.clone(),
                place: activation.place.clone(),
                kind,
                activation: activation.clone(),
                model_at_attempt,
                outcome: DepartureOutcome::Refused { code, detail },
                roster_before: roster_before_handles.clone(),
                roster_after: roster_before_handles,
                removed: Vec::new(),
                other_members_changed: Vec::new(),
                signed_leave_event: None,
                rotations: Vec::new(),
                rekeys: Vec::new(),
                keys_wrapped_for_leaver: 0,
                leaver_held_keys_digest_before: leaver_keys_digest_before.clone(),
                leaver_held_keys_digest_after: leaver_keys_digest_before,
                leaver_place_list_after: self.clients[&leaver_handle].place_handles(),
                leaver_retained_messages: self.clients[&leaver_handle]
                    .retained_history(&place_handle),
                deletion_choice_applied: false,
                state_bytes_before,
                state_bytes_after,
            });
        }

        // The leaver signs their own removal at the next epoch of the ladder
        // they hold. `MembershipEventLog` refuses anything but the successor.
        let (event_id, epoch_before, next) = {
            let client = &self.clients[&leaver_handle];
            let live = client.places.get(&place_handle).expect("live place");
            (
                live.membership.place_event_id(),
                live.membership.epoch(),
                live.membership.next_epoch()?,
            )
        };
        let leaver_keys = &self.members[&leaver_handle];
        let event = SignedMembershipEvent::sign(
            event_id,
            next,
            MembershipEventKind::Leave,
            leaver_keys.identity_public,
            &leaver_keys.identity_secret,
        );
        let wire = WireEvent::from_signed(&event);

        // Every client that holds the place applies the same signed event.
        for client in self.clients.values_mut() {
            if let Some(live) = client.places.get_mut(&place_handle) {
                live.membership.apply(&wire)?;
            }
        }

        let roster_after = self.roster_of(&activation.place).unwrap_or_default();
        let roster_after_handles = self.handles_of(&roster_after)?;
        let removed = self.handles_of(
            &roster_before
                .difference(&roster_after)
                .copied()
                .collect::<BTreeSet<_>>(),
        )?;
        let added = self.handles_of(
            &roster_after
                .difference(&roster_before)
                .copied()
                .collect::<BTreeSet<_>>(),
        )?;

        // Key work: rotate the group sender state, or re-key every channel the
        // leaver could read.
        let mut rotations = Vec::new();
        let mut rekeys = Vec::new();
        match kind {
            PlaceKind::Group => {
                for sender in self.place_clients(&activation.place) {
                    rotations.push(self.distribute_from(&activation.place, &sender, true)?);
                }
            }
            PlaceKind::Enclave => {
                let channels: Vec<(String, bool)> = self.places[&activation.place]
                    .channels
                    .iter()
                    .map(|channel| {
                        (
                            channel.handle.clone(),
                            channel.includes(&roster_before, leaver_id),
                        )
                    })
                    .collect();
                for (channel_handle, leaver_was_reader) in channels {
                    if leaver_was_reader {
                        rekeys.push(self.rekey_channel(
                            &activation.place,
                            &channel_handle,
                            true,
                            true,
                        )?);
                    } else {
                        let channel = self.places[&activation.place]
                            .channels
                            .iter()
                            .find(|channel| channel.handle == channel_handle)
                            .expect("channel");
                        rekeys.push(RekeyRow {
                            channel: channel_handle.clone(),
                            leaver_was_reader: false,
                            rekeyed: false,
                            key_id_before: hex::encode(channel.key_id),
                            key_id_after: hex::encode(channel.key_id),
                            key_epoch_before: channel.key_epoch,
                            key_epoch_after: channel.key_epoch,
                            wrapped_for: Vec::new(),
                        });
                    }
                }
            }
        }

        let keys_wrapped_for_leaver = rotations
            .iter()
            .filter(|row| row.distributed_to.contains(&leaver_handle))
            .count()
            + rekeys
                .iter()
                .filter(|row| row.wrapped_for.contains(&leaver_handle))
                .count();

        // The leaver's own client: the place leaves the list, the local copy
        // stays unless the separate deletion choice was taken.
        let leaver_retained_messages;
        {
            let client = self
                .clients
                .get_mut(&leaver_handle)
                .expect("leaver client exists");
            client
                .place_list
                .retain(|entry| entry.handle != place_handle);
            if activation.delete_local_history {
                client.history.retain(|record| record.place != place_handle);
            }
            leaver_retained_messages = client.retained_history(&place_handle);
            let place_name = client
                .places
                .get(&place_handle)
                .map(|live| live.name.clone())
                .unwrap_or_default();
            let left_at_epoch = client
                .places
                .get(&place_handle)
                .map(|live| live.membership.epoch())
                .unwrap_or_default();
            client.departures.push(DepartureNote {
                place: place_handle.clone(),
                place_name,
                kind,
                left_at_epoch,
                retained_messages: leaver_retained_messages,
                deletion_choice_applied: activation.delete_local_history,
            });
        }

        self.save_all()?;
        let state_bytes_after = self.state_bytes()?;
        let leaver_keys_digest_after = self.leaver_key_digest(&leaver_handle, &place_handle);

        Ok(DepartureReport {
            step: activation.step.clone(),
            place: activation.place.clone(),
            kind,
            activation: activation.clone(),
            model_at_attempt,
            outcome: DepartureOutcome::Completed {
                epoch_before,
                epoch_after: self
                    .clients
                    .values()
                    .find_map(|client| {
                        client
                            .places
                            .get(&place_handle)
                            .filter(|live| live.membership.contains(client.member))
                            .map(|live| live.membership.epoch())
                    })
                    .unwrap_or_default(),
            },
            roster_before: roster_before_handles,
            roster_after: roster_after_handles,
            removed,
            other_members_changed: added,
            signed_leave_event: Some(wire),
            rotations,
            rekeys,
            keys_wrapped_for_leaver,
            leaver_held_keys_digest_before: leaver_keys_digest_before,
            leaver_held_keys_digest_after: leaver_keys_digest_after,
            leaver_place_list_after: self.clients[&leaver_handle].place_handles(),
            leaver_retained_messages,
            deletion_choice_applied: activation.delete_local_history,
            state_bytes_before,
            state_bytes_after,
        })
    }

    /// Every reason a departure is refused, checked before anything is written.
    fn refusal_for(
        &self,
        activation: &MenuActivation,
        leaver_handle: &str,
        place_handle: &PlaceHandle,
    ) -> Result<Option<(String, String)>, WorldError> {
        let client = self
            .clients
            .get(leaver_handle)
            .ok_or_else(|| WorldError::UnknownMember(leaver_handle.to_string()))?;
        if !client
            .place_list
            .iter()
            .any(|entry| entry.handle == *place_handle)
        {
            return Ok(Some((
                "not-in-place-list".to_string(),
                format!("{leaver_handle} does not hold {place_handle} in their place list"),
            )));
        }
        let live = client
            .places
            .get(place_handle)
            .ok_or_else(|| WorldError::UnknownPlace(place_handle.to_string()))?;
        if activation.menu_action != live.kind.leave_action() {
            return Ok(Some((
                "menu-action-mismatch".to_string(),
                format!(
                    "menu action {} cannot leave a {:?} place",
                    activation.menu_action, live.kind
                ),
            )));
        }
        if !live.membership.contains(client.member) {
            return Ok(Some((
                "not-a-member".to_string(),
                format!("{leaver_handle} is not on the roster of {place_handle}"),
            )));
        }
        let verdict = evaluate_departure(
            live.membership.roles(),
            live.membership.holders(),
            client.member,
        );
        if let Some(code) = verdict.refusal_code() {
            let labels: Vec<String> = match &verdict {
                DepartureAuthority::LastHolder { held_role_ids } => held_role_ids
                    .iter()
                    .map(|id| id.hex())
                    .collect(),
                _ => Vec::new(),
            };
            return Ok(Some((
                code.to_string(),
                format!(
                    "{leaver_handle} is the last holder of role id(s) {} carrying the authority \
                     {place_handle} needs; transfer it before leaving",
                    labels.join(",")
                ),
            )));
        }
        Ok(None)
    }

    fn handles_of(&self, members: &BTreeSet<MemberId>) -> Result<Vec<String>, WorldError> {
        let mut out = Vec::new();
        for member in members {
            out.push(self.handle_of(*member)?);
        }
        out.sort();
        Ok(out)
    }

    /// Public form of the post-restart redistribution: every current member of
    /// a group place installs a fresh session chain and hands it to the roster
    /// they hold now. A departed member is not on that roster.
    pub fn redistribute_group_public(
        &mut self,
        place: &str,
    ) -> Result<Vec<RotationRow>, WorldError> {
        self.redistribute_group(place)
    }

    /// Members still on the roster of a place, in a stable order.
    pub fn remaining_members(&self, place: &str) -> Vec<String> {
        self.place_clients(place)
    }

    /// A member who still reads one channel, if there is one.
    pub fn remaining_channel_member(&self, place: &str, channel_handle: &str) -> Option<String> {
        let roster = self.roster_of(place).ok()?;
        let channel = self
            .places
            .get(place)?
            .channels
            .iter()
            .find(|channel| channel.handle == channel_handle)?;
        let readers = channel.readers(&roster);
        let mut handles: Vec<String> = self
            .members
            .values()
            .filter(|keys| readers.contains(&keys.member_id))
            .map(|keys| keys.handle.clone())
            .collect();
        handles.sort();
        handles.into_iter().next()
    }

    /// What the leaver's device still holds for each place it left.
    pub fn leaver_key_probes(&self) -> Result<Vec<crate::run::LeaverKeyProbe>, WorldError> {
        let leaver = &self.fixture.leaver;
        let client = self
            .clients
            .get(leaver)
            .ok_or_else(|| WorldError::UnknownMember(leaver.clone()))?;
        let mut probes = Vec::new();
        for live in client.places.values() {
            probes.push(crate::run::LeaverKeyProbe {
                place: live.handle.as_str().to_string(),
                held_sender_keys: live
                    .group_session
                    .as_ref()
                    .map(|session| session.held_keys().len())
                    .unwrap_or(0),
                held_channel_keys: live.held_channel_keys.len(),
                in_place_list: client
                    .place_list
                    .iter()
                    .any(|entry| entry.handle == live.handle),
            });
        }
        probes.sort_by(|left, right| left.place.cmp(&right.place));
        Ok(probes)
    }

    fn leaver_key_digest(&self, leaver_handle: &str, place: &PlaceHandle) -> String {
        use sha2::{Digest, Sha256};
        let Some(client) = self.clients.get(leaver_handle) else {
            return String::new();
        };
        let Some(live) = client.places.get(place) else {
            return String::new();
        };
        let mut hasher = Sha256::new();
        hasher.update(b"OSL/place-departure/held-keys/v1");
        if let Some(session) = &live.group_session {
            hasher.update(
                serde_json::to_vec(&session.held_keys())
                    .unwrap_or_default()
                    .as_slice(),
            );
        }
        let mut channel_keys: Vec<&HeldChannelKey> = live.held_channel_keys.values().collect();
        channel_keys.sort_by(|left, right| left.channel_handle.cmp(&right.channel_handle));
        hasher.update(
            serde_json::to_vec(&channel_keys)
                .unwrap_or_default()
                .as_slice(),
        );
        hex::encode(hasher.finalize())
    }
}

fn keys_device(client: &Client) -> PhysicalDeviceId {
    client.device_id
}

#[derive(Debug, thiserror::Error)]
pub enum WorldError {
    #[error("unknown member {0}")]
    UnknownMember(String),
    #[error("unknown place {0}")]
    UnknownPlace(String),
    #[error("unknown role {0}")]
    UnknownRole(String),
    #[error("unknown channel {0}")]
    UnknownChannel(String),
    #[error("a distribution named a recipient no client knows")]
    UnknownRecipient,
    #[error("that place is not a group place")]
    NotAGroupPlace,
    #[error("an enclave broadcast needs a channel")]
    ChannelRequired,
    #[error("no channel key is held for that channel")]
    NoChannelKey,
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Client(#[from] crate::client::ClientError),
    #[error(transparent)]
    Membership(#[from] crate::membership::PlaceMembershipError),
    #[error(transparent)]
    Group(#[from] crate::group::GroupError),
    #[error(transparent)]
    Enclave(#[from] crate::enclave::EnclaveError),
    #[error(transparent)]
    Authority(#[from] crate::authority::AuthorityError),
}
