//! Attended IMAP cleanup policy.
//!
//! This module is deliberately adapter-free: it models the credentials,
//! inspection results, deterministic local fixture, and delete-authority checks
//! that a reviewed IMAP adapter must satisfy before it can mutate a mailbox.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

const MAX_HOST_BYTES: usize = 253;
const MAX_USERNAME_BYTES: usize = 320;
const MAX_SECRET_BYTES: usize = 4096;
const MAX_ACCOUNT_ID_BYTES: usize = 128;
const MAX_OWNER_BYTES: usize = 128;
const MAX_MAILBOX_BYTES: usize = 128;
const MAX_MESSAGE_ID_BYTES: usize = 256;
const MAX_BATCH_MESSAGES: usize = 512;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ImapAuthKind {
    Password,
    OAuthBearer,
}

pub enum ImapAuthInput {
    Password {
        username: String,
        password: Zeroizing<String>,
    },
    OAuthBearer {
        username: String,
        bearer_token: Zeroizing<String>,
    },
}

impl<'de> Deserialize<'de> for ImapAuthInput {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        enum RawImapAuthInput {
            Password {
                username: String,
                password: String,
            },
            OAuthBearer {
                username: String,
                bearer_token: String,
            },
        }

        Ok(match RawImapAuthInput::deserialize(deserializer)? {
            RawImapAuthInput::Password { username, password } => Self::Password {
                username,
                password: Zeroizing::new(password),
            },
            RawImapAuthInput::OAuthBearer {
                username,
                bearer_token,
            } => Self::OAuthBearer {
                username,
                bearer_token: Zeroizing::new(bearer_token),
            },
        })
    }
}

impl ImapAuthInput {
    pub fn kind(&self) -> ImapAuthKind {
        match self {
            Self::Password { .. } => ImapAuthKind::Password,
            Self::OAuthBearer { .. } => ImapAuthKind::OAuthBearer,
        }
    }

    pub fn username(&self) -> &str {
        match self {
            Self::Password { username, .. } | Self::OAuthBearer { username, .. } => username,
        }
    }

    #[cfg(test)]
    fn secret_for_test(&self) -> &str {
        match self {
            Self::Password { password, .. } => password.as_str(),
            Self::OAuthBearer { bearer_token, .. } => bearer_token.as_str(),
        }
    }
}

impl fmt::Debug for ImapAuthInput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ImapAuthInput")
            .field("kind", &self.kind())
            .field("username", &"<redacted>")
            .field("secret", &"<redacted>")
            .finish()
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConfigureImapRequest {
    pub owner_osl_user_id: String,
    pub account_id: String,
    pub host: String,
    pub port: u16,
    pub tls_required: bool,
    pub auth: ImapAuthInput,
}

impl fmt::Debug for ConfigureImapRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ConfigureImapRequest")
            .field("owner_osl_user_id", &"<redacted>")
            .field("account_id", &"<redacted>")
            .field("host", &self.host)
            .field("port", &self.port)
            .field("tls_required", &self.tls_required)
            .field("auth", &self.auth)
            .finish()
    }
}

pub struct Imap {
    owner_osl_user_id: String,
    account_id: String,
    host: String,
    port: u16,
    tls_required: bool,
    auth_kind: ImapAuthKind,
    auth_username: String,
    credential: Zeroizing<String>,
}

impl Imap {
    pub fn configure(request: ConfigureImapRequest) -> Result<Self, ImapPolicyError> {
        validate_binding(&request.owner_osl_user_id, MAX_OWNER_BYTES)?;
        validate_binding(&request.account_id, MAX_ACCOUNT_ID_BYTES)?;
        validate_host(&request.host)?;
        if request.port == 0 {
            return Err(ImapPolicyError::InvalidBinding);
        }
        validate_username(request.auth.username())?;
        let auth_kind = request.auth.kind();
        let auth_username = request.auth.username().to_owned();
        let credential = match request.auth {
            ImapAuthInput::Password { password, .. } => {
                validate_secret(password.as_str())?;
                password
            }
            ImapAuthInput::OAuthBearer { bearer_token, .. } => {
                validate_secret(bearer_token.as_str())?;
                bearer_token
            }
        };
        Ok(Self {
            owner_osl_user_id: request.owner_osl_user_id,
            account_id: request.account_id,
            host: request.host,
            port: request.port,
            tls_required: request.tls_required,
            auth_kind,
            auth_username,
            credential,
        })
    }

    pub fn auth_kind(&self) -> ImapAuthKind {
        self.auth_kind
    }

    pub fn owner_osl_user_id(&self) -> &str {
        &self.owner_osl_user_id
    }

    pub fn account_id(&self) -> &str {
        &self.account_id
    }

    pub fn credential_for_imap_adapter(&self) -> &str {
        self.credential.as_str()
    }

    pub fn auth_username_for_imap_adapter(&self) -> &str {
        &self.auth_username
    }
}

impl fmt::Debug for Imap {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Imap")
            .field("owner_osl_user_id", &"<redacted>")
            .field("account_id", &"<redacted>")
            .field("host", &self.host)
            .field("port", &self.port)
            .field("tls_required", &self.tls_required)
            .field("auth_kind", &self.auth_kind)
            .field("auth_username", &"<redacted>")
            .field("credential", &"<redacted>")
            .finish()
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ImapCapability {
    Imap4Rev1,
    StartTls,
    UidPlus,
    Idle,
    Move,
    AuthPlain,
    AuthOAuthBearer,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImapEnumeration {
    pub capabilities: BTreeSet<ImapCapability>,
    pub transport_findings: Vec<ImapTransportFinding>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ImapTransportFinding {
    StartTlsAdvertised,
    TlsRequiredButUnavailable,
    CleartextPasswordAuthOffered,
    OAuthBearerAvailable,
    UidPlusMissing,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImapInspect {
    pub tls_active: bool,
    pub tls_required: bool,
}

impl ImapInspect {
    pub fn enumerate(&self, capability_lines: &[&str]) -> ImapEnumeration {
        let capabilities = parse_capabilities(capability_lines);
        let mut transport_findings = Vec::new();
        if capabilities.contains(&ImapCapability::StartTls) {
            transport_findings.push(ImapTransportFinding::StartTlsAdvertised);
        }
        if self.tls_required
            && !self.tls_active
            && !capabilities.contains(&ImapCapability::StartTls)
        {
            transport_findings.push(ImapTransportFinding::TlsRequiredButUnavailable);
        }
        if !self.tls_active && capabilities.contains(&ImapCapability::AuthPlain) {
            transport_findings.push(ImapTransportFinding::CleartextPasswordAuthOffered);
        }
        if capabilities.contains(&ImapCapability::AuthOAuthBearer) {
            transport_findings.push(ImapTransportFinding::OAuthBearerAvailable);
        }
        if !capabilities.contains(&ImapCapability::UidPlus) {
            transport_findings.push(ImapTransportFinding::UidPlusMissing);
        }
        ImapEnumeration {
            capabilities,
            transport_findings,
        }
    }
}

pub fn parse_capabilities(capability_lines: &[&str]) -> BTreeSet<ImapCapability> {
    let mut capabilities = BTreeSet::new();
    for token in capability_lines
        .iter()
        .flat_map(|line| line.split(|character: char| character.is_ascii_whitespace()))
    {
        let normalized = token
            .trim_matches(|character: char| {
                character == '*' || character == '[' || character == ']'
            })
            .to_ascii_uppercase();
        match normalized.as_str() {
            "IMAP4REV1" => {
                capabilities.insert(ImapCapability::Imap4Rev1);
            }
            "STARTTLS" => {
                capabilities.insert(ImapCapability::StartTls);
            }
            "UIDPLUS" => {
                capabilities.insert(ImapCapability::UidPlus);
            }
            "IDLE" => {
                capabilities.insert(ImapCapability::Idle);
            }
            "MOVE" => {
                capabilities.insert(ImapCapability::Move);
            }
            "AUTH=PLAIN" => {
                capabilities.insert(ImapCapability::AuthPlain);
            }
            "AUTH=OAUTHBEARER" | "AUTH=XOAUTH2" => {
                capabilities.insert(ImapCapability::AuthOAuthBearer);
            }
            _ => {}
        }
    }
    capabilities
}

#[derive(Clone, Eq, PartialEq)]
pub struct ImapMessageSnapshot {
    pub owner_osl_user_id: String,
    pub account_id: String,
    pub mailbox: String,
    pub message_id: String,
    pub uid: u32,
    pub fingerprint: [u8; 32],
    pub authored_by_self: bool,
}

impl fmt::Debug for ImapMessageSnapshot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ImapMessageSnapshot")
            .field("owner_osl_user_id", &"<redacted>")
            .field("account_id", &"<redacted>")
            .field("mailbox", &"<redacted>")
            .field("message_id", &"<redacted>")
            .field("uid", &self.uid)
            .field("fingerprint", &"<redacted>")
            .field("authored_by_self", &self.authored_by_self)
            .finish()
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct PreparedImapDelete {
    pub owner_osl_user_id: String,
    pub account_id: String,
    pub mailbox: String,
    pub message_id: String,
    pub prepared_uid: u32,
    pub fingerprint: [u8; 32],
    pub batch_digest: [u8; 32],
}

impl fmt::Debug for PreparedImapDelete {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PreparedImapDelete")
            .field("owner_osl_user_id", &"<redacted>")
            .field("account_id", &"<redacted>")
            .field("mailbox", &"<redacted>")
            .field("message_id", &"<redacted>")
            .field("prepared_uid", &self.prepared_uid)
            .field("fingerprint", &"<redacted>")
            .field("batch_digest", &"<redacted>")
            .finish()
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct ImapDeleteReceipt {
    pub account_id: String,
    pub mailbox: String,
    pub message_id: String,
    pub deleted_uid: u32,
}

impl fmt::Debug for ImapDeleteReceipt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ImapDeleteReceipt")
            .field("account_id", &"<redacted>")
            .field("mailbox", &"<redacted>")
            .field("message_id", &"<redacted>")
            .field("deleted_uid", &self.deleted_uid)
            .finish()
    }
}

#[derive(Clone, Default, Eq, PartialEq)]
pub struct SeededLocalImapFixture {
    messages: Vec<ImapMessageSnapshot>,
}

impl fmt::Debug for SeededLocalImapFixture {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SeededLocalImapFixture")
            .field("message_count", &self.messages.len())
            .finish()
    }
}

impl SeededLocalImapFixture {
    pub fn scaffold(seed: u64) -> Self {
        let account_id = format!("acct-fixture-{seed:016x}");
        let owner_osl_user_id = format!("owner-fixture-{seed:016x}");
        let mut messages = Vec::new();
        for index in 0..3u32 {
            let message_id = format!("<fixture-{seed:016x}-{index}@local.test>");
            messages.push(ImapMessageSnapshot {
                owner_osl_user_id: owner_osl_user_id.clone(),
                account_id: account_id.clone(),
                mailbox: "INBOX".to_owned(),
                message_id: message_id.clone(),
                uid: 100 + index,
                fingerprint: message_fingerprint(&account_id, "INBOX", &message_id, index),
                authored_by_self: index != 2,
            });
        }
        Self { messages }
    }

    pub fn messages(&self) -> &[ImapMessageSnapshot] {
        &self.messages
    }

    pub fn replace_message(&mut self, message: ImapMessageSnapshot) {
        if let Some(existing) = self.messages.iter_mut().find(|candidate| {
            candidate.account_id == message.account_id
                && candidate.mailbox == message.mailbox
                && candidate.message_id == message.message_id
        }) {
            *existing = message;
        } else {
            self.messages.push(message);
        }
    }
}

#[derive(Clone, Default)]
pub struct ImapMailbox {
    messages: BTreeMap<(String, String, String), ImapMessageSnapshot>,
    deleted: BTreeSet<(String, String, String)>,
}

impl fmt::Debug for ImapMailbox {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ImapMailbox")
            .field("message_count", &self.messages.len())
            .field("deleted_count", &self.deleted.len())
            .finish()
    }
}

impl ImapMailbox {
    pub fn from_fixture(fixture: &SeededLocalImapFixture) -> Self {
        Self::from_messages(fixture.messages.clone())
    }

    pub fn from_messages(messages: Vec<ImapMessageSnapshot>) -> Self {
        Self {
            messages: messages
                .into_iter()
                .map(|message| {
                    (
                        (
                            message.account_id.clone(),
                            message.mailbox.clone(),
                            message.message_id.clone(),
                        ),
                        message,
                    )
                })
                .collect(),
            deleted: BTreeSet::new(),
        }
    }

    pub fn search_message(
        &self,
        account_id: &str,
        mailbox: &str,
        message_id: &str,
    ) -> Option<&ImapMessageSnapshot> {
        self.messages.get(&(
            account_id.to_owned(),
            mailbox.to_owned(),
            message_id.to_owned(),
        ))
    }

    pub fn mutate_message(&mut self, message: ImapMessageSnapshot) {
        self.messages.insert(
            (
                message.account_id.clone(),
                message.mailbox.clone(),
                message.message_id.clone(),
            ),
            message,
        );
    }

    pub fn deleted_count(&self) -> usize {
        self.deleted.len()
    }
}

pub fn prepare_delete(
    mailbox: &ImapMailbox,
    owner_osl_user_id: &str,
    account_id: &str,
    mailbox_name: &str,
    message_id: &str,
) -> Result<PreparedImapDelete, ImapPolicyError> {
    validate_binding(owner_osl_user_id, MAX_OWNER_BYTES)?;
    validate_binding(account_id, MAX_ACCOUNT_ID_BYTES)?;
    validate_binding(mailbox_name, MAX_MAILBOX_BYTES)?;
    validate_binding(message_id, MAX_MESSAGE_ID_BYTES)?;
    let message = mailbox
        .search_message(account_id, mailbox_name, message_id)
        .ok_or(ImapPolicyError::MessageNotFound)?;
    require_message_binding(
        message,
        owner_osl_user_id,
        account_id,
        mailbox_name,
        message_id,
    )?;
    let mut digest_input = Vec::new();
    write_lp(&mut digest_input, owner_osl_user_id.as_bytes());
    write_lp(&mut digest_input, account_id.as_bytes());
    write_lp(&mut digest_input, mailbox_name.as_bytes());
    write_lp(&mut digest_input, message_id.as_bytes());
    digest_input.extend_from_slice(&message.uid.to_be_bytes());
    digest_input.extend_from_slice(&message.fingerprint);
    let batch_digest = Sha256::digest(digest_input).into();
    Ok(PreparedImapDelete {
        owner_osl_user_id: owner_osl_user_id.to_owned(),
        account_id: account_id.to_owned(),
        mailbox: mailbox_name.to_owned(),
        message_id: message_id.to_owned(),
        prepared_uid: message.uid,
        fingerprint: message.fingerprint,
        batch_digest,
    })
}

pub fn delete_prepared(
    mailbox: &mut ImapMailbox,
    prepared: &PreparedImapDelete,
) -> Result<ImapDeleteReceipt, ImapPolicyError> {
    let message = mailbox
        .search_message(
            &prepared.account_id,
            &prepared.mailbox,
            &prepared.message_id,
        )
        .cloned()
        .ok_or(ImapPolicyError::MessageNotFound)?;
    require_message_binding(
        &message,
        &prepared.owner_osl_user_id,
        &prepared.account_id,
        &prepared.mailbox,
        &prepared.message_id,
    )?;
    if message.fingerprint != prepared.fingerprint {
        return Err(ImapPolicyError::FingerprintMismatch);
    }
    mailbox.deleted.insert((
        prepared.account_id.clone(),
        prepared.mailbox.clone(),
        prepared.message_id.clone(),
    ));
    Ok(ImapDeleteReceipt {
        account_id: prepared.account_id.clone(),
        mailbox: prepared.mailbox.clone(),
        message_id: prepared.message_id.clone(),
        deleted_uid: message.uid,
    })
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ImapGrantAuthority {
    Run,
    Attended,
}

#[derive(Clone, Eq, PartialEq)]
pub struct ReviewedAttendedImapBatch {
    owner_osl_user_id: String,
    account_id: String,
    batch_digest: [u8; 32],
    message_count: usize,
}

impl fmt::Debug for ReviewedAttendedImapBatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ReviewedAttendedImapBatch")
            .field("owner_osl_user_id", &"<redacted>")
            .field("account_id", &"<redacted>")
            .field("batch_digest", &"<redacted>")
            .field("message_count", &self.message_count)
            .finish()
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct ImapDeleteGrant {
    pub grant_id: String,
    pub authority: ImapGrantAuthority,
    pub owner_osl_user_id: String,
    pub account_id: String,
    pub batch_digest: [u8; 32],
    pub message_fingerprints: BTreeSet<[u8; 32]>,
    pub phase: ImapDeletePhase,
    pub entitlement: ImapEntitlement,
    pub deadline_unix_ms: i64,
    pub used: bool,
    pub revoked: bool,
}

impl fmt::Debug for ImapDeleteGrant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ImapDeleteGrant")
            .field("grant_id", &"<redacted>")
            .field("authority", &self.authority)
            .field("owner_osl_user_id", &"<redacted>")
            .field("account_id", &"<redacted>")
            .field("batch_digest", &"<redacted>")
            .field(
                "message_fingerprint_count",
                &self.message_fingerprints.len(),
            )
            .field("phase", &self.phase)
            .field("entitlement", &self.entitlement)
            .field("deadline_unix_ms", &self.deadline_unix_ms)
            .field("used", &self.used)
            .field("revoked", &self.revoked)
            .finish()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ImapDeletePhase {
    Reviewed,
    Executing,
    Complete,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ImapEntitlement {
    Free,
    Pro,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ImapDeleteContext {
    pub entitlement: ImapEntitlement,
    pub phase: ImapDeletePhase,
    pub now_unix_ms: i64,
}

#[derive(Clone, Default)]
pub struct AttendedImapDeleteAuthorizer {
    next_sequence: u64,
    used_digests: BTreeSet<[u8; 32]>,
    revoked_grants: BTreeSet<String>,
}

impl fmt::Debug for AttendedImapDeleteAuthorizer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AttendedImapDeleteAuthorizer")
            .field("next_sequence", &self.next_sequence)
            .field("used_digest_count", &self.used_digests.len())
            .field("revoked_grant_count", &self.revoked_grants.len())
            .finish()
    }
}

impl AttendedImapDeleteAuthorizer {
    pub fn authorize_attended_imap_batch(
        &mut self,
        reviewed: ReviewedAttendedImapBatch,
        now_unix_ms: i64,
        ttl_ms: i64,
    ) -> Result<ImapDeleteGrant, ImapPolicyError> {
        if reviewed.message_count == 0 || reviewed.message_count > MAX_BATCH_MESSAGES || ttl_ms <= 0
        {
            return Err(ImapPolicyError::BatchNotReviewed);
        }
        if !self.used_digests.insert(reviewed.batch_digest) {
            return Err(ImapPolicyError::SingleUseAuthorityRequired);
        }
        let sequence = self.next_sequence;
        self.next_sequence = self.next_sequence.saturating_add(1);
        Ok(ImapDeleteGrant {
            grant_id: format!("imap-attended-{sequence:016x}"),
            authority: ImapGrantAuthority::Attended,
            owner_osl_user_id: reviewed.owner_osl_user_id,
            account_id: reviewed.account_id,
            batch_digest: reviewed.batch_digest,
            message_fingerprints: BTreeSet::new(),
            phase: ImapDeletePhase::Reviewed,
            entitlement: ImapEntitlement::Pro,
            deadline_unix_ms: now_unix_ms.saturating_add(ttl_ms),
            used: false,
            revoked: false,
        })
    }

    pub fn revoke_attended_imap_batch(&mut self, grant: &mut ImapDeleteGrant) -> bool {
        if grant.authority != ImapGrantAuthority::Attended {
            return false;
        }
        grant.revoked = true;
        self.revoked_grants.insert(grant.grant_id.clone())
    }

    #[allow(non_snake_case)]
    pub fn authorizeAttendedImapDeleteBatch(
        &mut self,
        reviewed: ReviewedAttendedImapBatch,
        now_unix_ms: i64,
        ttl_ms: i64,
    ) -> Result<ImapDeleteGrant, ImapPolicyError> {
        self.authorize_attended_imap_batch(reviewed, now_unix_ms, ttl_ms)
    }

    #[allow(non_snake_case)]
    pub fn revokeAttendedImapDel(&mut self, grant: &mut ImapDeleteGrant) -> bool {
        self.revoke_attended_imap_batch(grant)
    }
}

#[allow(non_snake_case)]
pub fn authorizeAttendedImapDeleteBatch(
    authorizer: &mut AttendedImapDeleteAuthorizer,
    reviewed: ReviewedAttendedImapBatch,
    now_unix_ms: i64,
    ttl_ms: i64,
) -> Result<ImapDeleteGrant, ImapPolicyError> {
    authorizer.authorize_attended_imap_batch(reviewed, now_unix_ms, ttl_ms)
}

#[allow(non_snake_case)]
pub fn revokeAttendedImapDel(
    authorizer: &mut AttendedImapDeleteAuthorizer,
    grant: &mut ImapDeleteGrant,
) -> bool {
    authorizer.revoke_attended_imap_batch(grant)
}

pub fn authorize_attended_imap_batch_reviewed(
    prepared: &[PreparedImapDelete],
    owner_osl_user_id: &str,
    account_id: &str,
) -> Result<ReviewedAttendedImapBatch, ImapPolicyError> {
    if prepared.is_empty() || prepared.len() > MAX_BATCH_MESSAGES {
        return Err(ImapPolicyError::BatchNotReviewed);
    }
    validate_binding(owner_osl_user_id, MAX_OWNER_BYTES)?;
    validate_binding(account_id, MAX_ACCOUNT_ID_BYTES)?;
    let mut digest_input = Vec::new();
    for item in prepared {
        if item.owner_osl_user_id != owner_osl_user_id || item.account_id != account_id {
            return Err(ImapPolicyError::AccountBindingMismatch);
        }
        digest_input.extend_from_slice(&item.batch_digest);
    }
    let batch_digest = Sha256::digest(digest_input).into();
    Ok(ReviewedAttendedImapBatch {
        owner_osl_user_id: owner_osl_user_id.to_owned(),
        account_id: account_id.to_owned(),
        batch_digest,
        message_count: prepared.len(),
    })
}

pub fn authorize_attended_imap_batch(
    authorizer: &mut AttendedImapDeleteAuthorizer,
    reviewed: ReviewedAttendedImapBatch,
    now_unix_ms: i64,
    ttl_ms: i64,
) -> Result<ImapDeleteGrant, ImapPolicyError> {
    authorizer.authorize_attended_imap_batch(reviewed, now_unix_ms, ttl_ms)
}

pub fn revoke_attended_imap_batch(
    authorizer: &mut AttendedImapDeleteAuthorizer,
    grant: &mut ImapDeleteGrant,
) -> bool {
    authorizer.revoke_attended_imap_batch(grant)
}

pub fn scrub_imap_verify(
    grant: &mut ImapDeleteGrant,
    reviewed: &ReviewedAttendedImapBatch,
) -> Result<(), ImapPolicyError> {
    if grant.authority != ImapGrantAuthority::Attended
        || grant.revoked
        || grant.owner_osl_user_id != reviewed.owner_osl_user_id
        || grant.account_id != reviewed.account_id
        || grant.batch_digest != reviewed.batch_digest
    {
        return Err(ImapPolicyError::AuthorityRefused);
    }
    if grant.used {
        return Err(ImapPolicyError::SingleUseAuthorityRequired);
    }
    grant.used = true;
    Ok(())
}

#[allow(non_snake_case)]
pub fn scrubImapVerify(
    grant: &mut ImapDeleteGrant,
    reviewed: &ReviewedAttendedImapBatch,
) -> Result<(), ImapPolicyError> {
    scrub_imap_verify(grant, reviewed)
}

pub fn still_authorizes_imap_delete(
    context: ImapDeleteContext,
    grant: &ImapDeleteGrant,
    candidate: &PreparedImapDelete,
) -> Result<(), ImapPolicyError> {
    if context.entitlement != ImapEntitlement::Pro || grant.entitlement != ImapEntitlement::Pro {
        return Err(ImapPolicyError::EntitlementRequired);
    }
    if context.phase != ImapDeletePhase::Executing || grant.phase != ImapDeletePhase::Executing {
        return Err(ImapPolicyError::PhaseRefused);
    }
    if context.now_unix_ms > grant.deadline_unix_ms {
        return Err(ImapPolicyError::GrantExpired);
    }
    if grant.revoked {
        return Err(ImapPolicyError::AuthorityRefused);
    }
    if grant.owner_osl_user_id != candidate.owner_osl_user_id
        || grant.account_id != candidate.account_id
    {
        return Err(ImapPolicyError::AccountBindingMismatch);
    }
    if !grant.message_fingerprints.contains(&candidate.fingerprint) {
        return Err(ImapPolicyError::FingerprintMismatch);
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ImapPresence {
    Present,
    Absent,
}

#[derive(Clone, Eq, PartialEq)]
pub struct ImapRunRequest {
    pub owner_osl_user_id: String,
    pub account_id: String,
    pub authority: ImapGrantAuthority,
    pub credential_source: ImapCredentialSource,
    pub presence: ImapPresence,
}

impl fmt::Debug for ImapRunRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ImapRunRequest")
            .field("owner_osl_user_id", &"<redacted>")
            .field("account_id", &"<redacted>")
            .field("authority", &self.authority)
            .field("credential_source", &self.credential_source)
            .field("presence", &self.presence)
            .finish()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ImapCredentialSource {
    OwnerProvisioned,
    Imported,
}

pub fn may_start_imap_run(request: &ImapRunRequest) -> Result<(), ImapPolicyError> {
    validate_binding(&request.owner_osl_user_id, MAX_OWNER_BYTES)?;
    validate_binding(&request.account_id, MAX_ACCOUNT_ID_BYTES)?;
    match (
        request.authority,
        request.credential_source,
        request.presence,
    ) {
        (ImapGrantAuthority::Run, ImapCredentialSource::OwnerProvisioned, _) => Ok(()),
        (_, _, ImapPresence::Present) => Ok(()),
        _ => Err(ImapPolicyError::PresenceRequired),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ImapPolicyError {
    InvalidBinding,
    MessageNotFound,
    AccountBindingMismatch,
    FingerprintMismatch,
    OwnershipRequired,
    BatchNotReviewed,
    SingleUseAuthorityRequired,
    AuthorityRefused,
    EntitlementRequired,
    PhaseRefused,
    GrantExpired,
    PresenceRequired,
}

impl fmt::Display for ImapPolicyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::InvalidBinding => "IMAP request binding is invalid",
            Self::MessageNotFound => "IMAP message was not found",
            Self::AccountBindingMismatch => "IMAP account binding was not confirmed",
            Self::FingerprintMismatch => "IMAP message fingerprint changed",
            Self::OwnershipRequired => "IMAP message is not owned by the active user",
            Self::BatchNotReviewed => "IMAP delete batch was not reviewed",
            Self::SingleUseAuthorityRequired => "IMAP delete authority must be single-use",
            Self::AuthorityRefused => "IMAP delete authority was refused",
            Self::EntitlementRequired => "IMAP delete requires active entitlement",
            Self::PhaseRefused => "IMAP delete is not in the authorized phase",
            Self::GrantExpired => "IMAP delete authority expired",
            Self::PresenceRequired => "IMAP attended presence is required",
        };
        f.write_str(message)
    }
}

impl std::error::Error for ImapPolicyError {}

fn require_message_binding(
    message: &ImapMessageSnapshot,
    owner_osl_user_id: &str,
    account_id: &str,
    mailbox: &str,
    message_id: &str,
) -> Result<(), ImapPolicyError> {
    if !message.authored_by_self {
        return Err(ImapPolicyError::OwnershipRequired);
    }
    if message.owner_osl_user_id != owner_osl_user_id
        || message.account_id != account_id
        || message.mailbox != mailbox
        || message.message_id != message_id
    {
        return Err(ImapPolicyError::AccountBindingMismatch);
    }
    Ok(())
}

fn message_fingerprint(account_id: &str, mailbox: &str, message_id: &str, uid: u32) -> [u8; 32] {
    let mut bytes = Vec::new();
    write_lp(&mut bytes, account_id.as_bytes());
    write_lp(&mut bytes, mailbox.as_bytes());
    write_lp(&mut bytes, message_id.as_bytes());
    bytes.extend_from_slice(&uid.to_be_bytes());
    Sha256::digest(bytes).into()
}

fn validate_host(value: &str) -> Result<(), ImapPolicyError> {
    if value.is_empty()
        || value.len() > MAX_HOST_BYTES
        || value.bytes().any(|byte| {
            !(byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-')) || byte == b' '
        })
    {
        Err(ImapPolicyError::InvalidBinding)
    } else {
        Ok(())
    }
}

fn validate_username(value: &str) -> Result<(), ImapPolicyError> {
    if value.is_empty()
        || value.len() > MAX_USERNAME_BYTES
        || value.contains('\0')
        || value.chars().any(|character| character.is_control())
    {
        Err(ImapPolicyError::InvalidBinding)
    } else {
        Ok(())
    }
}

fn validate_secret(value: &str) -> Result<(), ImapPolicyError> {
    if value.is_empty() || value.len() > MAX_SECRET_BYTES || value.contains('\0') {
        Err(ImapPolicyError::InvalidBinding)
    } else {
        Ok(())
    }
}

fn validate_binding(value: &str, max: usize) -> Result<(), ImapPolicyError> {
    if value.is_empty()
        || value.len() > max
        || value.bytes().any(|byte| {
            !(byte.is_ascii_alphanumeric()
                || matches!(byte, b'-' | b'_' | b'.' | b'@' | b'<' | b'>'))
        })
    {
        Err(ImapPolicyError::InvalidBinding)
    } else {
        Ok(())
    }
}

fn write_lp(target: &mut Vec<u8>, value: &[u8]) {
    target.extend_from_slice(&(value.len() as u32).to_be_bytes());
    target.extend_from_slice(value);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_zeroizing_string(_: &Zeroizing<String>) {}

    fn fixture_and_prepared() -> (ImapMailbox, PreparedImapDelete) {
        let fixture = SeededLocalImapFixture::scaffold(7);
        let first = fixture.messages()[0].clone();
        let mailbox = ImapMailbox::from_fixture(&fixture);
        let prepared = prepare_delete(
            &mailbox,
            &first.owner_osl_user_id,
            &first.account_id,
            &first.mailbox,
            &first.message_id,
        )
        .unwrap();
        (mailbox, prepared)
    }

    #[test]
    fn attended_imap_auth_types_retain_zeroizing_credentials() {
        let secret = "unit-test-imap-password";
        let request: ConfigureImapRequest = serde_json::from_str(&format!(
            r#"{{
                "ownerOslUserId": "owner-local-1",
                "accountId": "acct-local-1",
                "host": "imap.local.test",
                "port": 993,
                "tlsRequired": true,
                "auth": {{
                    "password": {{
                        "username": "local-user@example.test",
                        "password": "{secret}"
                    }}
                }}
            }}"#
        ))
        .unwrap();
        assert_eq!(request.auth.kind(), ImapAuthKind::Password);
        assert_eq!(request.auth.secret_for_test(), secret);
        match &request.auth {
            ImapAuthInput::Password { password, .. } => assert_zeroizing_string(password),
            ImapAuthInput::OAuthBearer { .. } => panic!("expected password auth"),
        }

        let debug = format!("{request:?}");
        assert!(!debug.contains(secret));
        assert!(!debug.contains("local-user@example.test"));
        assert!(!debug.contains("owner-local-1"));
        assert!(!debug.contains("acct-local-1"));

        let imap = Imap::configure(request).unwrap();
        assert_eq!(imap.auth_kind(), ImapAuthKind::Password);
        assert_eq!(imap.owner_osl_user_id(), "owner-local-1");
        assert_eq!(imap.account_id(), "acct-local-1");
        assert_eq!(
            imap.auth_username_for_imap_adapter(),
            "local-user@example.test"
        );
        assert_zeroizing_string(&imap.credential);
        assert_eq!(imap.credential_for_imap_adapter(), secret);
        assert!(!format!("{imap:?}").contains(secret));
    }

    #[test]
    fn attended_imap_inspection_types_parse_transport_findings() {
        let inspect = ImapInspect {
            tls_active: false,
            tls_required: true,
        };
        let enumeration = inspect.enumerate(&[
            "* CAPABILITY IMAP4rev1 STARTTLS AUTH=PLAIN UIDPLUS",
            "a001 OK capability completed",
        ]);

        assert!(enumeration
            .capabilities
            .contains(&ImapCapability::Imap4Rev1));
        assert!(enumeration.capabilities.contains(&ImapCapability::StartTls));
        assert!(enumeration
            .capabilities
            .contains(&ImapCapability::AuthPlain));
        assert!(enumeration.capabilities.contains(&ImapCapability::UidPlus));
        assert!(enumeration
            .transport_findings
            .contains(&ImapTransportFinding::StartTlsAdvertised));
        assert!(enumeration
            .transport_findings
            .contains(&ImapTransportFinding::CleartextPasswordAuthOffered));
        assert!(!enumeration
            .transport_findings
            .contains(&ImapTransportFinding::TlsRequiredButUnavailable));

        let blocked = inspect.enumerate(&["* CAPABILITY IMAP4rev1 AUTH=OAUTHBEARER"]);
        assert!(blocked
            .transport_findings
            .contains(&ImapTransportFinding::TlsRequiredButUnavailable));
        assert!(blocked
            .transport_findings
            .contains(&ImapTransportFinding::UidPlusMissing));
    }

    #[test]
    fn attended_imap_prepare_delete_rechecks_ownership_fingerprint_and_message_id() {
        let (mut mailbox, prepared) = fixture_and_prepared();
        let mut changed = mailbox
            .search_message(
                &prepared.account_id,
                &prepared.mailbox,
                &prepared.message_id,
            )
            .unwrap()
            .clone();
        changed.uid = changed.uid.saturating_add(900);
        changed.fingerprint = message_fingerprint(
            &changed.account_id,
            &changed.mailbox,
            &changed.message_id,
            changed.uid,
        );
        mailbox.mutate_message(changed);

        assert_eq!(
            delete_prepared(&mut mailbox, &prepared),
            Err(ImapPolicyError::FingerprintMismatch)
        );
        assert_eq!(mailbox.deleted_count(), 0);

        let (mut mailbox, prepared) = fixture_and_prepared();
        let mut transferred = mailbox
            .search_message(
                &prepared.account_id,
                &prepared.mailbox,
                &prepared.message_id,
            )
            .unwrap()
            .clone();
        transferred.owner_osl_user_id = "owner-other".to_owned();
        mailbox.mutate_message(transferred);
        assert_eq!(
            delete_prepared(&mut mailbox, &prepared),
            Err(ImapPolicyError::AccountBindingMismatch)
        );
        assert_eq!(mailbox.deleted_count(), 0);

        let (mut mailbox, mut prepared) = fixture_and_prepared();
        prepared.message_id = "<wrong-message@local.test>".to_owned();
        assert_eq!(
            delete_prepared(&mut mailbox, &prepared),
            Err(ImapPolicyError::MessageNotFound)
        );
        assert_eq!(mailbox.deleted_count(), 0);
    }

    #[test]
    fn attended_imap_delete_commands_authorize_revoke_and_verify_batches() {
        let (_mailbox, prepared) = fixture_and_prepared();
        let reviewed = authorize_attended_imap_batch_reviewed(
            std::slice::from_ref(&prepared),
            &prepared.owner_osl_user_id,
            &prepared.account_id,
        )
        .unwrap();
        let mut authorizer = AttendedImapDeleteAuthorizer::default();
        let mut grant =
            authorizeAttendedImapDeleteBatch(&mut authorizer, reviewed.clone(), 10_000, 5_000)
                .unwrap();

        assert_eq!(scrubImapVerify(&mut grant, &reviewed), Ok(()));
        assert_eq!(
            scrubImapVerify(&mut grant, &reviewed),
            Err(ImapPolicyError::SingleUseAuthorityRequired)
        );

        let mut revoke_authorizer = AttendedImapDeleteAuthorizer::default();
        let mut revoked = authorizeAttendedImapDeleteBatch(
            &mut revoke_authorizer,
            reviewed.clone(),
            20_000,
            5_000,
        )
        .unwrap();
        assert!(revokeAttendedImapDel(&mut revoke_authorizer, &mut revoked));
        assert_eq!(
            scrubImapVerify(&mut revoked, &reviewed),
            Err(ImapPolicyError::AuthorityRefused)
        );
    }

    #[test]
    fn seeded_local_imap_fixture_scaffold_is_credential_free_and_deterministic() {
        let first = SeededLocalImapFixture::scaffold(42);
        let second = SeededLocalImapFixture::scaffold(42);
        let other = SeededLocalImapFixture::scaffold(43);

        assert_eq!(first, second);
        assert_ne!(first, other);
        assert_eq!(first.messages().len(), 3);
        assert!(first.messages().iter().all(|message| {
            message.account_id.starts_with("acct-fixture-")
                && message.owner_osl_user_id.starts_with("owner-fixture-")
                && message.mailbox == "INBOX"
        }));

        let debug = format!("{first:?}").to_ascii_lowercase();
        for forbidden in ["password", "token", "secret", "credential", "bearer"] {
            assert!(
                !debug.contains(forbidden),
                "fixture debug leaked {forbidden}"
            );
        }
    }

    #[test]
    fn still_authorizes_imap_delete_rechecks_entitlement_phase_deadline_account_and_fingerprint() {
        let (_mailbox, prepared) = fixture_and_prepared();
        let mut fingerprints = BTreeSet::new();
        fingerprints.insert(prepared.fingerprint);
        let mut grant = ImapDeleteGrant {
            grant_id: "grant-1".to_owned(),
            authority: ImapGrantAuthority::Attended,
            owner_osl_user_id: prepared.owner_osl_user_id.clone(),
            account_id: prepared.account_id.clone(),
            batch_digest: prepared.batch_digest,
            message_fingerprints: fingerprints,
            phase: ImapDeletePhase::Executing,
            entitlement: ImapEntitlement::Pro,
            deadline_unix_ms: 1_500,
            used: false,
            revoked: false,
        };
        let context = ImapDeleteContext {
            entitlement: ImapEntitlement::Pro,
            phase: ImapDeletePhase::Executing,
            now_unix_ms: 1_000,
        };
        assert_eq!(
            still_authorizes_imap_delete(context, &grant, &prepared),
            Ok(())
        );

        let mut free_context = context;
        free_context.entitlement = ImapEntitlement::Free;
        assert_eq!(
            still_authorizes_imap_delete(free_context, &grant, &prepared),
            Err(ImapPolicyError::EntitlementRequired)
        );

        let mut wrong_phase = context;
        wrong_phase.phase = ImapDeletePhase::Reviewed;
        assert_eq!(
            still_authorizes_imap_delete(wrong_phase, &grant, &prepared),
            Err(ImapPolicyError::PhaseRefused)
        );

        let mut expired = context;
        expired.now_unix_ms = 1_501;
        assert_eq!(
            still_authorizes_imap_delete(expired, &grant, &prepared),
            Err(ImapPolicyError::GrantExpired)
        );

        let mut wrong_account = prepared.clone();
        wrong_account.account_id = "acct-other".to_owned();
        assert_eq!(
            still_authorizes_imap_delete(context, &grant, &wrong_account),
            Err(ImapPolicyError::AccountBindingMismatch)
        );

        grant.message_fingerprints.clear();
        assert_eq!(
            still_authorizes_imap_delete(context, &grant, &prepared),
            Err(ImapPolicyError::FingerprintMismatch)
        );
    }

    #[test]
    fn authorize_attended_imap_batch_requires_reviewed_single_use_authority() {
        let (_mailbox, prepared) = fixture_and_prepared();
        let mut authorizer = AttendedImapDeleteAuthorizer::default();
        assert_eq!(
            authorizer.authorize_attended_imap_batch(
                ReviewedAttendedImapBatch {
                    owner_osl_user_id: prepared.owner_osl_user_id.clone(),
                    account_id: prepared.account_id.clone(),
                    batch_digest: prepared.batch_digest,
                    message_count: 0,
                },
                1_000,
                5_000,
            ),
            Err(ImapPolicyError::BatchNotReviewed)
        );

        let reviewed = authorize_attended_imap_batch_reviewed(
            std::slice::from_ref(&prepared),
            &prepared.owner_osl_user_id,
            &prepared.account_id,
        )
        .unwrap();
        let grant =
            authorize_attended_imap_batch(&mut authorizer, reviewed.clone(), 1_000, 5_000).unwrap();
        assert_eq!(grant.authority, ImapGrantAuthority::Attended);
        assert_eq!(grant.phase, ImapDeletePhase::Reviewed);
        assert_eq!(
            authorize_attended_imap_batch(&mut authorizer, reviewed, 1_001, 5_000),
            Err(ImapPolicyError::SingleUseAuthorityRequired)
        );
    }

    #[test]
    fn revoke_attended_imap_batch_revokes_only_attended_authority() {
        let (_mailbox, prepared) = fixture_and_prepared();
        let mut authorizer = AttendedImapDeleteAuthorizer::default();
        let mut run_grant = ImapDeleteGrant {
            grant_id: "run-grant".to_owned(),
            authority: ImapGrantAuthority::Run,
            owner_osl_user_id: prepared.owner_osl_user_id.clone(),
            account_id: prepared.account_id.clone(),
            batch_digest: prepared.batch_digest,
            message_fingerprints: BTreeSet::new(),
            phase: ImapDeletePhase::Executing,
            entitlement: ImapEntitlement::Pro,
            deadline_unix_ms: 2_000,
            used: false,
            revoked: false,
        };
        assert!(!revoke_attended_imap_batch(&mut authorizer, &mut run_grant));
        assert!(!run_grant.revoked);

        let reviewed = authorize_attended_imap_batch_reviewed(
            std::slice::from_ref(&prepared),
            &prepared.owner_osl_user_id,
            &prepared.account_id,
        )
        .unwrap();
        let mut attended = authorizer
            .authorize_attended_imap_batch(reviewed, 1_000, 5_000)
            .unwrap();
        assert!(revoke_attended_imap_batch(&mut authorizer, &mut attended));
        assert!(attended.revoked);
    }

    #[test]
    fn imap_presence_exemption_allows_owner_provisioned_credential_run() {
        let owner_run = ImapRunRequest {
            owner_osl_user_id: "owner-local-1".to_owned(),
            account_id: "acct-local-1".to_owned(),
            authority: ImapGrantAuthority::Run,
            credential_source: ImapCredentialSource::OwnerProvisioned,
            presence: ImapPresence::Absent,
        };
        assert_eq!(may_start_imap_run(&owner_run), Ok(()));

        let imported_run = ImapRunRequest {
            credential_source: ImapCredentialSource::Imported,
            ..owner_run.clone()
        };
        assert_eq!(
            may_start_imap_run(&imported_run),
            Err(ImapPolicyError::PresenceRequired)
        );

        let attended_present = ImapRunRequest {
            authority: ImapGrantAuthority::Attended,
            credential_source: ImapCredentialSource::Imported,
            presence: ImapPresence::Present,
            ..owner_run
        };
        assert_eq!(may_start_imap_run(&attended_present), Ok(()));
    }

    #[test]
    fn imap_grant_authority_distinguishes_run_from_attended() {
        assert_ne!(ImapGrantAuthority::Run, ImapGrantAuthority::Attended);
        assert_eq!(
            serde_json::to_string(&ImapGrantAuthority::Run).unwrap(),
            "\"run\""
        );
        assert_eq!(
            serde_json::to_string(&ImapGrantAuthority::Attended).unwrap(),
            "\"attended\""
        );
        assert_eq!(
            serde_json::from_str::<ImapGrantAuthority>("\"run\"").unwrap(),
            ImapGrantAuthority::Run
        );
        assert_eq!(
            serde_json::from_str::<ImapGrantAuthority>("\"attended\"").unwrap(),
            ImapGrantAuthority::Attended
        );
    }
}
