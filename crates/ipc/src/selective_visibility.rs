//! Client-side selective-audience delivery.
//!
//! This wire intentionally does **not** reuse v=3's recipient-hash slot.
//! A selective store object has opaque, fixed-shape KEM wraps; a recipient
//! tries every wrap locally and only a selected key opens one.  Consequently
//! the identity-blind store sees one object and cannot map a wrap to a member
//! public key.  The signed manifest is inside the body cipher, binds the
//! plaintext digest and the exact membership snapshot used at send time, and
//! is verified before a client renders anything.

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use crypto::{aes_gcm, ed25519, hkdf, ml_kem_768, pqxdh, random, x25519};
use sha2::{Digest, Sha256};

/// Prefix for the one opaque object uploaded for a selective message.
pub const SELECTIVE_PREFIX: &str = "OSLS1::";
const VERSION: u8 = 1;
const BODY_VERSION: u8 = 1;
const SLOT_BYTES: usize = 32
    + 2
    + ml_kem_768::CIPHERTEXT_SIZE
    + aes_gcm::NONCE_SIZE
    + aes_gcm::KEY_SIZE
    + aes_gcm::TAG_SIZE;
const HEADER_BYTES: usize = 1 + 32 + 1;
const WRAP_AAD: &[u8] = b"OSL/selective-audience/wrap/v1";
const BODY_AAD: &[u8] = b"OSL/selective-audience/body/v1";
const WRAP_INFO: &[u8] = b"OSL/selective-audience/wrap-key/v1";
const MANIFEST_DOMAIN: &[u8] = b"OSL/selective-audience/manifest/v1\0";
const SNAPSHOT_DOMAIN: &[u8] = b"OSL/selective-audience/membership/v1\0";
const MEMBER_DOMAIN: &[u8] = b"OSL/selective-audience/member/v1\0";

/// The sender's policy choice.  Identities stay local to the caller: indices
/// are never serialised and are translated to opaque recipient keys first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Audience {
    OnlyThese(Vec<usize>),
    HideFrom(Vec<usize>),
}

impl Audience {
    fn tag(&self) -> u8 {
        match self {
            Self::OnlyThese(_) => 1,
            Self::HideFrom(_) => 2,
        }
    }
}

/// A current enclave member's two public recipient keys.  No member id is
/// carried on the wire or accepted by the store-facing API.
#[derive(Clone)]
pub struct SelectiveRecipient {
    pub x25519_pub: x25519::PublicKey,
    pub mlkem_pub: ml_kem_768::EncapsulationKey,
}

/// The secret keys held by one client.  They may be reconstructed from sealed
/// local storage after a restart; no replay cursor is consumed by this wire.
pub struct SelectiveRecipientSecret {
    pub x25519_secret: x25519::SecretKey,
    pub mlkem_secret: ml_kem_768::DecapsulationKey,
}

/// The one marker an authorised client may render.  It deliberately contains
/// neither a count nor an audience/member identity.
pub const SENT_ONLY_SELECTIVE_AUDIENCE_MARKER: &str = "sent-only selective audience";

/// Concrete surface accounting used by the receive boundary.  A hidden
/// delivery must be all zero: callers have no object hint or placeholder to
/// turn into an unread, notification, reaction/reply target, or timing row.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ClientSurfaces {
    pub timeline_entries: u8,
    pub message_keys: u8,
    pub object_hints: u8,
    pub placeholders: u8,
    pub unread_increments: u8,
    pub notifications: u8,
    pub reaction_targets: u8,
    pub reply_targets: u8,
    pub osl_timing_metadata: u8,
}

impl ClientSurfaces {
    pub fn hidden() -> Self {
        Self::default()
    }

    pub fn selected() -> Self {
        Self {
            timeline_entries: 1,
            message_keys: 1,
            ..Self::default()
        }
    }

    pub fn is_zero(&self) -> bool {
        self == &Self::default()
    }
}

/// The receive result is deliberately not a placeholder.  `Hidden` provides
/// no message-derived data, so the surrounding timeline joins its preceding
/// and following visible entries directly.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SelectiveDelivery {
    Hidden {
        surfaces: ClientSurfaces,
    },
    Selected {
        plaintext: Vec<u8>,
        marker: &'static str,
        surfaces: ClientSurfaces,
        manifest: VerifiedAudienceManifest,
    },
}

/// Audience facts available after decryption and signature verification.  The
/// UI gets only the generic marker; this is retained for audit/verification.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerifiedAudienceManifest {
    pub mode: u8,
    pub message_digest: [u8; 32],
    pub membership_snapshot: [u8; 32],
    pub selected_member_commitments: Vec<[u8; 32]>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SelectiveError {
    #[error("selective audience requires at least one current member")]
    EmptyMembership,
    #[error("selective audience has duplicate member keys")]
    DuplicateMember,
    #[error("selective audience index {0} is outside the membership snapshot")]
    InvalidAudienceIndex(usize),
    #[error("selective audience object is malformed")]
    Malformed,
    #[error("selective audience membership snapshot changed")]
    MembershipRace,
    #[error("selective audience manifest signature failed")]
    InvalidSignature,
    #[error("selective audience manifest does not bind this message")]
    MessageBinding,
    #[error("selective audience crypto failed")]
    Crypto,
}

/// Create one store object.  `members` is the membership snapshot observed at
/// send time; the selected effective set is calculated before encryption, so
/// no server ACL and no recipient identity lookup participates in delivery.
pub fn seal(
    sender_x25519_secret: &x25519::SecretKey,
    sender_x25519_public: &x25519::PublicKey,
    sender_signing_secret: &ed25519::SecretKey,
    members: &[SelectiveRecipient],
    audience: Audience,
    plaintext: &[u8],
) -> Result<String, SelectiveError> {
    validate_members(members)?;
    let selected = selected_indices(members.len(), &audience)?;
    let snapshot = membership_snapshot(members);
    let selected_commitments = selected
        .iter()
        .map(|index| member_commitment(&members[*index]))
        .collect::<Vec<_>>();
    let message_digest = digest(plaintext);
    let signature = ed25519::sign(
        sender_signing_secret,
        &manifest_signing_bytes(
            audience.tag(),
            &message_digest,
            &snapshot,
            &selected_commitments,
        ),
    );
    let payload = encode_payload(
        audience.tag(),
        plaintext,
        &snapshot,
        &selected_commitments,
        signature.as_bytes(),
    )?;

    let body_key_bytes = random::random_bytes(aes_gcm::KEY_SIZE);
    let mut body_key_array = [0u8; aes_gcm::KEY_SIZE];
    body_key_array.copy_from_slice(&body_key_bytes);
    let body_key = aes_gcm::Key::from_bytes(body_key_array);

    let mut header = Vec::with_capacity(HEADER_BYTES);
    header.push(VERSION);
    header.extend_from_slice(sender_x25519_public.as_bytes());
    header.push(u8::try_from(selected.len()).map_err(|_| SelectiveError::Malformed)?);
    let body_aad = bound_aad(BODY_AAD, &header, 0);
    let (body_nonce, body_ciphertext) =
        aes_gcm::seal(&body_key, &body_aad, &payload).map_err(|_| SelectiveError::Crypto)?;

    let mut raw = Vec::with_capacity(
        header.len() + selected.len() * SLOT_BYTES + aes_gcm::NONCE_SIZE + body_ciphertext.len(),
    );
    raw.extend_from_slice(&header);
    for (slot, member_index) in selected.into_iter().enumerate() {
        let member = &members[member_index];
        let (session_key, handshake) = pqxdh::initiate(
            sender_x25519_secret,
            &member.x25519_pub,
            &member.x25519_pub,
            None,
            &member.mlkem_pub,
        )
        .map_err(|_| SelectiveError::Crypto)?;
        let wrap_key = aes_gcm::Key::from_bytes(
            hkdf::derive_32(&[], session_key.as_bytes(), WRAP_INFO)
                .map_err(|_| SelectiveError::Crypto)?,
        );
        let wrap_aad = bound_aad(WRAP_AAD, &header, slot as u16);
        let (nonce, wrapped_key) = aes_gcm::seal(&wrap_key, &wrap_aad, body_key.as_bytes())
            .map_err(|_| SelectiveError::Crypto)?;
        raw.extend_from_slice(handshake.ek_x25519_pub.as_bytes());
        raw.extend_from_slice(&(ml_kem_768::CIPHERTEXT_SIZE as u16).to_le_bytes());
        raw.extend_from_slice(&handshake.mlkem_ciphertext.to_bytes());
        raw.extend_from_slice(nonce.as_bytes());
        raw.extend_from_slice(&wrapped_key);
    }
    raw.extend_from_slice(body_nonce.as_bytes());
    raw.extend_from_slice(&body_ciphertext);
    Ok(format!("{SELECTIVE_PREFIX}{}", STANDARD.encode(raw)))
}

/// Open an object locally.  `current_members` is the caller's current
/// membership snapshot; a changed roster fails closed before rendering.  An
/// unselected client gets `Hidden`, not an error/placeholder/oracle.
pub fn receive(
    object: &str,
    recipient: &SelectiveRecipientSecret,
    expected_sender_signing_public: &ed25519::PublicKey,
    current_members: &[SelectiveRecipient],
) -> Result<SelectiveDelivery, SelectiveError> {
    validate_members(current_members)?;
    let raw = decode_object(object)?;
    let header = &raw[..HEADER_BYTES];
    let slot_count = header[33] as usize;
    let slots_end = HEADER_BYTES
        .checked_add(
            slot_count
                .checked_mul(SLOT_BYTES)
                .ok_or(SelectiveError::Malformed)?,
        )
        .ok_or(SelectiveError::Malformed)?;
    if raw.len() < slots_end + aes_gcm::NONCE_SIZE + aes_gcm::TAG_SIZE {
        return Err(SelectiveError::Malformed);
    }
    let mut sender_bytes = [0u8; 32];
    sender_bytes.copy_from_slice(&header[1..33]);
    let sender_x25519_public = x25519::PublicKey::from_bytes(sender_bytes);
    let mut body_key = None;
    for slot in 0..slot_count {
        let offset = HEADER_BYTES + slot * SLOT_BYTES;
        let mut ephemeral = [0u8; 32];
        ephemeral.copy_from_slice(&raw[offset..offset + 32]);
        let ct_offset = offset + 34;
        let ct_len = u16::from_le_bytes([raw[offset + 32], raw[offset + 33]]) as usize;
        if ct_len != ml_kem_768::CIPHERTEXT_SIZE {
            return Err(SelectiveError::Malformed);
        }
        let mut ct = [0u8; ml_kem_768::CIPHERTEXT_SIZE];
        ct.copy_from_slice(&raw[ct_offset..ct_offset + ct_len]);
        let nonce_offset = ct_offset + ct_len;
        let mut nonce_bytes = [0u8; aes_gcm::NONCE_SIZE];
        nonce_bytes.copy_from_slice(&raw[nonce_offset..nonce_offset + aes_gcm::NONCE_SIZE]);
        let wrapped = &raw[nonce_offset + aes_gcm::NONCE_SIZE..offset + SLOT_BYTES];
        let handshake = pqxdh::InitiatorHandshake {
            ek_x25519_pub: x25519::PublicKey::from_bytes(ephemeral),
            mlkem_ciphertext: ml_kem_768::Ciphertext::from_bytes(&ct),
            no_opk: true,
            opk_id: None,
        };
        let session_key = pqxdh::respond(
            &recipient.x25519_secret,
            &recipient.x25519_secret,
            None,
            &recipient.mlkem_secret,
            &sender_x25519_public,
            &handshake,
        )
        .map_err(|_| SelectiveError::Crypto)?;
        let wrap_key = aes_gcm::Key::from_bytes(
            hkdf::derive_32(&[], session_key.as_bytes(), WRAP_INFO)
                .map_err(|_| SelectiveError::Crypto)?,
        );
        let nonce = aes_gcm::Nonce::from_bytes(nonce_bytes);
        if let Ok(key_bytes) = aes_gcm::open(
            &wrap_key,
            &nonce,
            &bound_aad(WRAP_AAD, header, slot as u16),
            wrapped,
        ) {
            if key_bytes.len() == aes_gcm::KEY_SIZE {
                let mut key = [0u8; aes_gcm::KEY_SIZE];
                key.copy_from_slice(&key_bytes);
                body_key = Some(aes_gcm::Key::from_bytes(key));
                break;
            }
        }
    }
    let Some(body_key) = body_key else {
        return Ok(SelectiveDelivery::Hidden {
            surfaces: ClientSurfaces::hidden(),
        });
    };
    let mut nonce_bytes = [0u8; aes_gcm::NONCE_SIZE];
    nonce_bytes.copy_from_slice(&raw[slots_end..slots_end + aes_gcm::NONCE_SIZE]);
    let payload = aes_gcm::open(
        &body_key,
        &aes_gcm::Nonce::from_bytes(nonce_bytes),
        &bound_aad(BODY_AAD, header, 0),
        &raw[slots_end + aes_gcm::NONCE_SIZE..],
    )
    .map_err(|_| SelectiveError::Crypto)?;
    let (plaintext, manifest) = decode_payload(&payload)?;
    if manifest.membership_snapshot != membership_snapshot(current_members) {
        return Err(SelectiveError::MembershipRace);
    }
    if manifest.message_digest != digest(&plaintext) {
        return Err(SelectiveError::MessageBinding);
    }
    let signature = ed25519::Signature::from_bytes(manifest.signature);
    let valid = ed25519::verify(
        expected_sender_signing_public,
        &manifest_signing_bytes(
            manifest.mode,
            &manifest.message_digest,
            &manifest.membership_snapshot,
            &manifest.selected_member_commitments,
        ),
        &signature,
    )
    .map_err(|_| SelectiveError::InvalidSignature)?;
    if !valid {
        return Err(SelectiveError::InvalidSignature);
    }
    Ok(SelectiveDelivery::Selected {
        plaintext,
        marker: SENT_ONLY_SELECTIVE_AUDIENCE_MARKER,
        surfaces: ClientSurfaces::selected(),
        manifest: VerifiedAudienceManifest {
            mode: manifest.mode,
            message_digest: manifest.message_digest,
            membership_snapshot: manifest.membership_snapshot,
            selected_member_commitments: manifest.selected_member_commitments,
        },
    })
}

/// The exact membership commitment used by send and receive.  Exposed so a
/// caller can persist the snapshot with a queued/replayed object.
pub fn membership_snapshot(members: &[SelectiveRecipient]) -> [u8; 32] {
    let mut commitments = members.iter().map(member_commitment).collect::<Vec<_>>();
    commitments.sort_unstable();
    let mut hasher = Sha256::new();
    hasher.update(SNAPSHOT_DOMAIN);
    hasher.update((commitments.len() as u16).to_be_bytes());
    for commitment in commitments {
        hasher.update(commitment);
    }
    hasher.finalize().into()
}

/// Store-side inspection may count opaque slots but never gets a recipient
/// hash or any identity string. This is only for auditing/telemetry tests.
pub fn opaque_slot_count(object: &str) -> Result<usize, SelectiveError> {
    Ok(decode_object(object)?[33] as usize)
}

fn decode_object(object: &str) -> Result<Vec<u8>, SelectiveError> {
    let encoded = object
        .strip_prefix(SELECTIVE_PREFIX)
        .ok_or(SelectiveError::Malformed)?;
    let raw = STANDARD
        .decode(encoded)
        .map_err(|_| SelectiveError::Malformed)?;
    if raw.len() < HEADER_BYTES || raw[0] != VERSION {
        return Err(SelectiveError::Malformed);
    }
    Ok(raw)
}

fn validate_members(members: &[SelectiveRecipient]) -> Result<(), SelectiveError> {
    if members.is_empty() || members.len() > u8::MAX as usize {
        return Err(SelectiveError::EmptyMembership);
    }
    let mut commitments = members.iter().map(member_commitment).collect::<Vec<_>>();
    commitments.sort_unstable();
    if commitments.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(SelectiveError::DuplicateMember);
    }
    Ok(())
}

fn selected_indices(
    member_count: usize,
    audience: &Audience,
) -> Result<Vec<usize>, SelectiveError> {
    let source = match audience {
        Audience::OnlyThese(indices) | Audience::HideFrom(indices) => indices,
    };
    let mut marked = vec![false; member_count];
    for &index in source {
        if index >= member_count {
            return Err(SelectiveError::InvalidAudienceIndex(index));
        }
        marked[index] = true;
    }
    Ok(match audience {
        Audience::OnlyThese(_) => marked
            .iter()
            .enumerate()
            .filter_map(|(index, chosen)| chosen.then_some(index))
            .collect(),
        Audience::HideFrom(_) => marked
            .iter()
            .enumerate()
            .filter_map(|(index, hidden)| (!hidden).then_some(index))
            .collect(),
    })
}

fn member_commitment(member: &SelectiveRecipient) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(MEMBER_DOMAIN);
    hasher.update(member.x25519_pub.as_bytes());
    hasher.update(member.mlkem_pub.to_bytes());
    hasher.finalize().into()
}

fn digest(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

fn bound_aad(label: &[u8], header: &[u8], slot: u16) -> Vec<u8> {
    let mut out = Vec::with_capacity(label.len() + header.len() + 2);
    out.extend_from_slice(label);
    out.extend_from_slice(header);
    out.extend_from_slice(&slot.to_be_bytes());
    out
}

fn manifest_signing_bytes(
    mode: u8,
    message_digest: &[u8; 32],
    snapshot: &[u8; 32],
    selected: &[[u8; 32]],
) -> Vec<u8> {
    let mut out = Vec::with_capacity(MANIFEST_DOMAIN.len() + 1 + 32 + 32 + 2 + selected.len() * 32);
    out.extend_from_slice(MANIFEST_DOMAIN);
    out.push(mode);
    out.extend_from_slice(message_digest);
    out.extend_from_slice(snapshot);
    out.extend_from_slice(&(selected.len() as u16).to_be_bytes());
    for commitment in selected {
        out.extend_from_slice(commitment);
    }
    out
}

fn encode_payload(
    mode: u8,
    plaintext: &[u8],
    snapshot: &[u8; 32],
    selected: &[[u8; 32]],
    signature: &[u8; ed25519::SIGNATURE_SIZE],
) -> Result<Vec<u8>, SelectiveError> {
    let length = u32::try_from(plaintext.len()).map_err(|_| SelectiveError::Malformed)?;
    let count = u8::try_from(selected.len()).map_err(|_| SelectiveError::Malformed)?;
    let mut out = Vec::with_capacity(
        2 + 4 + plaintext.len() + 32 + 1 + selected.len() * 32 + signature.len(),
    );
    out.push(BODY_VERSION);
    out.push(mode);
    out.extend_from_slice(&length.to_be_bytes());
    out.extend_from_slice(plaintext);
    out.extend_from_slice(snapshot);
    out.push(count);
    for commitment in selected {
        out.extend_from_slice(commitment);
    }
    out.extend_from_slice(signature);
    Ok(out)
}

struct DecodedPayload {
    mode: u8,
    membership_snapshot: [u8; 32],
    message_digest: [u8; 32],
    selected_member_commitments: Vec<[u8; 32]>,
    signature: [u8; ed25519::SIGNATURE_SIZE],
}

fn decode_payload(payload: &[u8]) -> Result<(Vec<u8>, DecodedPayload), SelectiveError> {
    if payload.len() < 2 + 4 + 32 + 1 + ed25519::SIGNATURE_SIZE
        || payload[0] != BODY_VERSION
        || !matches!(payload[1], 1 | 2)
    {
        return Err(SelectiveError::Malformed);
    }
    let text_len = u32::from_be_bytes(
        payload[2..6]
            .try_into()
            .map_err(|_| SelectiveError::Malformed)?,
    ) as usize;
    let snapshot_at = 6usize
        .checked_add(text_len)
        .ok_or(SelectiveError::Malformed)?;
    if payload.len() < snapshot_at + 33 + ed25519::SIGNATURE_SIZE {
        return Err(SelectiveError::Malformed);
    }
    let plaintext = payload[6..snapshot_at].to_vec();
    let mut snapshot = [0u8; 32];
    snapshot.copy_from_slice(&payload[snapshot_at..snapshot_at + 32]);
    let count = payload[snapshot_at + 32] as usize;
    let commitments_at = snapshot_at + 33;
    let signature_at = commitments_at
        .checked_add(count.checked_mul(32).ok_or(SelectiveError::Malformed)?)
        .ok_or(SelectiveError::Malformed)?;
    if payload.len() != signature_at + ed25519::SIGNATURE_SIZE {
        return Err(SelectiveError::Malformed);
    }
    let mut selected = Vec::with_capacity(count);
    for item in payload[commitments_at..signature_at].chunks_exact(32) {
        let mut commitment = [0u8; 32];
        commitment.copy_from_slice(item);
        selected.push(commitment);
    }
    let mut signature = [0u8; ed25519::SIGNATURE_SIZE];
    signature.copy_from_slice(&payload[signature_at..]);
    Ok((
        plaintext.clone(),
        DecodedPayload {
            mode: payload[1],
            membership_snapshot: snapshot,
            message_digest: digest(&plaintext),
            selected_member_commitments: selected,
            signature,
        },
    ))
}
