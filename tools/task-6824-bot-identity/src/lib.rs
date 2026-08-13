//! Durable Enclave bot authority for TASK 6824.
//!
//! Bot keys inhabit a domain separate from people. Enrollment needs both an
//! owner-signed membership action and proof that the declared bot holds its
//! key for the approved package. Every later post or command is signed by that
//! key and bound to its authenticated session, package, scope and sequence.

use crypto::ed25519;
use serde::{Deserialize, Serialize};
use sha2::{Digest as ShaDigest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

const BOT_ID_DOMAIN: &[u8] = b"OSL/enclave-bot-id/v1\0";
const OWNER_DOMAIN: &[u8] = b"OSL/enclave-bot-owner-action/v1\0";
const CHALLENGE_DOMAIN: &[u8] = b"OSL/enclave-bot-challenge/v1\0";
const ACTION_DOMAIN: &[u8] = b"OSL/enclave-bot-action/v1\0";
type PublicKey = [u8; 32];
type Signature = [u8; 64];

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BotId([u8; 32]);

impl BotId {
    fn from_key(key: &PublicKey) -> Self {
        Self(hash(BOT_ID_DOMAIN, &[key]))
    }
    pub fn hex(self) -> String {
        hex::encode(self.0)
    }
}

impl fmt::Display for BotId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&hex::encode(self.0))
    }
}

impl Serialize for BotId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&hex::encode(self.0))
    }
}

impl<'de> Deserialize<'de> for BotId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let encoded = String::deserialize(deserializer)?;
        let bytes = hex::decode(encoded).map_err(serde::de::Error::custom)?;
        let value: [u8; 32] = bytes
            .try_into()
            .map_err(|_| serde::de::Error::custom("bot id must be 32 bytes"))?;
        Ok(Self(value))
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PackageDigest([u8; 32]);

impl PackageDigest {
    pub fn sha256(package: &[u8]) -> Self {
        Self(hash(b"OSL/enclave-bot-package/v1\0", &[package]))
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct HumanIdentity {
    id: String,
    public_key: PublicKey,
}

#[derive(Clone)]
pub struct HumanCredentials {
    identity: HumanIdentity,
    secret: ed25519::SecretKey,
}

impl HumanCredentials {
    pub fn generate(id: impl Into<String>) -> Self {
        let (secret, public) = ed25519::generate_keypair();
        Self {
            identity: HumanIdentity {
                id: id.into(),
                public_key: *public.as_bytes(),
            },
            secret,
        }
    }
    pub fn public_identity(&self) -> HumanIdentity {
        self.identity.clone()
    }
    pub fn public_key(&self) -> PublicKey {
        self.identity.public_key
    }
    pub fn human_id(&self) -> &str {
        &self.identity.id
    }

    /// Test/audit seam: produces a human-signed object carrying a bot id. The
    /// authority detects the person key before normal bot verification.
    pub fn sign_bot_action_for_test(
        &self,
        bot_id: BotId,
        session: &BotSession,
        digest: PackageDigest,
        sequence: u64,
        action: BotAction,
    ) -> SignedBotAction {
        sign_action(
            bot_id,
            self.identity.public_key,
            &self.secret,
            session,
            digest,
            sequence,
            action,
        )
    }
}

#[derive(Clone)]
pub struct BotCredentials {
    id: BotId,
    secret: ed25519::SecretKey,
    public: PublicKey,
}

impl BotCredentials {
    pub fn generate(_local_name: impl Into<String>) -> Self {
        let (secret, public) = ed25519::generate_keypair();
        let public = *public.as_bytes();
        Self {
            id: BotId::from_key(&public),
            secret,
            public,
        }
    }
    pub fn bot_id(&self) -> BotId {
        self.id
    }
    pub fn public_key(&self) -> PublicKey {
        self.public
    }
    pub fn answer_challenge(
        &self,
        challenge: &BotChallenge,
        digest: PackageDigest,
    ) -> ChallengeResponse {
        let bytes = challenge_bytes(challenge, digest);
        ChallengeResponse {
            bot_id: self.id,
            challenge_id: challenge.id,
            digest,
            signer_public: self.public,
            signature: *ed25519::sign(&self.secret, &bytes).as_bytes(),
        }
    }
    pub fn sign_action(
        &self,
        session: &BotSession,
        digest: PackageDigest,
        sequence: u64,
        action: BotAction,
    ) -> SignedBotAction {
        sign_action(
            self.id,
            self.public,
            &self.secret,
            session,
            digest,
            sequence,
            action,
        )
    }
    pub fn sign_action_as(
        &self,
        claimed: BotId,
        session: &BotSession,
        digest: PackageDigest,
        sequence: u64,
        action: BotAction,
    ) -> SignedBotAction {
        sign_action(
            claimed,
            self.public,
            &self.secret,
            session,
            digest,
            sequence,
            action,
        )
    }
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct BotScope {
    kind: ActionKind,
    name: String,
}

impl BotScope {
    pub fn post(name: impl Into<String>) -> Self {
        Self {
            kind: ActionKind::Post,
            name: name.into(),
        }
    }
    pub fn command(name: impl Into<String>) -> Self {
        Self {
            kind: ActionKind::Command,
            name: name.into(),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
enum ActionKind {
    Post,
    Command,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BotAction {
    kind: ActionKind,
    scope: String,
    body: Vec<u8>,
}

impl BotAction {
    pub fn post(scope: impl Into<String>, body: impl AsRef<[u8]>) -> Self {
        Self {
            kind: ActionKind::Post,
            scope: scope.into(),
            body: body.as_ref().to_vec(),
        }
    }
    pub fn command(scope: impl Into<String>, body: impl AsRef<[u8]>) -> Self {
        Self {
            kind: ActionKind::Command,
            scope: scope.into(),
            body: body.as_ref().to_vec(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BotDeclaration {
    id: BotId,
    owner: String,
    public: PublicKey,
    digest: PackageDigest,
    scopes: BTreeSet<BotScope>,
}

impl BotDeclaration {
    pub fn new(
        id: BotId,
        owner: impl Into<String>,
        public: PublicKey,
        digest: PackageDigest,
        scopes: Vec<BotScope>,
    ) -> Self {
        Self {
            id,
            owner: owner.into(),
            public,
            digest,
            scopes: scopes.into_iter().collect(),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum BotLifecycle {
    Enabled,
    Disabled,
    Removed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BotMembership {
    id: BotId,
    owner: String,
    public: PublicKey,
    digest: PackageDigest,
    lifecycle: BotLifecycle,
    epoch: u64,
    scopes: BTreeSet<BotScope>,
}

impl BotMembership {
    pub fn bot_id(&self) -> BotId {
        self.id
    }
    pub fn owner_id(&self) -> &str {
        &self.owner
    }
    pub fn public_key(&self) -> PublicKey {
        self.public
    }
    pub fn package_digest(&self) -> PackageDigest {
        self.digest
    }
    pub fn lifecycle(&self) -> BotLifecycle {
        self.lifecycle
    }
    pub fn membership_epoch(&self) -> u64 {
        self.epoch
    }
    pub fn scopes(&self) -> &BTreeSet<BotScope> {
        &self.scopes
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum OwnerOperation {
    Add(BotDeclaration),
    Disable(BotId),
    Remove(BotId),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SignedOwnerAction {
    enclave: String,
    epoch: u64,
    actor: String,
    nonce: [u8; 32],
    operation: OwnerOperation,
    signature: Signature,
}

impl SignedOwnerAction {
    pub fn add_bot(
        enclave: &str,
        epoch: u64,
        declaration: BotDeclaration,
        signer: &HumanCredentials,
    ) -> Self {
        Self::sign(enclave, epoch, OwnerOperation::Add(declaration), signer)
    }
    pub fn disable_bot(enclave: &str, epoch: u64, id: BotId, signer: &HumanCredentials) -> Self {
        Self::sign(enclave, epoch, OwnerOperation::Disable(id), signer)
    }
    pub fn remove_bot(enclave: &str, epoch: u64, id: BotId, signer: &HumanCredentials) -> Self {
        Self::sign(enclave, epoch, OwnerOperation::Remove(id), signer)
    }
    fn sign(
        enclave: &str,
        epoch: u64,
        operation: OwnerOperation,
        signer: &HumanCredentials,
    ) -> Self {
        let mut value = Self {
            enclave: enclave.into(),
            epoch,
            actor: signer.human_id().into(),
            nonce: random(),
            operation,
            signature: [0; 64],
        };
        value.signature = *ed25519::sign(&signer.secret, &value.signing_bytes()).as_bytes();
        value
    }
    fn signing_bytes(&self) -> Vec<u8> {
        let mut out = OWNER_DOMAIN.to_vec();
        put_str(&mut out, &self.enclave);
        out.extend_from_slice(&self.epoch.to_be_bytes());
        put_str(&mut out, &self.actor);
        put(&mut out, &self.nonce);
        match &self.operation {
            OwnerOperation::Add(d) => {
                out.push(1);
                put(&mut out, &d.id.0);
                put_str(&mut out, &d.owner);
                put(&mut out, &d.public);
                put(&mut out, &d.digest.0);
                out.extend_from_slice(&(d.scopes.len() as u64).to_be_bytes());
                for scope in &d.scopes {
                    out.push(scope.kind as u8);
                    put_str(&mut out, &scope.name);
                }
            }
            OwnerOperation::Disable(id) => {
                out.push(2);
                put(&mut out, &id.0);
            }
            OwnerOperation::Remove(id) => {
                out.push(3);
                put(&mut out, &id.0);
            }
        }
        out
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ChallengePurpose {
    Enroll,
    Authenticate { generation: u64 },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BotChallenge {
    enclave_hash: [u8; 32],
    bot_id: BotId,
    id: [u8; 32],
    digest: PackageDigest,
    purpose: ChallengePurpose,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChallengeResponse {
    bot_id: BotId,
    challenge_id: [u8; 32],
    digest: PackageDigest,
    signer_public: PublicKey,
    signature: Signature,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BotSession {
    bot_id: BotId,
    generation: u64,
    session_id: [u8; 32],
}

impl BotSession {
    pub fn session_id(&self) -> [u8; 32] {
        self.session_id
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SignedBotAction {
    bot_id: BotId,
    signer_public: PublicKey,
    session_id: [u8; 32],
    generation: u64,
    digest: PackageDigest,
    sequence: u64,
    action: BotAction,
    signature: Signature,
}

fn sign_action(
    id: BotId,
    public: PublicKey,
    secret: &ed25519::SecretKey,
    session: &BotSession,
    digest: PackageDigest,
    sequence: u64,
    action: BotAction,
) -> SignedBotAction {
    let mut value = SignedBotAction {
        bot_id: id,
        signer_public: public,
        session_id: session.session_id,
        generation: session.generation,
        digest,
        sequence,
        action,
        signature: [0; 64],
    };
    value.signature = *ed25519::sign(secret, &value.signing_bytes()).as_bytes();
    value
}

impl SignedBotAction {
    fn signing_bytes(&self) -> Vec<u8> {
        let mut out = ACTION_DOMAIN.to_vec();
        put(&mut out, &self.bot_id.0);
        put(&mut out, &self.signer_public);
        put(&mut out, &self.session_id);
        out.extend_from_slice(&self.generation.to_be_bytes());
        put(&mut out, &self.digest.0);
        out.extend_from_slice(&self.sequence.to_be_bytes());
        out.push(self.action.kind as u8);
        put_str(&mut out, &self.action.scope);
        put(&mut out, &self.action.body);
        out
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AcceptedBotAction {
    message_id: u64,
    signer: BotId,
    kind: ActionKind,
    scope: String,
    body: Vec<u8>,
    signature: Vec<u8>,
}

impl AcceptedBotAction {
    pub fn message_id(&self) -> u64 {
        self.message_id
    }
    pub fn signer_bot_id(&self) -> BotId {
        self.signer
    }
    pub fn body(&self) -> &[u8] {
        &self.body
    }
    pub fn signature(&self) -> &[u8] {
        &self.signature
    }
    pub fn person_signer(&self) -> Option<&str> {
        None
    }
}

#[derive(Clone, Debug)]
struct PendingChallenge {
    challenge: BotChallenge,
    declaration: Option<BotDeclaration>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct Durable {
    version: u8,
    enclave: String,
    owner: HumanIdentity,
    humans: Vec<HumanIdentity>,
    epoch: u64,
    bots: BTreeMap<BotId, BotMembership>,
    owner_nonces: BTreeSet<[u8; 32]>,
    challenge_replays: BTreeSet<[u8; 32]>,
    action_replays: BTreeSet<(BotId, u64, u64)>,
    accepted: Vec<AcceptedBotAction>,
    next_message_id: u64,
}

#[derive(Clone, Debug)]
pub struct BotAuthority {
    durable: Durable,
    pending: BTreeMap<[u8; 32], PendingChallenge>,
    sessions: BTreeMap<[u8; 32], BotSession>,
}

impl BotAuthority {
    pub fn new(
        enclave: impl Into<String>,
        owner: HumanIdentity,
        humans: impl IntoIterator<Item = HumanIdentity>,
    ) -> Self {
        let mut all: Vec<_> = std::iter::once(owner.clone()).chain(humans).collect();
        all.sort_by(|a, b| a.id.cmp(&b.id));
        Self {
            durable: Durable {
                version: 1,
                enclave: enclave.into(),
                owner,
                humans: all,
                epoch: 0,
                bots: BTreeMap::new(),
                owner_nonces: BTreeSet::new(),
                challenge_replays: BTreeSet::new(),
                action_replays: BTreeSet::new(),
                accepted: vec![],
                next_message_id: 1,
            },
            pending: BTreeMap::new(),
            sessions: BTreeMap::new(),
        }
    }
    pub fn enclave_id(&self) -> &str {
        &self.durable.enclave
    }
    pub fn membership_epoch(&self) -> u64 {
        self.durable.epoch
    }
    pub fn next_membership_epoch(&self) -> u64 {
        self.durable
            .epoch
            .checked_add(1)
            .expect("membership epoch exhausted")
    }
    pub fn human_identities(&self) -> &[HumanIdentity] {
        &self.durable.humans
    }
    pub fn bot_count(&self) -> usize {
        self.durable.bots.len()
    }
    pub fn bot(&self, id: BotId) -> Option<&BotMembership> {
        self.durable.bots.get(&id)
    }
    pub fn bot_epoch(&self, id: BotId) -> Option<u64> {
        self.bot(id).map(|b| b.epoch)
    }
    pub fn posts(&self) -> Vec<&AcceptedBotAction> {
        self.durable
            .accepted
            .iter()
            .filter(|a| a.kind == ActionKind::Post)
            .collect()
    }
    pub fn commands(&self) -> Vec<&AcceptedBotAction> {
        self.durable
            .accepted
            .iter()
            .filter(|a| a.kind == ActionKind::Command)
            .collect()
    }
    pub fn live_session_count(&self, id: BotId) -> usize {
        self.sessions.values().filter(|s| s.bot_id == id).count()
    }
    pub fn persist(&self) -> Vec<u8> {
        serde_json::to_vec(&self.durable).expect("durable bot authority serializes")
    }
    pub fn restart(bytes: &[u8]) -> Result<Self, BotError> {
        let durable: Durable = serde_json::from_slice(bytes).map_err(|_| BotError::InvalidState)?;
        validate(&durable)?;
        Ok(Self {
            durable,
            pending: BTreeMap::new(),
            sessions: BTreeMap::new(),
        })
    }

    pub fn begin_add(&mut self, action: SignedOwnerAction) -> Result<BotChallenge, BotError> {
        self.verify_owner(&action)?;
        let declaration = match &action.operation {
            OwnerOperation::Add(d) => d.clone(),
            _ => return Err(BotError::InvalidLifecycle),
        };
        if declaration.owner != self.durable.owner.id {
            return Err(BotError::UnauthorizedOwner);
        }
        if BotId::from_key(&declaration.public) != declaration.id {
            return Err(BotError::WrongBotKey);
        }
        if self
            .durable
            .humans
            .iter()
            .any(|h| h.public_key == declaration.public)
        {
            return Err(BotError::UserKeyImpersonation);
        }
        if self.durable.bots.contains_key(&declaration.id) {
            return Err(BotError::InvalidLifecycle);
        }
        self.durable.owner_nonces.insert(action.nonce);
        let challenge = BotChallenge {
            enclave_hash: hash(b"OSL/enclave-id/v1", &[self.durable.enclave.as_bytes()]),
            bot_id: declaration.id,
            id: random(),
            digest: declaration.digest,
            purpose: ChallengePurpose::Enroll,
        };
        self.pending.insert(
            challenge.id,
            PendingChallenge {
                challenge,
                declaration: Some(declaration),
            },
        );
        Ok(challenge)
    }

    pub fn complete_add(&mut self, response: ChallengeResponse) -> Result<(), BotError> {
        let pending = self.pending.get(&response.challenge_id).ok_or_else(|| {
            if self
                .durable
                .challenge_replays
                .contains(&response.challenge_id)
            {
                BotError::ChallengeReplay
            } else {
                BotError::UnknownChallenge
            }
        })?;
        if pending.challenge.purpose != ChallengePurpose::Enroll
            || pending.challenge.bot_id != response.bot_id
        {
            return Err(BotError::WrongBotKey);
        }
        let declaration = pending
            .declaration
            .as_ref()
            .expect("enrollment declaration");
        self.verify_response(&pending.challenge, &response, declaration.public)?;
        let declaration = declaration.clone();
        self.pending.remove(&response.challenge_id);
        self.durable.challenge_replays.insert(response.challenge_id);
        self.durable.epoch += 1;
        self.durable.bots.insert(
            declaration.id,
            BotMembership {
                id: declaration.id,
                owner: declaration.owner,
                public: declaration.public,
                digest: declaration.digest,
                lifecycle: BotLifecycle::Enabled,
                epoch: self.durable.epoch,
                scopes: declaration.scopes,
            },
        );
        Ok(())
    }

    pub fn begin_authentication(
        &mut self,
        id: BotId,
        digest: PackageDigest,
    ) -> Result<BotChallenge, BotError> {
        let bot = self.durable.bots.get(&id).ok_or(BotError::UnknownBot)?;
        lifecycle_error(bot.lifecycle)?;
        if bot.digest != digest {
            return Err(BotError::PackageDigestMismatch);
        }
        let challenge = BotChallenge {
            enclave_hash: hash(b"OSL/enclave-id/v1", &[self.durable.enclave.as_bytes()]),
            bot_id: id,
            id: random(),
            digest,
            purpose: ChallengePurpose::Authenticate {
                generation: bot.epoch,
            },
        };
        self.pending.insert(
            challenge.id,
            PendingChallenge {
                challenge,
                declaration: None,
            },
        );
        Ok(challenge)
    }

    pub fn complete_authentication(
        &mut self,
        response: ChallengeResponse,
    ) -> Result<BotSession, BotError> {
        let pending = self.pending.get(&response.challenge_id).ok_or_else(|| {
            if self
                .durable
                .challenge_replays
                .contains(&response.challenge_id)
            {
                BotError::ChallengeReplay
            } else {
                BotError::UnknownChallenge
            }
        })?;
        let bot = self
            .durable
            .bots
            .get(&response.bot_id)
            .ok_or(BotError::UnknownBot)?;
        lifecycle_error(bot.lifecycle)?;
        self.verify_response(&pending.challenge, &response, bot.public)?;
        let generation = match pending.challenge.purpose {
            ChallengePurpose::Authenticate { generation } if generation == bot.epoch => generation,
            _ => return Err(BotError::StaleCredential),
        };
        self.pending.remove(&response.challenge_id);
        self.durable.challenge_replays.insert(response.challenge_id);
        let session = BotSession {
            bot_id: bot.id,
            generation,
            session_id: random(),
        };
        self.sessions.insert(session.session_id, session);
        Ok(session)
    }

    pub fn apply_owner_action(&mut self, action: SignedOwnerAction) -> Result<(), BotError> {
        self.verify_owner(&action)?;
        let (id, next) = match action.operation {
            OwnerOperation::Disable(id) => (id, BotLifecycle::Disabled),
            OwnerOperation::Remove(id) => (id, BotLifecycle::Removed),
            OwnerOperation::Add(_) => return Err(BotError::InvalidLifecycle),
        };
        let current = self
            .durable
            .bots
            .get(&id)
            .ok_or(BotError::UnknownBot)?
            .lifecycle;
        if !matches!(
            (current, next),
            (BotLifecycle::Enabled, BotLifecycle::Disabled)
                | (BotLifecycle::Enabled, BotLifecycle::Removed)
                | (BotLifecycle::Disabled, BotLifecycle::Removed)
        ) {
            return Err(BotError::InvalidLifecycle);
        }
        self.durable.owner_nonces.insert(action.nonce);
        self.durable.epoch += 1;
        let bot = self.durable.bots.get_mut(&id).expect("checked");
        bot.lifecycle = next;
        bot.epoch = self.durable.epoch;
        self.sessions.retain(|_, session| session.bot_id != id);
        self.pending
            .retain(|_, pending| pending.challenge.bot_id != id);
        Ok(())
    }

    pub fn submit(
        &mut self,
        session: &BotSession,
        signed: SignedBotAction,
    ) -> Result<AcceptedBotAction, BotError> {
        let bot = self
            .durable
            .bots
            .get(&signed.bot_id)
            .ok_or(BotError::UnknownBot)?;
        lifecycle_error(bot.lifecycle)?;
        if signed.digest != bot.digest {
            return Err(BotError::PackageDigestMismatch);
        }
        if signed.signer_public != bot.public {
            if self
                .durable
                .humans
                .iter()
                .any(|h| h.public_key == signed.signer_public)
            {
                return Err(BotError::UserKeyImpersonation);
            }
            return Err(BotError::WrongBotKey);
        }
        if session.bot_id != bot.id
            || signed.session_id != session.session_id
            || signed.generation != session.generation
            || bot.epoch != session.generation
            || !self.sessions.contains_key(&session.session_id)
        {
            return Err(BotError::StaleCredential);
        }
        let scope = BotScope {
            kind: signed.action.kind,
            name: signed.action.scope.clone(),
        };
        if !bot.scopes.contains(&scope) {
            return Err(BotError::OutOfScope);
        }
        let replay = (bot.id, signed.generation, signed.sequence);
        if self.durable.action_replays.contains(&replay) {
            return Err(BotError::Replay);
        }
        if !verify(
            signed.signer_public,
            &signed.signing_bytes(),
            signed.signature,
        ) {
            return Err(BotError::WrongBotKey);
        }
        self.durable.action_replays.insert(replay);
        let accepted = AcceptedBotAction {
            message_id: self.durable.next_message_id,
            signer: bot.id,
            kind: signed.action.kind,
            scope: signed.action.scope,
            body: signed.action.body,
            signature: signed.signature.to_vec(),
        };
        self.durable.next_message_id += 1;
        self.durable.accepted.push(accepted.clone());
        Ok(accepted)
    }

    pub fn render_bot_tag(&self, claim: &BotUiClaim) -> Result<BotUiTag, BotError> {
        let id = match claim.subject {
            UiSubject::Bot(id) => id,
            UiSubject::Human => return Err(BotError::NoBotAuthority),
        };
        let bot = self.durable.bots.get(&id).ok_or(BotError::NoBotAuthority)?;
        if bot.lifecycle != BotLifecycle::Enabled || claim.label != "BOT" {
            return Err(BotError::NoBotAuthority);
        }
        Ok(BotUiTag {
            id,
            label: "BOT",
            authenticated: true,
        })
    }

    fn verify_owner(&self, action: &SignedOwnerAction) -> Result<(), BotError> {
        if action.enclave != self.durable.enclave || action.epoch != self.next_membership_epoch() {
            return Err(BotError::StaleMembershipEpoch);
        }
        if action.actor != self.durable.owner.id {
            return Err(BotError::UnauthorizedOwner);
        }
        if self.durable.owner_nonces.contains(&action.nonce) {
            return Err(BotError::Replay);
        }
        if !verify(
            self.durable.owner.public_key,
            &action.signing_bytes(),
            action.signature,
        ) {
            return Err(BotError::UnauthorizedOwner);
        }
        Ok(())
    }
    fn verify_response(
        &self,
        challenge: &BotChallenge,
        response: &ChallengeResponse,
        expected: PublicKey,
    ) -> Result<(), BotError> {
        if response.digest != challenge.digest {
            return Err(BotError::PackageDigestMismatch);
        }
        if response.signer_public != expected
            || !verify(
                expected,
                &challenge_bytes(challenge, response.digest),
                response.signature,
            )
        {
            return Err(BotError::WrongBotKey);
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub enum UiSubject {
    Bot(BotId),
    Human,
}
impl From<BotId> for UiSubject {
    fn from(value: BotId) -> Self {
        Self::Bot(value)
    }
}
impl From<&str> for UiSubject {
    fn from(_value: &str) -> Self {
        Self::Human
    }
}

#[derive(Clone, Debug)]
pub struct BotUiClaim {
    subject: UiSubject,
    _name: String,
    label: String,
}
impl BotUiClaim {
    pub fn new(
        subject: impl Into<UiSubject>,
        name: impl Into<String>,
        label: impl Into<String>,
    ) -> Self {
        Self {
            subject: subject.into(),
            _name: name.into(),
            label: label.into(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BotUiTag {
    id: BotId,
    label: &'static str,
    authenticated: bool,
}
impl BotUiTag {
    pub fn bot_id(&self) -> BotId {
        self.id
    }
    pub fn label(&self) -> &str {
        self.label
    }
    pub fn authenticated(&self) -> bool {
        self.authenticated
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum BotError {
    #[error("authorized owner signature required")]
    UnauthorizedOwner,
    #[error("membership epoch is stale")]
    StaleMembershipEpoch,
    #[error("wrong bot key")]
    WrongBotKey,
    #[error("a user key cannot impersonate a bot")]
    UserKeyImpersonation,
    #[error("bot package digest changed")]
    PackageDigestMismatch,
    #[error("challenge is unknown")]
    UnknownChallenge,
    #[error("challenge replay refused")]
    ChallengeReplay,
    #[error("bot action replay refused")]
    Replay,
    #[error("bot scope refused")]
    OutOfScope,
    #[error("bot is unknown")]
    UnknownBot,
    #[error("bot is disabled")]
    BotDisabled,
    #[error("bot is removed")]
    BotRemoved,
    #[error("bot credential is stale or revoked")]
    StaleCredential,
    #[error("bot lifecycle transition is invalid")]
    InvalidLifecycle,
    #[error("BOT tag lacks authenticated bot authority")]
    NoBotAuthority,
    #[error("durable bot state is invalid")]
    InvalidState,
}

fn lifecycle_error(state: BotLifecycle) -> Result<(), BotError> {
    match state {
        BotLifecycle::Enabled => Ok(()),
        BotLifecycle::Disabled => Err(BotError::BotDisabled),
        BotLifecycle::Removed => Err(BotError::BotRemoved),
    }
}
fn validate(d: &Durable) -> Result<(), BotError> {
    if d.version != 1 || d.humans.is_empty() || !d.humans.contains(&d.owner) {
        return Err(BotError::InvalidState);
    }
    if d.bots.iter().any(|(id, b)| {
        id != &b.id
            || BotId::from_key(&b.public) != *id
            || d.humans.iter().any(|h| h.public_key == b.public)
    }) {
        return Err(BotError::InvalidState);
    }
    Ok(())
}
fn challenge_bytes(c: &BotChallenge, digest: PackageDigest) -> Vec<u8> {
    let mut out = CHALLENGE_DOMAIN.to_vec();
    put(&mut out, &c.enclave_hash);
    put(&mut out, &c.bot_id.0);
    put(&mut out, &c.id);
    put(&mut out, &digest.0);
    out.push(match c.purpose {
        ChallengePurpose::Enroll => 1,
        ChallengePurpose::Authenticate { .. } => 2,
    });
    if let ChallengePurpose::Authenticate { generation } = c.purpose {
        out.extend_from_slice(&generation.to_be_bytes());
    }
    out
}
fn verify(public: PublicKey, bytes: &[u8], signature: Signature) -> bool {
    ed25519::verify(
        &ed25519::PublicKey::from_bytes(public),
        bytes,
        &ed25519::Signature::from_bytes(signature),
    )
    .unwrap_or(false)
}
fn hash(domain: &[u8], parts: &[&[u8]]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(domain);
    for p in parts {
        h.update((p.len() as u64).to_be_bytes());
        h.update(p);
    }
    h.finalize().into()
}
fn random() -> [u8; 32] {
    let mut value = [0; 32];
    getrandom::getrandom(&mut value).expect("OS CSPRNG unavailable");
    value
}
fn put(out: &mut Vec<u8>, bytes: &[u8]) {
    out.extend_from_slice(&(bytes.len() as u64).to_be_bytes());
    out.extend_from_slice(bytes);
}
fn put_str(out: &mut Vec<u8>, value: &str) {
    put(out, value.as_bytes());
}
