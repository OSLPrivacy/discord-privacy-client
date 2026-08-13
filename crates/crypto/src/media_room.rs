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
use subtle::ConstantTimeEq;
use zeroize::{Zeroize, ZeroizeOnDrop};

const JOIN_DOMAIN: &[u8] = b"OSL-MEDIA-ROOM-JOIN-V1";
const SPEAKER_KDF_DOMAIN: &[u8] = b"osl/media-room/speaker-secret/v1";
const FRAME_KDF_DOMAIN: &[u8] = b"osl/media-room/frame-key/v1";
const FRAME_WIRE_DOMAIN: &[u8] = b"OSL-MEDIA-ROOM-FRAME-V1";
const STORE_SCHEMA: &str = "osl-media-public-store-v1";
const MAX_MEMBERS: usize = 200;
const MAX_AUDIO_FRAME_BYTES: usize = 64 * 1024;

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

/// The only membership changes that advance a media-room key epoch.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RosterEventKind {
    Join,
    Leave,
    ConnectionExpired,
}

/// Public, authority-signed explanation for one group-key rotation.
///
/// The group key is deliberately absent.  It is delivered separately to the
/// members named by `members`, while this event can safely be retained as an
/// audit record by clients and the relay.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AuthenticatedRosterEvent {
    pub room_id: [u8; 16],
    pub previous_epoch: u64,
    pub key_epoch: u64,
    pub kind: RosterEventKind,
    pub subject: AuthenticatedDeviceId,
    pub members: Vec<AuthenticatedDeviceId>,
    pub previous_roster_digest: [u8; 32],
    pub roster_digest: [u8; 32],
    pub group_key_commitment: [u8; 32],
    pub observed_at_unix_ms: u64,
    pub connection_expiry_delay_ms: Option<u64>,
    pub authority_signature: Vec<u8>,
}

impl AuthenticatedRosterEvent {
    fn signing_bytes(&self) -> Vec<u8> {
        let mut out = b"OSL-MEDIA-ROSTER-EVENT-V1".to_vec();
        out.extend_from_slice(&self.room_id);
        out.extend_from_slice(&self.previous_epoch.to_be_bytes());
        out.extend_from_slice(&self.key_epoch.to_be_bytes());
        out.push(match self.kind {
            RosterEventKind::Join => 1,
            RosterEventKind::Leave => 2,
            RosterEventKind::ConnectionExpired => 3,
        });
        put_bytes(&mut out, &self.subject.canonical_bytes());
        out.extend_from_slice(&(self.members.len() as u32).to_be_bytes());
        for member in &self.members {
            put_bytes(&mut out, &member.canonical_bytes());
        }
        out.extend_from_slice(&self.previous_roster_digest);
        out.extend_from_slice(&self.roster_digest);
        out.extend_from_slice(&self.group_key_commitment);
        out.extend_from_slice(&self.observed_at_unix_ms.to_be_bytes());
        match self.connection_expiry_delay_ms {
            Some(delay) => {
                out.push(1);
                out.extend_from_slice(&delay.to_be_bytes());
            }
            None => out.push(0),
        }
        out
    }
}

fn roster_digest(roster: &VerifiedMediaRoster) -> [u8; 32] {
    let mut bytes = b"OSL-MEDIA-ROSTER-DIGEST-V1".to_vec();
    bytes.extend_from_slice(&roster.room_id);
    bytes.extend_from_slice(&roster.epoch.to_be_bytes());
    bytes.extend_from_slice(&(roster.speakers.len() as u32).to_be_bytes());
    for speaker in &roster.speakers {
        put_bytes(&mut bytes, &speaker.signing_bytes());
        put_bytes(&mut bytes, &speaker.identity_signature);
    }
    Sha256::digest(bytes).into()
}

fn group_key_commitment(invitation: &MediaRoomInvitation) -> [u8; 32] {
    sha256(&[
        b"OSL-MEDIA-GROUP-KEY-COMMITMENT-V1",
        &invitation.room_id,
        &invitation.epoch.to_be_bytes(),
        &invitation.group_key,
    ])
}

fn roster_identities(roster: &VerifiedMediaRoster) -> BTreeSet<AuthenticatedDeviceId> {
    roster
        .speakers
        .iter()
        .map(|speaker| speaker.device.identity.clone())
        .collect()
}

fn validate_roster_change(
    previous: &VerifiedMediaRoster,
    next: &VerifiedMediaRoster,
    event: &AuthenticatedRosterEvent,
) -> Result<()> {
    if previous.room_id != next.room_id
        || event.room_id != previous.room_id
        || event.previous_epoch != previous.epoch
        || event.key_epoch != next.epoch
        || next.epoch
            != previous
                .epoch
                .checked_add(1)
                .ok_or_else(|| invalid("media roster key epoch overflow"))?
    {
        return Err(invalid(format!(
            "roster event epoch chain {} -> {} is invalid",
            event.previous_epoch, event.key_epoch
        )));
    }
    if event.observed_at_unix_ms == 0 {
        return Err(invalid(format!(
            "roster event epoch {} has no observation time",
            event.key_epoch
        )));
    }
    let previous_members = roster_identities(previous);
    let next_members = roster_identities(next);
    let event_members = event.members.iter().cloned().collect::<BTreeSet<_>>();
    if event.members.len() != event_members.len() || event_members != next_members {
        return Err(invalid(format!(
            "roster event epoch {} member snapshot mismatch",
            event.key_epoch
        )));
    }
    if event.previous_roster_digest != roster_digest(previous)
        || event.roster_digest != roster_digest(next)
    {
        return Err(invalid(format!(
            "roster event epoch {} signed roster digest mismatch",
            event.key_epoch
        )));
    }
    let mut expected = previous_members.clone();
    match event.kind {
        RosterEventKind::Join => {
            if !expected.insert(event.subject.clone()) {
                return Err(invalid(format!(
                    "roster join epoch {} subject was already present",
                    event.key_epoch
                )));
            }
            if event.connection_expiry_delay_ms.is_some() {
                return Err(invalid(format!(
                    "roster join epoch {} carried a connection-expiry delay",
                    event.key_epoch
                )));
            }
        }
        RosterEventKind::Leave => {
            if !expected.remove(&event.subject) {
                return Err(invalid(format!(
                    "roster leave epoch {} subject was absent",
                    event.key_epoch
                )));
            }
            if event.connection_expiry_delay_ms.is_some() {
                return Err(invalid(format!(
                    "roster leave epoch {} carried a connection-expiry delay",
                    event.key_epoch
                )));
            }
        }
        RosterEventKind::ConnectionExpired => {
            if !expected.remove(&event.subject) {
                return Err(invalid(format!(
                    "roster connection-expired epoch {} subject was absent",
                    event.key_epoch
                )));
            }
            let delay = event.connection_expiry_delay_ms.ok_or_else(|| {
                invalid(format!(
                    "roster connection-expired epoch {} omitted detection delay",
                    event.key_epoch
                ))
            })?;
            if delay > 10_000 {
                return Err(invalid(format!(
                    "roster connection-expired epoch {} detection delay {delay}ms exceeds 10000ms",
                    event.key_epoch
                )));
            }
        }
    }
    if expected != next_members {
        return Err(invalid(format!(
            "roster event epoch {} changed members beyond its named subject",
            event.key_epoch
        )));
    }
    Ok(())
}

pub fn sign_authenticated_roster_event(
    authority_secret: &ed25519::SecretKey,
    previous: &VerifiedMediaRoster,
    next: &VerifiedMediaRoster,
    next_invitation: &MediaRoomInvitation,
    kind: RosterEventKind,
    subject: AuthenticatedDeviceId,
    observed_at_unix_ms: u64,
    connection_expiry_delay_ms: Option<u64>,
) -> Result<AuthenticatedRosterEvent> {
    if next_invitation.room_id != next.room_id || next_invitation.epoch != next.epoch {
        return Err(invalid(
            "roster event group-key invitation is for another epoch",
        ));
    }
    let mut event = AuthenticatedRosterEvent {
        room_id: next.room_id,
        previous_epoch: previous.epoch,
        key_epoch: next.epoch,
        kind,
        subject,
        members: roster_identities(next).into_iter().collect(),
        previous_roster_digest: roster_digest(previous),
        roster_digest: roster_digest(next),
        group_key_commitment: group_key_commitment(next_invitation),
        observed_at_unix_ms,
        connection_expiry_delay_ms,
        authority_signature: Vec::new(),
    };
    validate_roster_change(previous, next, &event)?;
    event.authority_signature = ed25519::sign(authority_secret, &event.signing_bytes())
        .as_bytes()
        .to_vec();
    Ok(event)
}

pub fn verify_roster_event_group_key(
    event: &AuthenticatedRosterEvent,
    invitation: &MediaRoomInvitation,
) -> Result<()> {
    if event.room_id != invitation.room_id
        || event.key_epoch != invitation.epoch
        || !bool::from(
            event
                .group_key_commitment
                .ct_eq(&group_key_commitment(invitation)),
        )
    {
        return Err(invalid(format!(
            "roster event epoch {} group-key commitment mismatch",
            event.key_epoch
        )));
    }
    Ok(())
}

pub fn verify_authenticated_roster_event(
    authority_public: &ed25519::PublicKey,
    previous: &VerifiedMediaRoster,
    next: &VerifiedMediaRoster,
    event: &AuthenticatedRosterEvent,
) -> Result<()> {
    validate_roster_change(previous, next, event)?;
    let signature: [u8; 64] = event
        .authority_signature
        .as_slice()
        .try_into()
        .map_err(|_| {
            invalid(format!(
                "roster event epoch {} authority signature is malformed",
                event.key_epoch
            ))
        })?;
    if !ed25519::verify(
        authority_public,
        &event.signing_bytes(),
        &ed25519::Signature::from_bytes(signature),
    )? {
        return Err(invalid(format!(
            "roster event epoch {} authority signature mismatch",
            event.key_epoch
        )));
    }
    Ok(())
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

    pub fn key_epoch(&self) -> u64 {
        self.invitation.epoch
    }

    /// Atomically replace the old epoch secret and roster after authenticating
    /// the exact one-member transition.  A removed client cannot call this:
    /// `join_room` refuses a roster that no longer contains its identity.
    pub fn rekey_for_roster_event(
        &mut self,
        invitation: MediaRoomInvitation,
        roster: VerifiedMediaRoster,
        event: &AuthenticatedRosterEvent,
        authority_public: &ed25519::PublicKey,
    ) -> Result<()> {
        verify_authenticated_roster_event(authority_public, &self.roster, &roster, event)?;
        verify_roster_event_group_key(event, &invitation)?;
        if bool::from(invitation.group_key.ct_eq(&self.invitation.group_key)) {
            return Err(invalid(format!(
                "roster event epoch {} reused the previous group key",
                event.key_epoch
            )));
        }
        let send_calls = self.shipping_send_calls;
        let receive_calls = self.shipping_receive_calls;
        let mut rotated = Self::join_room(invitation, self.device.clone(), roster)?;
        rotated.shipping_send_calls = send_calls;
        rotated.shipping_receive_calls = receive_calls;
        *self = rotated;
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
