//! Signed, recipient-bound invitations for existing group chats and Enclaves.
//!
//! A pending invitation is not a membership mutation.  The only operation
//! which changes a roster or emits a new place key is `accept`, and it verifies
//! the invitation's place, recipient, roster version and every required
//! signature while holding the durable authority lock.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub const GROUP_MEMBER_CAP: usize = 20;
pub const PROPOSED_ONE_OF_TWO: &str = "PROPOSED · 1 OF 2";

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InvitePlaceKind {
    Group,
    Enclave,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct InviteMember {
    pub id: String,
    pub display_name: String,
    /// A stable role identifier, not a display label.  Renaming a role never
    /// changes invitation authority.
    pub role_id: Option<String>,
    pub direct_invite: bool,
    pub invite_approval: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct InviteRoster {
    pub place_id: String,
    pub place_name: String,
    pub kind: InvitePlaceKind,
    pub members: BTreeMap<String, InviteMember>,
    pub roster_version: u64,
    pub key_epoch: u64,
    pub rekey_count: u64,
    pub roster_signature: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InvitationState {
    Proposed,
    Ready,
    Declined,
    Accepted,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SignedMembershipInvitation {
    pub id: String,
    pub place_id: String,
    pub place_name: String,
    pub kind: InvitePlaceKind,
    pub inviter_id: String,
    pub inviter_name: String,
    pub recipient_id: String,
    pub recipient_name: String,
    pub roster_version: u64,
    pub state: InvitationState,
    /// The proposer/direct inviter's signature.  For a proposed Enclave
    /// invite, this is the ordinary-member half of the two required approvals.
    pub inviter_signature: String,
    pub approver_id: Option<String>,
    pub approver_signature: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvitationCard {
    pub invitation_id: String,
    pub inviter_name: String,
    pub place_name: String,
    pub recipient_name: String,
    pub roster_version: u64,
    pub status: String,
}

/// This projection is consumed by the person-plus control.  It intentionally
/// shows who invited whom and where before accepting; no HTML path can present
/// an invitation as an already-added member.
pub fn person_plus_control_markup(card: &InvitationCard) -> String {
    format!(
        "<section class=\"place-invitation\" data-person-plus-control data-invitation-id=\"{}\"><p><strong>{}</strong> invited <strong>{}</strong> to <strong>{}</strong>.</p><p data-roster-version=\"{}\">Roster version {}</p><p data-invitation-status>{}</p><button type=\"button\" data-accept-place-invitation>Accept invitation</button><button type=\"button\" data-decline-place-invitation>Decline</button></section>",
        escape_html(&card.invitation_id),
        escape_html(&card.inviter_name),
        escape_html(&card.recipient_name),
        escape_html(&card.place_name),
        card.roster_version,
        card.roster_version,
        escape_html(&card.status),
    )
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct PersistedInvitations {
    root_secret: String,
    next_invitation: u64,
    places: BTreeMap<String, InviteRoster>,
    invitations: BTreeMap<String, SignedMembershipInvitation>,
}

#[derive(Clone)]
pub struct PlaceInvitationService {
    path: PathBuf,
    state: Arc<Mutex<PersistedInvitations>>,
}

#[derive(Clone)]
pub struct PlaceInvitationClient {
    service: PlaceInvitationService,
    member_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InvitationError {
    UnknownPlace(String),
    UnknownInvitation(String),
    NotCurrentMember(String),
    NotRecipient,
    RecipientAlreadyMember,
    GroupFull,
    NeedsApproval,
    ApprovalAuthorityRequired,
    StaleRoster,
    CrossPlace,
    Replay,
    Declined,
    InvalidSignature,
    InvalidState(String),
    Persistence(String),
}

impl std::fmt::Display for InvitationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownPlace(id) => write!(f, "Unknown place {id}"),
            Self::UnknownInvitation(id) => write!(f, "Unknown invitation {id}"),
            Self::NotCurrentMember(id) => write!(f, "{id} is not a current member"),
            Self::NotRecipient => f.write_str("This invitation belongs to another recipient"),
            Self::RecipientAlreadyMember => f.write_str("The recipient is already a member"),
            Self::GroupFull => f.write_str("Group chats hold at most 20 people"),
            Self::NeedsApproval => f.write_str("This Enclave invitation still needs approval"),
            Self::ApprovalAuthorityRequired => f.write_str("A role-id holder with invite approval authority must approve"),
            Self::StaleRoster => f.write_str("This invitation was made for a stale roster version"),
            Self::CrossPlace => f.write_str("This invitation belongs to another place"),
            Self::Replay => f.write_str("This invitation has already been used"),
            Self::Declined => f.write_str("This invitation was declined"),
            Self::InvalidSignature => f.write_str("Invitation signature is invalid"),
            Self::InvalidState(detail) => write!(f, "Invitation state is invalid: {detail}"),
            Self::Persistence(detail) => write!(f, "Invitation persistence failed: {detail}"),
        }
    }
}

impl std::error::Error for InvitationError {}

impl PlaceInvitationService {
    pub fn create(path: impl AsRef<Path>, root_secret: impl Into<String>) -> Result<Self, InvitationError> {
        let path = path.as_ref().to_path_buf();
        let state = PersistedInvitations { root_secret: root_secret.into(), next_invitation: 1, places: BTreeMap::new(), invitations: BTreeMap::new() };
        persist(&path, &state)?;
        Ok(Self { path, state: Arc::new(Mutex::new(state)) })
    }

    pub fn restart(path: impl AsRef<Path>) -> Result<Self, InvitationError> {
        let path = path.as_ref().to_path_buf();
        let bytes = fs::read(&path).map_err(|e| InvitationError::Persistence(e.to_string()))?;
        let state: PersistedInvitations = serde_json::from_slice(&bytes).map_err(|e| InvitationError::InvalidState(e.to_string()))?;
        audit(&state)?;
        Ok(Self { path, state: Arc::new(Mutex::new(state)) })
    }

    pub fn client(&self, member_id: impl Into<String>) -> PlaceInvitationClient {
        PlaceInvitationClient { service: self.clone(), member_id: member_id.into() }
    }

    pub fn create_place(&self, place_id: &str, place_name: &str, kind: InvitePlaceKind, creator: InviteMember) -> Result<(), InvitationError> {
        let mut state = self.state.lock().expect("invitation state mutex poisoned");
        if state.places.contains_key(place_id) { return Err(InvitationError::InvalidState(format!("duplicate place {place_id}"))); }
        let mut members = BTreeMap::new();
        members.insert(creator.id.clone(), creator);
        let mut roster = InviteRoster { place_id: place_id.to_owned(), place_name: place_name.to_owned(), kind, members, roster_version: 1, key_epoch: 1, rekey_count: 0, roster_signature: String::new() };
        sign_roster(&state.root_secret, &mut roster)?;
        state.places.insert(place_id.to_owned(), roster);
        persist(&self.path, &state)
    }

    pub fn roster(&self, place_id: &str) -> Result<InviteRoster, InvitationError> {
        self.state.lock().expect("invitation state mutex poisoned").places.get(place_id).cloned().ok_or_else(|| InvitationError::UnknownPlace(place_id.to_owned()))
    }

    pub fn invitation(&self, invitation_id: &str) -> Result<SignedMembershipInvitation, InvitationError> {
        self.state.lock().expect("invitation state mutex poisoned").invitations.get(invitation_id).cloned().ok_or_else(|| InvitationError::UnknownInvitation(invitation_id.to_owned()))
    }

    pub fn invitation_card(&self, invitation_id: &str) -> Result<InvitationCard, InvitationError> {
        let invite = self.invitation(invitation_id)?;
        let status = invitation_status(&invite);
        Ok(InvitationCard { invitation_id: invite.id, inviter_name: invite.inviter_name, place_name: invite.place_name, recipient_name: invite.recipient_name, roster_version: invite.roster_version, status })
    }

    pub fn persisted_bytes(&self) -> Result<Vec<u8>, InvitationError> { fs::read(&self.path).map_err(|e| InvitationError::Persistence(e.to_string())) }
}

impl PlaceInvitationClient {
    pub fn invite(&self, place_id: &str, recipient_id: &str, recipient_name: &str) -> Result<SignedMembershipInvitation, InvitationError> {
        let mut state = self.service.state.lock().expect("invitation state mutex poisoned");
        let roster = state.places.get(place_id).cloned().ok_or_else(|| InvitationError::UnknownPlace(place_id.to_owned()))?;
        let inviter = roster.members.get(&self.member_id).cloned().ok_or_else(|| InvitationError::NotCurrentMember(self.member_id.clone()))?;
        if roster.members.contains_key(recipient_id) { return Err(InvitationError::RecipientAlreadyMember); }
        if roster.kind == InvitePlaceKind::Group && roster.members.len() >= GROUP_MEMBER_CAP { return Err(InvitationError::GroupFull); }
        let id = format!("invite-{}", state.next_invitation);
        state.next_invitation += 1;
        let state_kind = if roster.kind == InvitePlaceKind::Enclave && !inviter.direct_invite { InvitationState::Proposed } else { InvitationState::Ready };
        let mut invite = SignedMembershipInvitation {
            id: id.clone(), place_id: roster.place_id.clone(), place_name: roster.place_name.clone(), kind: roster.kind, inviter_id: inviter.id.clone(), inviter_name: inviter.display_name.clone(), recipient_id: recipient_id.to_owned(), recipient_name: recipient_name.to_owned(), roster_version: roster.roster_version, state: state_kind, inviter_signature: String::new(), approver_id: None, approver_signature: None,
        };
        invite.inviter_signature = sign_invitation(&state.root_secret, &inviter.id, &invite)?;
        state.invitations.insert(id, invite.clone());
        persist(&self.service.path, &state)?;
        Ok(invite)
    }

    pub fn approve(&self, invitation_id: &str) -> Result<SignedMembershipInvitation, InvitationError> {
        let mut state = self.service.state.lock().expect("invitation state mutex poisoned");
        let mut invite = state.invitations.get(invitation_id).cloned().ok_or_else(|| InvitationError::UnknownInvitation(invitation_id.to_owned()))?;
        if invite.state != InvitationState::Proposed { return Err(match invite.state { InvitationState::Accepted => InvitationError::Replay, InvitationState::Declined => InvitationError::Declined, _ => InvitationError::NeedsApproval }); }
        let roster = state.places.get(&invite.place_id).cloned().ok_or_else(|| InvitationError::UnknownPlace(invite.place_id.clone()))?;
        if invite.roster_version != roster.roster_version { return Err(InvitationError::StaleRoster); }
        verify_inviter_signature(&state.root_secret, &roster, &invite)?;
        let approver = roster.members.get(&self.member_id).ok_or_else(|| InvitationError::NotCurrentMember(self.member_id.clone()))?;
        if !approver.invite_approval || approver.role_id.is_none() { return Err(InvitationError::ApprovalAuthorityRequired); }
        if approver.id == invite.inviter_id { return Err(InvitationError::ApprovalAuthorityRequired); }
        invite.approver_id = Some(approver.id.clone());
        invite.approver_signature = Some(sign_invitation(&state.root_secret, &approver.id, &invite)?);
        invite.state = InvitationState::Ready;
        state.invitations.insert(invitation_id.to_owned(), invite.clone());
        persist(&self.service.path, &state)?;
        Ok(invite)
    }

    pub fn decline(&self, invitation_id: &str) -> Result<(), InvitationError> {
        let mut state = self.service.state.lock().expect("invitation state mutex poisoned");
        let invite = state.invitations.get_mut(invitation_id).ok_or_else(|| InvitationError::UnknownInvitation(invitation_id.to_owned()))?;
        if invite.recipient_id != self.member_id { return Err(InvitationError::NotRecipient); }
        if invite.state == InvitationState::Accepted { return Err(InvitationError::Replay); }
        if invite.state == InvitationState::Declined { return Err(InvitationError::Declined); }
        invite.state = InvitationState::Declined;
        persist(&self.service.path, &state)
    }

    pub fn accept(&self, invitation_id: &str, displayed_place_id: &str) -> Result<InviteRoster, InvitationError> {
        let mut state = self.service.state.lock().expect("invitation state mutex poisoned");
        let mut invite = state.invitations.get(invitation_id).cloned().ok_or_else(|| InvitationError::UnknownInvitation(invitation_id.to_owned()))?;
        if invite.place_id != displayed_place_id { return Err(InvitationError::CrossPlace); }
        if invite.recipient_id != self.member_id { return Err(InvitationError::NotRecipient); }
        match invite.state { InvitationState::Proposed => return Err(InvitationError::NeedsApproval), InvitationState::Declined => return Err(InvitationError::Declined), InvitationState::Accepted => return Err(InvitationError::Replay), InvitationState::Ready => {} }
        let current = state.places.get(&invite.place_id).cloned().ok_or_else(|| InvitationError::UnknownPlace(invite.place_id.clone()))?;
        ensure_current(&state.root_secret, &current, &invite)?;
        if current.members.contains_key(&invite.recipient_id) { return Err(InvitationError::RecipientAlreadyMember); }
        if current.kind == InvitePlaceKind::Group && current.members.len() >= GROUP_MEMBER_CAP { return Err(InvitationError::GroupFull); }
        let mut next = current;
        next.members.insert(invite.recipient_id.clone(), InviteMember { id: invite.recipient_id.clone(), display_name: invite.recipient_name.clone(), role_id: None, direct_invite: false, invite_approval: false });
        next.roster_version += 1;
        next.key_epoch += 1;
        next.rekey_count += 1;
        sign_roster(&state.root_secret, &mut next)?;
        invite.state = InvitationState::Accepted;
        state.places.insert(next.place_id.clone(), next.clone());
        state.invitations.insert(invitation_id.to_owned(), invite);
        persist(&self.service.path, &state)?;
        Ok(next)
    }
}

fn ensure_current(secret: &str, roster: &InviteRoster, invite: &SignedMembershipInvitation) -> Result<(), InvitationError> {
    if invite.roster_version != roster.roster_version { return Err(InvitationError::StaleRoster); }
    verify_invitation(secret, roster, invite)
}

fn verify_invitation(secret: &str, roster: &InviteRoster, invite: &SignedMembershipInvitation) -> Result<(), InvitationError> {
    let inviter = verify_inviter_signature(secret, roster, invite)?;
    if invite.kind == InvitePlaceKind::Enclave && !inviter.direct_invite {
        let approver_id = invite.approver_id.as_deref().ok_or(InvitationError::NeedsApproval)?;
        let signature = invite.approver_signature.as_deref().ok_or(InvitationError::NeedsApproval)?;
        let approver = roster.members.get(approver_id).ok_or_else(|| InvitationError::NotCurrentMember(approver_id.to_owned()))?;
        if !approver.invite_approval || approver.role_id.is_none() || approver.id == inviter.id { return Err(InvitationError::ApprovalAuthorityRequired); }
        if sign_invitation(secret, approver_id, invite)? != signature { return Err(InvitationError::InvalidSignature); }
    }
    Ok(())
}

fn verify_inviter_signature<'a>(secret: &str, roster: &'a InviteRoster, invite: &SignedMembershipInvitation) -> Result<&'a InviteMember, InvitationError> {
    let inviter = roster.members.get(&invite.inviter_id).ok_or_else(|| InvitationError::NotCurrentMember(invite.inviter_id.clone()))?;
    if sign_invitation(secret, &inviter.id, invite)? != invite.inviter_signature { return Err(InvitationError::InvalidSignature); }
    Ok(inviter)
}

fn invitation_status(invite: &SignedMembershipInvitation) -> String {
    match invite.state {
        InvitationState::Proposed => PROPOSED_ONE_OF_TWO.to_owned(),
        InvitationState::Ready => "READY TO ACCEPT".to_owned(),
        InvitationState::Declined => "DECLINED".to_owned(),
        InvitationState::Accepted => "ACCEPTED".to_owned(),
    }
}

fn sign_roster(secret: &str, roster: &mut InviteRoster) -> Result<(), InvitationError> {
    roster.roster_signature.clear();
    roster.roster_signature = mac(secret.as_bytes(), &serde_json::to_vec(roster).map_err(|e| InvitationError::InvalidState(e.to_string()))?);
    Ok(())
}

fn sign_invitation(secret: &str, signer_id: &str, invite: &SignedMembershipInvitation) -> Result<String, InvitationError> {
    // Approval and terminal state are deliberately not covered here: they are
    // mutable workflow evidence.  The signed offer itself is immutable and
    // binds the recipient, place and exact roster version.
    let canonical = serde_json::to_vec(&(
        &invite.id,
        &invite.place_id,
        &invite.place_name,
        invite.kind,
        &invite.inviter_id,
        &invite.inviter_name,
        &invite.recipient_id,
        &invite.recipient_name,
        invite.roster_version,
    )).map_err(|e| InvitationError::InvalidState(e.to_string()))?;
    Ok(mac(&derive_member_key(secret, signer_id), &canonical))
}

fn derive_member_key(secret: &str, member_id: &str) -> Vec<u8> { mac(secret.as_bytes(), member_id.as_bytes()).into_bytes() }

/// Minimal HMAC-SHA-256 so invitation signatures cannot be forged by merely
/// knowing public invitation fields.
fn mac(key: &[u8], message: &[u8]) -> String {
    const BLOCK: usize = 64;
    let mut normalized = [0_u8; BLOCK];
    if key.len() > BLOCK { normalized[..32].copy_from_slice(&Sha256::digest(key)); } else { normalized[..key.len()].copy_from_slice(key); }
    let mut inner = Sha256::new();
    inner.update(normalized.map(|byte| byte ^ 0x36));
    inner.update(message);
    let inner = inner.finalize();
    let mut outer = Sha256::new();
    outer.update(normalized.map(|byte| byte ^ 0x5c));
    outer.update(inner);
    hex::encode(outer.finalize())
}

fn audit(state: &PersistedInvitations) -> Result<(), InvitationError> {
    for roster in state.places.values() {
        let mut unsigned = roster.clone();
        let signature = std::mem::take(&mut unsigned.roster_signature);
        if signature != mac(state.root_secret.as_bytes(), &serde_json::to_vec(&unsigned).map_err(|e| InvitationError::InvalidState(e.to_string()))?) { return Err(InvitationError::InvalidSignature); }
        if roster.kind == InvitePlaceKind::Group && roster.members.len() > GROUP_MEMBER_CAP { return Err(InvitationError::GroupFull); }
    }
    for invite in state.invitations.values() {
        let roster = state.places.get(&invite.place_id).ok_or_else(|| InvitationError::UnknownPlace(invite.place_id.clone()))?;
        // Accepted/declined invitations may refer to an old version, but their
        // signatures must still remain intact after a restart.
        let inviter = roster.members.get(&invite.inviter_id).ok_or_else(|| InvitationError::NotCurrentMember(invite.inviter_id.clone()))?;
        if sign_invitation(&state.root_secret, &inviter.id, invite)? != invite.inviter_signature { return Err(InvitationError::InvalidSignature); }
    }
    Ok(())
}

fn persist(path: &Path, state: &PersistedInvitations) -> Result<(), InvitationError> {
    if let Some(parent) = path.parent() { fs::create_dir_all(parent).map_err(|e| InvitationError::Persistence(e.to_string()))?; }
    let temporary = path.with_extension("invite.tmp");
    fs::write(&temporary, serde_json::to_vec(state).map_err(|e| InvitationError::InvalidState(e.to_string()))?).map_err(|e| InvitationError::Persistence(e.to_string()))?;
    fs::rename(temporary, path).map_err(|e| InvitationError::Persistence(e.to_string()))
}

fn escape_html(value: &str) -> String {
    value.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&#39;")
}
