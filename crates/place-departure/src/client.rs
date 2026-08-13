//! One client's durable state and its sealed at-rest file.
//!
//! Everything a departure has to survive lives here: the place list, the
//! signed membership ladder, the key material this device holds, and the local
//! historical copy. A restart is modelled honestly — the in-memory client is
//! dropped and rebuilt from the sealed file, and the roster is *replayed* from
//! the signed events rather than restored from a cached snapshot.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crypto::aead;
use crypto::sender_keys::{PhysicalDeviceId, PHYSICAL_DEVICE_ID_BYTES};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::authority::{RoleHolders, RoleTable};
use crate::enclave::{ChannelId, ChannelMemberList, EnclaveChannel, HeldChannelKey};
use crate::group::{GroupSession, HeldSenderKey};
use crate::ids::{MemberId, PlaceHandle};
use crate::membership::{MemberRecord, PersistedEvent, PlaceMembership, PlaceMembershipError};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PlaceKind {
    Group,
    Enclave,
}

impl PlaceKind {
    /// The one menu action that may leave this kind of place. A "leave group"
    /// activation aimed at an enclave is a mismatch, not a synonym.
    pub fn leave_action(self) -> &'static str {
        match self {
            Self::Group => "leave-group",
            Self::Enclave => "leave-enclave",
        }
    }
}

/// One row of the client's own place list.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PlaceListEntry {
    pub handle: PlaceHandle,
    pub kind: PlaceKind,
    pub name: String,
}

/// One locally held message. This is the copy that survives a departure.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct HistoryRecord {
    pub place: PlaceHandle,
    pub channel: Option<String>,
    pub mark: String,
    pub body: String,
}

/// What the client records about a place it left.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DepartureNote {
    pub place: PlaceHandle,
    pub place_name: String,
    pub kind: PlaceKind,
    pub left_at_epoch: u64,
    /// How many locally held messages are still on this device.
    pub retained_messages: usize,
    /// Whether the separate "delete my copy" choice was taken at the same time.
    pub deletion_choice_applied: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PersistedChannel {
    pub handle: String,
    pub name: String,
    #[serde(with = "crate::keys::hex_bytes")]
    pub channel_id: Vec<u8>,
    pub member_list: ChannelMemberList,
    pub key_epoch: u64,
    #[serde(with = "crate::keys::hex_bytes")]
    pub key_id: Vec<u8>,
}

impl PersistedChannel {
    fn from_channel(channel: &EnclaveChannel) -> Self {
        Self {
            handle: channel.handle.clone(),
            name: channel.name.clone(),
            channel_id: channel.channel_id.as_bytes().to_vec(),
            member_list: channel.member_list.clone(),
            key_epoch: channel.key_epoch,
            key_id: channel.key_id.to_vec(),
        }
    }

    fn to_channel(&self) -> Result<EnclaveChannel, ClientError> {
        let mut channel_id = [0_u8; ipc::space_roster::SpaceChannelId::LENGTH];
        let mut key_id = [0_u8; 16];
        if self.channel_id.len() != channel_id.len() || self.key_id.len() != key_id.len() {
            return Err(ClientError::Malformed);
        }
        channel_id.copy_from_slice(&self.channel_id);
        key_id.copy_from_slice(&self.key_id);
        Ok(EnclaveChannel {
            channel_id: ChannelId::from_bytes(channel_id),
            handle: self.handle.clone(),
            name: self.name.clone(),
            member_list: self.member_list.clone(),
            key_epoch: self.key_epoch,
            key_id,
        })
    }
}

/// One place as this client persists it.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PersistedPlace {
    pub handle: PlaceHandle,
    pub kind: PlaceKind,
    pub name: String,
    #[serde(with = "crate::keys::hex_bytes")]
    pub place_event_id: Vec<u8>,
    #[serde(with = "crate::keys::hex_bytes")]
    pub group_id: Vec<u8>,
    pub roles: RoleTable,
    pub holders: RoleHolders,
    pub directory: Vec<MemberRecord>,
    pub events: Vec<PersistedEvent>,
    pub channels: Vec<PersistedChannel>,
    pub held_channel_keys: Vec<HeldChannelKey>,
    pub held_sender_keys: Vec<HeldSenderKey>,
}

/// The whole sealed file.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ClientFile {
    pub version: u32,
    pub member: MemberId,
    pub handle: String,
    #[serde(with = "crate::keys::hex_bytes")]
    pub device_id: Vec<u8>,
    pub place_list: Vec<PlaceListEntry>,
    pub places: Vec<PersistedPlace>,
    pub history: Vec<HistoryRecord>,
    pub departures: Vec<DepartureNote>,
}

const FILE_VERSION: u32 = 1;
const AT_REST_AD: &[u8] = b"OSL/place-departure/client-state/v1";

/// Seals `file` under `key` and writes it. There is no plaintext path: a
/// client state file always leaves this function encrypted.
pub fn write_client_file(
    path: &Path,
    key: &aead::Key,
    file: &ClientFile,
) -> Result<(), ClientError> {
    let body = serde_json::to_vec(file)?;
    let nonce = crypto::random::random_nonce();
    let sealed = aead::seal(key, &nonce, AT_REST_AD, &body)?;
    let mut bytes = Vec::with_capacity(aead::NONCE_SIZE + sealed.len());
    bytes.extend_from_slice(nonce.as_bytes());
    bytes.extend_from_slice(&sealed);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension("tmp");
    std::fs::write(&temporary, &bytes)?;
    std::fs::rename(&temporary, path)?;
    Ok(())
}

pub fn read_client_file(path: &Path, key: &aead::Key) -> Result<ClientFile, ClientError> {
    let bytes = std::fs::read(path)?;
    if bytes.len() < aead::NONCE_SIZE {
        return Err(ClientError::Malformed);
    }
    let mut nonce = [0_u8; aead::NONCE_SIZE];
    nonce.copy_from_slice(&bytes[..aead::NONCE_SIZE]);
    let plaintext = aead::open(
        key,
        &aead::Nonce::from_bytes(nonce),
        AT_REST_AD,
        &bytes[aead::NONCE_SIZE..],
    )?;
    Ok(serde_json::from_slice(&plaintext)?)
}

/// SHA-256 over the sealed file exactly as it sits on disk, plus its length.
/// This is the measurement behind "changed 0 bytes".
pub fn file_digest(path: &Path) -> Result<(usize, String), ClientError> {
    let bytes = std::fs::read(path)?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    Ok((bytes.len(), hex::encode(hasher.finalize())))
}

/// One place as the running client holds it.
pub struct LivePlace {
    pub handle: PlaceHandle,
    pub kind: PlaceKind,
    pub name: String,
    pub group_id: Vec<u8>,
    pub membership: PlaceMembership,
    pub channels: Vec<EnclaveChannel>,
    pub held_channel_keys: BTreeMap<String, HeldChannelKey>,
    pub group_session: Option<GroupSession>,
    pub directory: Vec<MemberRecord>,
}

impl LivePlace {
    pub fn roster(&self) -> BTreeSet<MemberId> {
        self.membership.roster_ids()
    }

    pub fn channel(&self, handle: &str) -> Option<&EnclaveChannel> {
        self.channels.iter().find(|channel| channel.handle == handle)
    }

    pub fn channel_mut(&mut self, handle: &str) -> Option<&mut EnclaveChannel> {
        self.channels
            .iter_mut()
            .find(|channel| channel.handle == handle)
    }
}

/// The running client.
pub struct Client {
    pub member: MemberId,
    pub handle: String,
    pub device_id: PhysicalDeviceId,
    pub path: PathBuf,
    pub key: aead::Key,
    pub place_list: Vec<PlaceListEntry>,
    pub places: BTreeMap<PlaceHandle, LivePlace>,
    pub history: Vec<HistoryRecord>,
    pub departures: Vec<DepartureNote>,
}

impl Client {
    pub fn new(
        member: MemberId,
        handle: &str,
        device_id: PhysicalDeviceId,
        path: PathBuf,
        key: aead::Key,
    ) -> Self {
        Self {
            member,
            handle: handle.to_string(),
            device_id,
            path,
            key,
            place_list: Vec::new(),
            places: BTreeMap::new(),
            history: Vec::new(),
            departures: Vec::new(),
        }
    }

    pub fn place_handles(&self) -> Vec<String> {
        let mut handles: Vec<String> = self
            .place_list
            .iter()
            .map(|entry| entry.handle.as_str().to_string())
            .collect();
        handles.sort();
        handles
    }

    pub fn retained_history(&self, place: &PlaceHandle) -> usize {
        self.history
            .iter()
            .filter(|record| record.place == *place)
            .count()
    }

    pub fn departure_note(&self, place: &PlaceHandle) -> Option<&DepartureNote> {
        self.departures.iter().find(|note| note.place == *place)
    }

    pub fn snapshot(&self) -> ClientFile {
        ClientFile {
            version: FILE_VERSION,
            member: self.member,
            handle: self.handle.clone(),
            device_id: self.device_id.as_bytes().to_vec(),
            place_list: self.place_list.clone(),
            places: self
                .places
                .values()
                .map(|place| PersistedPlace {
                    handle: place.handle.clone(),
                    kind: place.kind,
                    name: place.name.clone(),
                    place_event_id: place.membership.place_event_id().as_bytes().to_vec(),
                    group_id: place.group_id.clone(),
                    roles: place.membership.roles().clone(),
                    holders: place.membership.holders().clone(),
                    directory: place.directory.clone(),
                    events: place.membership.persisted_events().to_vec(),
                    channels: place
                        .channels
                        .iter()
                        .map(PersistedChannel::from_channel)
                        .collect(),
                    held_channel_keys: place.held_channel_keys.values().cloned().collect(),
                    held_sender_keys: place
                        .group_session
                        .as_ref()
                        .map(GroupSession::held_keys)
                        .unwrap_or_default(),
                })
                .collect(),
            history: self.history.clone(),
            departures: self.departures.clone(),
        }
    }

    pub fn save(&self) -> Result<(), ClientError> {
        write_client_file(&self.path, &self.key, &self.snapshot())
    }

    /// Rebuilds a client from its sealed file. The roster for every place is
    /// replayed from the signed events, so a restart cannot inherit a roster
    /// that no signed event supports.
    pub fn restart(path: PathBuf, key: aead::Key) -> Result<Self, ClientError> {
        let file = read_client_file(&path, &key)?;
        let mut device = [0_u8; PHYSICAL_DEVICE_ID_BYTES];
        if file.device_id.len() != device.len() {
            return Err(ClientError::Malformed);
        }
        device.copy_from_slice(&file.device_id);
        let device_id = PhysicalDeviceId::from_bytes(device)?;

        let mut client = Self::new(file.member, &file.handle, device_id, path, key);
        client.place_list = file.place_list;
        client.history = file.history;
        client.departures = file.departures;

        for persisted in file.places {
            let mut place_event_id = [0_u8; ipc::space_roster::SpaceId::LENGTH];
            if persisted.place_event_id.len() != place_event_id.len() {
                return Err(ClientError::Malformed);
            }
            place_event_id.copy_from_slice(&persisted.place_event_id);
            let membership = PlaceMembership::replay(
                ipc::space_roster::SpaceId::from_persisted(place_event_id),
                persisted.roles.clone(),
                persisted.holders.clone(),
                &persisted.directory,
                &persisted.events,
            )?;
            let channels = persisted
                .channels
                .iter()
                .map(PersistedChannel::to_channel)
                .collect::<Result<Vec<_>, _>>()?;
            let group_session = match persisted.kind {
                PlaceKind::Group => Some(GroupSession::restore(
                    persisted.group_id.clone(),
                    device_id,
                    persisted.held_sender_keys.clone(),
                )?),
                PlaceKind::Enclave => None,
            };
            client.places.insert(
                persisted.handle.clone(),
                LivePlace {
                    handle: persisted.handle,
                    kind: persisted.kind,
                    name: persisted.name,
                    group_id: persisted.group_id,
                    membership,
                    channels,
                    held_channel_keys: persisted
                        .held_channel_keys
                        .into_iter()
                        .map(|held| (held.channel_handle.clone(), held))
                        .collect(),
                    group_session,
                    directory: persisted.directory,
                },
            );
        }
        Ok(client)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error("client state file is malformed")]
    Malformed,
    #[error("client state io failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("client state serialization failed: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("client state crypto failed: {0}")]
    Crypto(#[from] crypto::Error),
    #[error("membership replay failed: {0}")]
    Membership(#[from] PlaceMembershipError),
    #[error("group session failed: {0}")]
    Group(#[from] crate::group::GroupError),
}
