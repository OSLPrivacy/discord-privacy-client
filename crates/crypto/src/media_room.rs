//! Identity-bound media encryption for multi-party rooms.
//!
//! A room epoch secret is deliberately not an AEAD key.  Each signed-in
//! device mixes that secret with its own long-term signing secret to derive
//! one epoch-scoped X25519 speaker secret.  Frames contain one ciphertext for
//! each current receiver, keyed by X25519(sender speaker secret, receiver
//! device public key).  A receiver therefore holds its own receive secret and
//! every speaker's public binding, but no foreign private sending key.

use crate::{aead, ed25519, hkdf, random, x25519, Error, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use zeroize::{Zeroize, ZeroizeOnDrop};

const JOIN_DOMAIN: &[u8] = b"OSL-MEDIA-ROOM-JOIN-V1";
const ROSTER_EVENT_DOMAIN: &[u8] = b"OSL-MEDIA-ROSTER-EVENT-V1";
const SPEAKER_KDF_DOMAIN: &[u8] = b"osl/media-room/speaker-secret/v1";
const FRAME_KDF_DOMAIN: &[u8] = b"osl/media-room/frame-key/v1";
const FRAME_WIRE_DOMAIN: &[u8] = b"OSL-MEDIA-ROOM-FRAME-V1";
const STORE_SCHEMA: &str = "osl-media-public-store-v1";
const MAX_MEMBERS: usize = 200;
const MAX_AUDIO_FRAME_BYTES: usize = 64 * 1024;
/// The authority never considers a device live for longer than this without
/// an authenticated heartbeat.
pub const MEDIA_ROSTER_HEARTBEAT_TIMEOUT_SECONDS: u64 = 10;

fn invalid(message: impl Into<String>) -> Error {
    Error::Internal(message.into())
}

fn put_bytes(out: &mut Vec<u8>, value: &[u8]) {
    out.extend_from_slice(&(value.len() as u32).to_be_bytes());
    out.extend_from_slice(value);
}

fn put_text(out: &mut Vec<u8>, value: &str) {
    put_bytes(out, value.as_bytes());
}

fn sha256(parts: &[&[u8]]) -> [u8; 32] {
    let mut digest = Sha256::new();
    for part in parts {
        digest.update((part.len() as u64).to_be_bytes());
        digest.update(part);
    }
    digest.finalize().into()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AuthenticatedDeviceId {
    pub account_id: String,
    pub device_id: String,
}

impl AuthenticatedDeviceId {
    pub fn label(&self) -> String {
        format!("{}/{}", self.account_id, self.device_id)
    }

    fn canonical_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        put_text(&mut out, &self.account_id);
        put_text(&mut out, &self.device_id);
        out
    }

    fn validate(&self) -> Result<()> {
        if self.account_id.is_empty()
            || self.account_id.len() > 128
            || self.device_id.is_empty()
            || self.device_id.len() > 128
        {
            return Err(invalid(format!(
                "speaker {} authenticated identity is invalid",
                self.label()
            )));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AuthenticatedDevicePublic {
    pub identity: AuthenticatedDeviceId,
    pub identity_signing_public: [u8; 32],
    pub receive_public: [u8; 32],
}

#[derive(Clone)]
pub struct SignedInDevice {
    pub identity: AuthenticatedDeviceId,
    identity_signing_secret: ed25519::SecretKey,
    receive_secret: x25519::SecretKey,
}

impl SignedInDevice {
    pub fn generate(account_id: impl Into<String>, device_id: impl Into<String>) -> Result<Self> {
        let identity = AuthenticatedDeviceId {
            account_id: account_id.into(),
            device_id: device_id.into(),
        };
        identity.validate()?;
        let (identity_signing_secret, _) = ed25519::generate_keypair();
        let (receive_secret, _) = x25519::generate_keypair();
        Ok(Self {
            identity,
            identity_signing_secret,
            receive_secret,
        })
    }

    /// Reconstruct one signed-in OS process from its own private handoff.
    /// Callers must never hand one process another device's material.
    pub fn from_process_handoff(
        identity: AuthenticatedDeviceId,
        identity_signing_secret: [u8; 32],
        receive_secret: [u8; 32],
    ) -> Result<Self> {
        identity.validate()?;
        Ok(Self {
            identity,
            identity_signing_secret: ed25519::SecretKey::from_bytes(identity_signing_secret),
            receive_secret: x25519::SecretKey::from_bytes(receive_secret),
        })
    }

    pub fn public_identity(&self) -> AuthenticatedDevicePublic {
        AuthenticatedDevicePublic {
            identity: self.identity.clone(),
            identity_signing_public: *ed25519::derive_public(&self.identity_signing_secret)
                .as_bytes(),
            receive_public: *x25519::derive_public(&self.receive_secret).as_bytes(),
        }
    }

    /// Parent-to-child process handoff used by native client launchers.
    pub fn private_material_for_own_process(&self) -> [[u8; 32]; 2] {
        [
            *self.identity_signing_secret.as_bytes(),
            *self.receive_secret.as_bytes(),
        ]
    }
}

#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct MediaRoomInvitation {
    pub room_id: [u8; 16],
    pub epoch: u64,
    group_key: [u8; 32],
}

impl MediaRoomInvitation {
    pub fn new(room_id: [u8; 16], epoch: u64, group_key: [u8; 32]) -> Result<Self> {
        if room_id == [0; 16] || epoch == 0 || group_key == [0; 32] {
            return Err(invalid("media room invitation is incomplete"));
        }
        Ok(Self {
            room_id,
            epoch,
            group_key,
        })
    }

    pub fn group_key_for_own_client_process(&self) -> [u8; 32] {
        self.group_key
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SignedSpeakerJoin {
    pub device: AuthenticatedDevicePublic,
    pub room_id: [u8; 16],
    pub epoch: u64,
    pub speaker_public: [u8; 32],
    pub speaker_fingerprint: [u8; 32],
    pub identity_signature: Vec<u8>,
}

impl SignedSpeakerJoin {
    fn signing_bytes(&self) -> Vec<u8> {
        let mut out = JOIN_DOMAIN.to_vec();
        out.extend_from_slice(&self.room_id);
        out.extend_from_slice(&self.epoch.to_be_bytes());
        put_bytes(&mut out, &self.device.identity.canonical_bytes());
        out.extend_from_slice(&self.device.identity_signing_public);
        out.extend_from_slice(&self.device.receive_public);
        out.extend_from_slice(&self.speaker_public);
        out.extend_from_slice(&self.speaker_fingerprint);
        out
    }

    pub fn speaker_fingerprint_hex(&self) -> String {
        hex(&self.speaker_fingerprint)
    }
}

fn derive_speaker_secret(
    invitation: &MediaRoomInvitation,
    device: &SignedInDevice,
) -> Result<x25519::SecretKey> {
    let mut info = SPEAKER_KDF_DOMAIN.to_vec();
    info.extend_from_slice(&invitation.room_id);
    info.extend_from_slice(&invitation.epoch.to_be_bytes());
    put_bytes(&mut info, &device.identity.canonical_bytes());
    Ok(x25519::SecretKey::from_bytes(hkdf::derive_32(
        &invitation.group_key,
        device.identity_signing_secret.as_bytes(),
        &info,
    )?))
}

pub fn prepare_room_join(
    invitation: &MediaRoomInvitation,
    device: &SignedInDevice,
) -> Result<SignedSpeakerJoin> {
    let speaker_secret = derive_speaker_secret(invitation, device)?;
    let speaker_public = *x25519::derive_public(&speaker_secret).as_bytes();
    let identity_bytes = device.identity.canonical_bytes();
    let speaker_fingerprint = sha256(&[
        b"OSL-MEDIA-SPEAKER-FINGERPRINT-V1",
        &invitation.room_id,
        &invitation.epoch.to_be_bytes(),
        &identity_bytes,
        &speaker_public,
    ]);
    let mut join = SignedSpeakerJoin {
        device: device.public_identity(),
        room_id: invitation.room_id,
        epoch: invitation.epoch,
        speaker_public,
        speaker_fingerprint,
        identity_signature: Vec::new(),
    };
    join.identity_signature = ed25519::sign(&device.identity_signing_secret, &join.signing_bytes())
        .as_bytes()
        .to_vec();
    Ok(join)
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VerifiedMediaRoster {
    pub room_id: [u8; 16],
    pub epoch: u64,
    pub speakers: Vec<SignedSpeakerJoin>,
}

pub fn verify_room_roster(
    invitation: &MediaRoomInvitation,
    mut joins: Vec<SignedSpeakerJoin>,
) -> Result<VerifiedMediaRoster> {
    if joins.is_empty() || joins.len() > MAX_MEMBERS {
        return Err(invalid(format!(
            "media room roster cardinality {} is outside 1..={MAX_MEMBERS}",
            joins.len()
        )));
    }
    joins.sort_by(|left, right| left.device.identity.cmp(&right.device.identity));
    let mut identities = BTreeSet::new();
    let mut fingerprints = BTreeMap::<[u8; 32], AuthenticatedDeviceId>::new();
    for join in &joins {
        let speaker = join.device.identity.label();
        join.device.identity.validate()?;
        if join.room_id != invitation.room_id || join.epoch != invitation.epoch {
            return Err(invalid(format!(
                "speaker {speaker} identity binding uses another room epoch"
            )));
        }
        if !identities.insert(join.device.identity.clone()) {
            return Err(invalid(format!(
                "speaker {speaker} authenticated identity is duplicated"
            )));
        }
        let signature: [u8; 64] = join.identity_signature.as_slice().try_into().map_err(|_| {
            invalid(format!(
                "speaker {speaker} identity binding signature is malformed"
            ))
        })?;
        let verified = ed25519::verify(
            &ed25519::PublicKey::from_bytes(join.device.identity_signing_public),
            &join.signing_bytes(),
            &ed25519::Signature::from_bytes(signature),
        )?;
        let expected_fingerprint = sha256(&[
            b"OSL-MEDIA-SPEAKER-FINGERPRINT-V1",
            &invitation.room_id,
            &invitation.epoch.to_be_bytes(),
            &join.device.identity.canonical_bytes(),
            &join.speaker_public,
        ]);
        if !verified || expected_fingerprint != join.speaker_fingerprint {
            return Err(invalid(format!(
                "speaker {speaker} identity binding signature/fingerprint mismatch"
            )));
        }
        if let Some(first) =
            fingerprints.insert(join.speaker_fingerprint, join.device.identity.clone())
        {
            return Err(invalid(format!(
                "speaker {speaker} speaker-key fingerprint is shared with speaker {}",
                first.label()
            )));
        }
    }
    Ok(VerifiedMediaRoster {
        room_id: invitation.room_id,
        epoch: invitation.epoch,
        speakers: joins,
    })
}

/// The authority-authenticated reason for a roster epoch.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum MediaRosterEventKind {
    Bootstrap,
    Join,
    Leave,
    Vanished,
}

impl MediaRosterEventKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Bootstrap => "bootstrap",
            Self::Join => "join",
            Self::Leave => "leave",
            Self::Vanished => "vanished",
        }
    }

    fn tag(self) -> u8 {
        match self {
            Self::Bootstrap => 1,
            Self::Join => 2,
            Self::Leave => 3,
            Self::Vanished => 4,
        }
    }
}

/// A canonical, authority-signed statement of the complete room membership.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AuthenticatedMediaRosterEvent {
    pub kind: MediaRosterEventKind,
    pub room_id: [u8; 16],
    pub epoch: u64,
    pub previous_epoch: u64,
    pub subject: Option<AuthenticatedDeviceId>,
    pub timestamp: u64,
    pub members: Vec<AuthenticatedDevicePublic>,
    pub authority_signature: Vec<u8>,
}

impl AuthenticatedMediaRosterEvent {
    /// Stable binary representation.  This deliberately does not rely on a
    /// serializer's map ordering or field-name conventions.
    pub fn canonical_signed_bytes(&self) -> Vec<u8> {
        let mut out = ROSTER_EVENT_DOMAIN.to_vec();
        out.push(self.kind.tag());
        out.extend_from_slice(&self.room_id);
        out.extend_from_slice(&self.epoch.to_be_bytes());
        out.extend_from_slice(&self.previous_epoch.to_be_bytes());
        match &self.subject {
            Some(subject) => {
                out.push(1);
                put_bytes(&mut out, &subject.canonical_bytes());
            }
            None => out.push(0),
        }
        out.extend_from_slice(&self.timestamp.to_be_bytes());
        out.extend_from_slice(&(self.members.len() as u32).to_be_bytes());
        for member in &self.members {
            put_bytes(&mut out, &member.identity.canonical_bytes());
            out.extend_from_slice(&member.identity_signing_public);
            out.extend_from_slice(&member.receive_public);
        }
        out
    }

    pub fn kind_name(&self) -> &'static str {
        self.kind.name()
    }

    pub fn verify(&self, authority_public_key: [u8; 32]) -> Result<()> {
        validate_public_members(&self.members)?;
        if self.room_id == [0; 16] || self.epoch == 0 {
            return Err(invalid("media roster event is incomplete"));
        }
        if self.kind == MediaRosterEventKind::Bootstrap {
            if self.previous_epoch != 0 || self.subject.is_some() {
                return Err(invalid("media roster bootstrap continuity is invalid"));
            }
        } else if self.previous_epoch == 0 || self.previous_epoch.checked_add(1) != Some(self.epoch)
        {
            return Err(invalid("media roster event epoch continuity is invalid"));
        }
        let signature: [u8; 64] = self
            .authority_signature
            .as_slice()
            .try_into()
            .map_err(|_| invalid("media roster authority signature is malformed"))?;
        if !ed25519::verify(
            &ed25519::PublicKey::from_bytes(authority_public_key),
            &self.canonical_signed_bytes(),
            &ed25519::Signature::from_bytes(signature),
        )? {
            return Err(invalid("media roster authority signature is invalid"));
        }
        Ok(())
    }
}

fn validate_public_members(members: &[AuthenticatedDevicePublic]) -> Result<()> {
    if members.is_empty() || members.len() > MAX_MEMBERS {
        return Err(invalid(format!(
            "media roster cardinality {} is outside 1..={MAX_MEMBERS}",
            members.len()
        )));
    }
    let mut previous = None;
    for member in members {
        member.identity.validate()?;
        if previous.as_ref() >= Some(&member.identity) {
            return Err(invalid(
                "media roster members are not exactly sorted and unique",
            ));
        }
        previous = Some(member.identity.clone());
    }
    Ok(())
}

fn sorted_public_members(
    mut members: Vec<AuthenticatedDevicePublic>,
) -> Result<Vec<AuthenticatedDevicePublic>> {
    members.sort_by(|left, right| left.identity.cmp(&right.identity));
    validate_public_members(&members)?;
    Ok(members)
}

fn fresh_group_key() -> [u8; 32] {
    loop {
        let key: [u8; 32] = random::random_bytes(32)
            .try_into()
            .expect("random group key has fixed length");
        if key != [0; 32] {
            return key;
        }
    }
}

/// The authority's result for one roster transition.  The invitation is
/// intentionally shared only through `invitation_for`, which refuses former
/// members.
#[derive(Clone)]
pub struct MediaRosterTransition {
    event: AuthenticatedMediaRosterEvent,
    invitation: MediaRoomInvitation,
}

impl MediaRosterTransition {
    pub fn event(&self) -> &AuthenticatedMediaRosterEvent {
        &self.event
    }

    pub fn invitation_for(&self, member: &AuthenticatedDeviceId) -> Result<MediaRoomInvitation> {
        if self
            .event
            .members
            .iter()
            .any(|current| &current.identity == member)
        {
            Ok(self.invitation.clone())
        } else {
            Err(invalid(format!(
                "{} is not a current media room member",
                member.label()
            )))
        }
    }

    pub fn epoch(&self) -> u64 {
        self.event.epoch
    }
}

/// Stateful signer and membership source for one room.
pub struct MediaRosterAuthority {
    room_id: [u8; 16],
    signing_secret: ed25519::SecretKey,
    authority_public_key: [u8; 32],
    current_members: Vec<AuthenticatedDevicePublic>,
    current_epoch: u64,
    last_seen: BTreeMap<AuthenticatedDeviceId, u64>,
}

impl MediaRosterAuthority {
    pub fn new(room_id: [u8; 16]) -> Result<Self> {
        if room_id == [0; 16] {
            return Err(invalid("media roster authority room id is zero"));
        }
        let (signing_secret, authority_public_key) = ed25519::generate_keypair();
        Ok(Self {
            room_id,
            signing_secret,
            authority_public_key: *authority_public_key.as_bytes(),
            current_members: Vec::new(),
            current_epoch: 0,
            last_seen: BTreeMap::new(),
        })
    }

    pub fn authority_public_key(&self) -> [u8; 32] {
        self.authority_public_key
    }

    pub fn current_epoch(&self) -> u64 {
        self.current_epoch
    }

    pub fn current_members(&self) -> &[AuthenticatedDevicePublic] {
        &self.current_members
    }

    pub fn bootstrap(
        &mut self,
        epoch: u64,
        members: Vec<AuthenticatedDevicePublic>,
        timestamp: u64,
    ) -> Result<MediaRosterTransition> {
        if self.current_epoch != 0 || epoch == 0 {
            return Err(invalid("media roster bootstrap is not available"));
        }
        self.install_transition(
            MediaRosterEventKind::Bootstrap,
            None,
            epoch,
            members,
            timestamp,
        )
    }

    pub fn join(
        &mut self,
        member: AuthenticatedDevicePublic,
        timestamp: u64,
    ) -> Result<MediaRosterTransition> {
        if self.current_epoch == 0
            || self
                .current_members
                .iter()
                .any(|item| item.identity == member.identity)
        {
            return Err(invalid("media roster join is not available"));
        }
        let mut members = self.current_members.clone();
        members.push(member.clone());
        self.install_transition(
            MediaRosterEventKind::Join,
            Some(member.identity),
            self.next_epoch()?,
            members,
            timestamp,
        )
    }

    pub fn leave(
        &mut self,
        subject: &AuthenticatedDeviceId,
        timestamp: u64,
    ) -> Result<MediaRosterTransition> {
        self.remove(MediaRosterEventKind::Leave, subject, timestamp)
    }

    pub fn vanished(
        &mut self,
        subject: &AuthenticatedDeviceId,
        timestamp: u64,
    ) -> Result<MediaRosterTransition> {
        self.remove(MediaRosterEventKind::Vanished, subject, timestamp)
    }

    pub fn heartbeat(&mut self, member: &AuthenticatedDeviceId, timestamp: u64) -> Result<()> {
        if !self
            .current_members
            .iter()
            .any(|item| &item.identity == member)
        {
            return Err(invalid(format!(
                "{} is not a current media room member",
                member.label()
            )));
        }
        let last_seen = self.last_seen.get(member).copied().unwrap_or(0);
        if timestamp < last_seen {
            return Err(invalid("media roster heartbeat moved backwards"));
        }
        self.last_seen.insert(member.clone(), timestamp);
        Ok(())
    }

    /// Expire timed-out members, producing one signed rekey per member in
    /// identity order.  A later item therefore chains from the prior one.
    pub fn expire(&mut self, timestamp: u64) -> Result<Vec<MediaRosterTransition>> {
        let expired = self
            .current_members
            .iter()
            .filter_map(|member| {
                let last_seen = self.last_seen.get(&member.identity).copied().unwrap_or(0);
                (timestamp.saturating_sub(last_seen) >= MEDIA_ROSTER_HEARTBEAT_TIMEOUT_SECONDS)
                    .then(|| member.identity.clone())
            })
            .collect::<Vec<_>>();
        let mut transitions = Vec::with_capacity(expired.len());
        for member in expired {
            transitions.push(self.vanished(&member, timestamp)?);
        }
        Ok(transitions)
    }

    fn remove(
        &mut self,
        kind: MediaRosterEventKind,
        subject: &AuthenticatedDeviceId,
        timestamp: u64,
    ) -> Result<MediaRosterTransition> {
        if self.current_epoch == 0
            || !self
                .current_members
                .iter()
                .any(|item| &item.identity == subject)
        {
            return Err(invalid("media roster removal is not available"));
        }
        let members = self
            .current_members
            .iter()
            .filter(|item| &item.identity != subject)
            .cloned()
            .collect();
        self.install_transition(
            kind,
            Some(subject.clone()),
            self.next_epoch()?,
            members,
            timestamp,
        )
    }

    fn next_epoch(&self) -> Result<u64> {
        self.current_epoch
            .checked_add(1)
            .filter(|epoch| *epoch != 0)
            .ok_or_else(|| invalid("media roster epoch exhausted"))
    }

    fn install_transition(
        &mut self,
        kind: MediaRosterEventKind,
        subject: Option<AuthenticatedDeviceId>,
        epoch: u64,
        members: Vec<AuthenticatedDevicePublic>,
        timestamp: u64,
    ) -> Result<MediaRosterTransition> {
        let members = sorted_public_members(members)?;
        let previous_epoch = self.current_epoch;
        if kind != MediaRosterEventKind::Bootstrap && epoch != self.next_epoch()? {
            return Err(invalid(
                "media roster transition did not increment exactly once",
            ));
        }
        let group_key = fresh_group_key();
        let invitation = MediaRoomInvitation::new(self.room_id, epoch, group_key)?;
        let mut event = AuthenticatedMediaRosterEvent {
            kind,
            room_id: self.room_id,
            epoch,
            previous_epoch,
            subject,
            timestamp,
            members: members.clone(),
            authority_signature: Vec::new(),
        };
        event.authority_signature =
            ed25519::sign(&self.signing_secret, &event.canonical_signed_bytes())
                .as_bytes()
                .to_vec();
        self.current_epoch = epoch;
        self.current_members = members;
        self.last_seen.retain(|member, _| {
            self.current_members
                .iter()
                .any(|current| current.identity == *member)
        });
        for member in &self.current_members {
            self.last_seen
                .entry(member.identity.clone())
                .or_insert(timestamp);
        }
        Ok(MediaRosterTransition { event, invitation })
    }
}

/// A complete installable epoch.  It is public except for its invitation's
/// group key, and verifies all cross-artifact bindings before a client uses it.
#[derive(Clone)]
pub struct AuthenticatedMediaEpochPackage {
    pub event: AuthenticatedMediaRosterEvent,
    invitation: MediaRoomInvitation,
    pub speaker_joins: Vec<SignedSpeakerJoin>,
}

impl AuthenticatedMediaEpochPackage {
    pub fn new(transition: &MediaRosterTransition, joins: Vec<SignedSpeakerJoin>) -> Self {
        Self {
            event: transition.event.clone(),
            invitation: transition.invitation.clone(),
            speaker_joins: joins,
        }
    }

    /// Reconstruct an epoch in an independently launched shipping client.
    /// The same verification performed by [`Self::verify`] remains mandatory
    /// before the package can be installed.
    pub fn from_process_handoff(
        event: AuthenticatedMediaRosterEvent,
        invitation: MediaRoomInvitation,
        speaker_joins: Vec<SignedSpeakerJoin>,
    ) -> Self {
        Self {
            event,
            invitation,
            speaker_joins,
        }
    }

    pub fn invitation_for(&self, member: &AuthenticatedDeviceId) -> Result<MediaRoomInvitation> {
        if self
            .event
            .members
            .iter()
            .any(|current| &current.identity == member)
        {
            Ok(self.invitation.clone())
        } else {
            Err(invalid(format!(
                "{} is not a current media room member",
                member.label()
            )))
        }
    }

    pub fn epoch(&self) -> u64 {
        self.event.epoch
    }

    pub fn verify(&self, authority_public_key: [u8; 32]) -> Result<VerifiedMediaRoster> {
        self.event.verify(authority_public_key)?;
        if self.invitation.room_id != self.event.room_id
            || self.invitation.epoch != self.event.epoch
        {
            return Err(invalid("media epoch package invitation/event mismatch"));
        }
        let roster = verify_room_roster(&self.invitation, self.speaker_joins.clone())?;
        let roster_members = roster
            .speakers
            .iter()
            .map(|join| join.device.clone())
            .collect::<Vec<_>>();
        if roster_members != self.event.members {
            return Err(invalid("media epoch package speaker roster is not exact"));
        }
        Ok(roster)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RecipientMediaCiphertext {
    pub recipient: AuthenticatedDeviceId,
    pub nonce: [u8; 24],
    pub ciphertext: Vec<u8>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EncryptedMediaFrame {
    pub room_id: [u8; 16],
    pub epoch: u64,
    pub sender: AuthenticatedDeviceId,
    pub speaker_fingerprint: [u8; 32],
    pub sequence: u64,
    pub recipients: Vec<RecipientMediaCiphertext>,
}

impl EncryptedMediaFrame {
    pub fn wire_bytes(&self) -> Result<Vec<u8>> {
        let mut out = FRAME_WIRE_DOMAIN.to_vec();
        let json = serde_json::to_vec(self)
            .map_err(|error| invalid(format!("media frame serialization failed: {error}")))?;
        out.extend_from_slice(&json);
        Ok(out)
    }

    pub fn packet_hash(&self) -> Result<[u8; 32]> {
        Ok(Sha256::digest(self.wire_bytes()?).into())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RelayMediaPacket {
    pub authenticated_transport_sender: AuthenticatedDeviceId,
    pub frame: EncryptedMediaFrame,
}

impl RelayMediaPacket {
    pub fn wire_bytes(&self) -> Result<Vec<u8>> {
        serde_json::to_vec(self)
            .map_err(|error| invalid(format!("relay media serialization failed: {error}")))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MediaKeyInventory {
    pub owner: AuthenticatedDeviceId,
    pub own_private_sending_keys: usize,
    pub foreign_private_sending_keys: usize,
    pub exported_private_sending_keys: usize,
    pub public_speaker_keys: usize,
}

fn expected_rekey_members(
    old_members: &[AuthenticatedDevicePublic],
    event: &AuthenticatedMediaRosterEvent,
) -> Result<Vec<AuthenticatedDevicePublic>> {
    let subject = event
        .subject
        .as_ref()
        .ok_or_else(|| invalid("media room rekey event has no subject"))?;
    let mut members = old_members.to_vec();
    match event.kind {
        MediaRosterEventKind::Join => {
            if members.iter().any(|member| member.identity == *subject) {
                return Err(invalid("media room rekey join subject already exists"));
            }
            let joined = event
                .members
                .iter()
                .find(|member| member.identity == *subject)
                .cloned()
                .ok_or_else(|| invalid("media room rekey join subject is absent"))?;
            members.push(joined);
        }
        MediaRosterEventKind::Leave | MediaRosterEventKind::Vanished => {
            let before = members.len();
            members.retain(|member| member.identity != *subject);
            if members.len() + 1 != before {
                return Err(invalid("media room rekey removal subject is absent"));
            }
        }
        MediaRosterEventKind::Bootstrap => {
            return Err(invalid("media room bootstrap cannot rekey a client"));
        }
    }
    sorted_public_members(members)
}

pub struct MediaRoomClient {
    invitation: MediaRoomInvitation,
    device: SignedInDevice,
    roster: VerifiedMediaRoster,
    speaker_secret: x25519::SecretKey,
    own_fingerprint: [u8; 32],
    shipping_send_calls: u64,
    shipping_receive_calls: u64,
}

impl MediaRoomClient {
    /// Join an authority-authenticated epoch.  This supplements, rather than
    /// replaces, the TASK 4702 `join_room` API.
    pub fn join_authenticated(
        package: &AuthenticatedMediaEpochPackage,
        authority_public_key: [u8; 32],
        device: SignedInDevice,
    ) -> Result<Self> {
        let roster = package.verify(authority_public_key)?;
        let invitation = package.invitation_for(&device.identity)?;
        Self::join_room(invitation, device, roster)
    }

    pub fn join_room(
        invitation: MediaRoomInvitation,
        device: SignedInDevice,
        roster: VerifiedMediaRoster,
    ) -> Result<Self> {
        if roster.room_id != invitation.room_id || roster.epoch != invitation.epoch {
            return Err(invalid(format!(
                "speaker {} joined with a foreign room roster",
                device.identity.label()
            )));
        }
        let speaker_secret = derive_speaker_secret(&invitation, &device)?;
        let speaker_public = *x25519::derive_public(&speaker_secret).as_bytes();
        let own = roster
            .speakers
            .iter()
            .find(|join| join.device.identity == device.identity)
            .ok_or_else(|| {
                invalid(format!(
                    "speaker {} authenticated identity is absent from room roster",
                    device.identity.label()
                ))
            })?;
        let public = device.public_identity();
        if own.device != public || own.speaker_public != speaker_public {
            return Err(invalid(format!(
                "speaker {} sender key mismatch with authenticated identity binding",
                device.identity.label()
            )));
        }
        let own_fingerprint = own.speaker_fingerprint;
        Ok(Self {
            invitation,
            device,
            roster,
            speaker_secret,
            own_fingerprint,
            shipping_send_calls: 0,
            shipping_receive_calls: 0,
        })
    }

    pub fn identity(&self) -> &AuthenticatedDeviceId {
        &self.device.identity
    }

    pub fn current_epoch(&self) -> u64 {
        self.invitation.epoch
    }

    pub fn current_members(&self) -> Vec<AuthenticatedDevicePublic> {
        self.roster
            .speakers
            .iter()
            .map(|speaker| speaker.device.clone())
            .collect()
    }

    /// Verify and install the immediate successor epoch.  Counter state is
    /// intentionally retained: rekeying is membership state, not a new call.
    pub fn rekey_authenticated(
        &mut self,
        package: &AuthenticatedMediaEpochPackage,
        authority_public_key: [u8; 32],
    ) -> Result<()> {
        let event = &package.event;
        if Some(event.epoch) != self.invitation.epoch.checked_add(1)
            || event.previous_epoch != self.invitation.epoch
        {
            return Err(invalid("media room rekey epoch continuity is invalid"));
        }
        let roster = package.verify(authority_public_key)?;
        let old_members = self.current_members();
        let expected_members = expected_rekey_members(&old_members, event)?;
        if expected_members != event.members {
            return Err(invalid("media room rekey membership delta is not exact"));
        }
        let invitation = package.invitation_for(&self.device.identity)?;
        if invitation.group_key == self.invitation.group_key {
            return Err(invalid("media room rekey group key is not fresh"));
        }
        let speaker_secret = derive_speaker_secret(&invitation, &self.device)?;
        let speaker_public = *x25519::derive_public(&speaker_secret).as_bytes();
        let own = roster
            .speakers
            .iter()
            .find(|join| join.device.identity == self.device.identity)
            .ok_or_else(|| {
                invalid(format!(
                    "speaker {} authenticated identity is absent from room roster",
                    self.device.identity.label()
                ))
            })?;
        if own.device != self.device.public_identity() || own.speaker_public != speaker_public {
            return Err(invalid(format!(
                "speaker {} sender key mismatch with authenticated identity binding",
                self.device.identity.label()
            )));
        }
        let own_fingerprint = own.speaker_fingerprint;
        self.invitation = invitation;
        self.roster = roster;
        self.speaker_secret = speaker_secret;
        self.own_fingerprint = own_fingerprint;
        Ok(())
    }

    pub fn speaker_fingerprint(&self) -> [u8; 32] {
        self.own_fingerprint
    }

    pub fn speaker_fingerprint_hex(&self) -> String {
        hex(&self.own_fingerprint)
    }

    pub fn shipping_send_calls(&self) -> u64 {
        self.shipping_send_calls
    }

    pub fn shipping_receive_calls(&self) -> u64 {
        self.shipping_receive_calls
    }

    pub fn key_inventory(&self) -> MediaKeyInventory {
        MediaKeyInventory {
            owner: self.device.identity.clone(),
            own_private_sending_keys: 1,
            foreign_private_sending_keys: 0,
            exported_private_sending_keys: 0,
            public_speaker_keys: self.roster.speakers.len(),
        }
    }

    pub fn persisted_public_store_bytes(&self) -> Result<Vec<u8>> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct PublicStore<'a> {
            schema: &'static str,
            owner: &'a AuthenticatedDeviceId,
            own_speaker_fingerprint: String,
            receive_bindings: &'a [SignedSpeakerJoin],
        }
        serde_json::to_vec_pretty(&PublicStore {
            schema: STORE_SCHEMA,
            owner: &self.device.identity,
            own_speaker_fingerprint: self.speaker_fingerprint_hex(),
            receive_bindings: &self.roster.speakers,
        })
        .map_err(|error| invalid(format!("media public store serialization failed: {error}")))
    }

    pub fn send_media_frame(
        &mut self,
        sequence: u64,
        plaintext: &[u8],
    ) -> Result<EncryptedMediaFrame> {
        if plaintext.is_empty() || plaintext.len() > MAX_AUDIO_FRAME_BYTES {
            return Err(invalid(format!(
                "speaker {} audio frame is empty or oversized",
                self.device.identity.label()
            )));
        }
        let mut frame = EncryptedMediaFrame {
            room_id: self.invitation.room_id,
            epoch: self.invitation.epoch,
            sender: self.device.identity.clone(),
            speaker_fingerprint: self.own_fingerprint,
            sequence,
            recipients: Vec::with_capacity(self.roster.speakers.len()),
        };
        for receiver in &self.roster.speakers {
            let shared = x25519::diffie_hellman(
                &self.speaker_secret,
                &x25519::PublicKey::from_bytes(receiver.device.receive_public),
            )?;
            let key = frame_key(
                &self.invitation,
                &frame.sender,
                &receiver.device.identity,
                shared.as_bytes(),
            )?;
            let nonce = random::random_nonce();
            let aad = frame_aad(&frame, &receiver.device.identity);
            let ciphertext = aead::seal(&aead::Key::from_bytes(key), &nonce, &aad, plaintext)?;
            frame.recipients.push(RecipientMediaCiphertext {
                recipient: receiver.device.identity.clone(),
                nonce: *nonce.as_bytes(),
                ciphertext,
            });
        }
        self.shipping_send_calls += 1;
        Ok(frame)
    }

    pub fn receive_media_frame(&mut self, packet: &RelayMediaPacket) -> Result<Vec<u8>> {
        self.shipping_receive_calls += 1;
        let frame = &packet.frame;
        let claimed = frame.sender.label();
        if packet.authenticated_transport_sender != frame.sender {
            return Err(invalid(format!("speaker {claimed} sender key mismatch")));
        }
        if frame.room_id != self.invitation.room_id || frame.epoch != self.invitation.epoch {
            return Err(invalid(format!("speaker {claimed} sender key mismatch")));
        }
        let binding = self
            .roster
            .speakers
            .iter()
            .find(|join| join.device.identity == frame.sender)
            .ok_or_else(|| invalid(format!("speaker {claimed} sender key mismatch")))?;
        if binding.speaker_fingerprint != frame.speaker_fingerprint {
            return Err(invalid(format!("speaker {claimed} sender key mismatch")));
        }
        let recipient = frame
            .recipients
            .iter()
            .find(|ciphertext| ciphertext.recipient == self.device.identity)
            .ok_or_else(|| {
                invalid(format!(
                    "speaker {claimed} omitted receiver {}",
                    self.device.identity.label()
                ))
            })?;
        let shared = x25519::diffie_hellman(
            &self.device.receive_secret,
            &x25519::PublicKey::from_bytes(binding.speaker_public),
        )?;
        let key = frame_key(
            &self.invitation,
            &frame.sender,
            &self.device.identity,
            shared.as_bytes(),
        )?;
        aead::open(
            &aead::Key::from_bytes(key),
            &aead::Nonce::from_bytes(recipient.nonce),
            &frame_aad(frame, &self.device.identity),
            &recipient.ciphertext,
        )
        .map_err(|_| invalid(format!("speaker {claimed} sender key mismatch")))
    }
}

fn frame_key(
    invitation: &MediaRoomInvitation,
    sender: &AuthenticatedDeviceId,
    recipient: &AuthenticatedDeviceId,
    shared: &[u8; 32],
) -> Result<[u8; 32]> {
    let mut info = FRAME_KDF_DOMAIN.to_vec();
    info.extend_from_slice(&invitation.room_id);
    info.extend_from_slice(&invitation.epoch.to_be_bytes());
    put_bytes(&mut info, &sender.canonical_bytes());
    put_bytes(&mut info, &recipient.canonical_bytes());
    hkdf::derive_32(&invitation.group_key, shared, &info)
}

fn frame_aad(frame: &EncryptedMediaFrame, recipient: &AuthenticatedDeviceId) -> Vec<u8> {
    let mut out = FRAME_WIRE_DOMAIN.to_vec();
    out.extend_from_slice(&frame.room_id);
    out.extend_from_slice(&frame.epoch.to_be_bytes());
    put_bytes(&mut out, &frame.sender.canonical_bytes());
    out.extend_from_slice(&frame.speaker_fingerprint);
    out.extend_from_slice(&frame.sequence.to_be_bytes());
    put_bytes(&mut out, &recipient.canonical_bytes());
    out
}

#[derive(Clone, Debug)]
pub struct MediaRelayCapture {
    pub packet_hash: [u8; 32],
    pub authenticated_transport_sender: AuthenticatedDeviceId,
    pub claimed_sender: AuthenticatedDeviceId,
    pub speaker_fingerprint: [u8; 32],
    pub ciphertext_bytes: usize,
    pub wire_bytes: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct OpaqueMediaRelay {
    room_id: [u8; 16],
    epoch: u64,
    receivers: BTreeSet<AuthenticatedDeviceId>,
    inboxes: BTreeMap<AuthenticatedDeviceId, Vec<RelayMediaPacket>>,
    captures: Vec<MediaRelayCapture>,
}

impl OpaqueMediaRelay {
    pub fn new(roster: &VerifiedMediaRoster) -> Self {
        let receivers = roster
            .speakers
            .iter()
            .map(|speaker| speaker.device.identity.clone())
            .collect::<BTreeSet<_>>();
        Self {
            room_id: roster.room_id,
            epoch: roster.epoch,
            inboxes: receivers
                .iter()
                .cloned()
                .map(|receiver| (receiver, Vec::new()))
                .collect(),
            receivers,
            captures: Vec::new(),
        }
    }

    /// Forward opaque ciphertext.  The transport identity is kept separately
    /// from the untrusted claimed header so every receiver can enforce the
    /// identity-bound speaker key even if the relay is malicious.
    pub fn forward(
        &mut self,
        authenticated_transport_sender: AuthenticatedDeviceId,
        frame: EncryptedMediaFrame,
    ) -> Result<[u8; 32]> {
        if !self.receivers.contains(&authenticated_transport_sender)
            || frame.room_id != self.room_id
            || frame.epoch != self.epoch
        {
            return Err(invalid(format!(
                "speaker {} relay room admission mismatch",
                authenticated_transport_sender.label()
            )));
        }
        let packet = RelayMediaPacket {
            authenticated_transport_sender: authenticated_transport_sender.clone(),
            frame,
        };
        let wire_bytes = packet.wire_bytes()?;
        let packet_hash: [u8; 32] = Sha256::digest(&wire_bytes).into();
        let ciphertext_bytes = packet
            .frame
            .recipients
            .iter()
            .map(|recipient| recipient.ciphertext.len())
            .sum();
        self.captures.push(MediaRelayCapture {
            packet_hash,
            authenticated_transport_sender,
            claimed_sender: packet.frame.sender.clone(),
            speaker_fingerprint: packet.frame.speaker_fingerprint,
            ciphertext_bytes,
            wire_bytes,
        });
        for inbox in self.inboxes.values_mut() {
            inbox.push(packet.clone());
        }
        Ok(packet_hash)
    }

    pub fn drain_for(&mut self, receiver: &AuthenticatedDeviceId) -> Result<Vec<RelayMediaPacket>> {
        self.inboxes
            .get_mut(receiver)
            .map(std::mem::take)
            .ok_or_else(|| {
                invalid(format!(
                    "receiver {} is not in media room",
                    receiver.label()
                ))
            })
    }

    pub fn captures(&self) -> &[MediaRelayCapture] {
        &self.captures
    }
}
