//! Cryptographic sender admission before a provider body fetch (TASK 5902).
//!
//! Provider sender and folder filters deliberately remain candidate selectors.
//! The only values that authorize a body fetch are an established friend's
//! pinned Ed25519 key and a signed, bounded proof envelope binding the exact
//! provider message, recipient, conversation, and ciphertext digest.

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use crypto::ed25519;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fmt;

use super::{
    normalize_sender, provider_prefilter_request_from_senders, provider_spec,
    ProviderPrefilterRequest, Readiness, PROCESS_ENTRY_BOUNDARY,
};

pub const SENDER_PROOF_VERSION: u8 = 1;
pub const SENDER_PROOF_SIGNING_DOMAIN: &[u8] = b"OSL/mail-authenticated-sender-proof/v1";
pub const FAILED_SENDER_PROOF: &str = "failed cryptographic sender proof";
pub const MAX_CANDIDATE_RESPONSE_BYTES: usize = 16 * 1024;
pub const MAX_PROOF_ENVELOPE_BYTES: usize = 4 * 1024;
pub const MAX_CANDIDATES: usize = 256;
pub const MAX_PROVIDER_ID_BYTES: usize = 64;
pub const MAX_PROVIDER_MESSAGE_ID_BYTES: usize = 512;
pub const MAX_RECIPIENT_BYTES: usize = 254;
pub const MAX_CONVERSATION_BYTES: usize = 512;
pub const PROOF_ENVELOPE_FIELDS: [&str; 7] = [
    "version",
    "provider_id",
    "provider_message_id",
    "recipient",
    "conversation",
    "body_ciphertext_sha256_b64",
    "signature_b64",
];

/// One allowed sender address joined to the Ed25519 key from the established
/// OSL friend identity. The lookup, not provider metadata, supplies this value.
#[derive(Clone, PartialEq, Eq)]
pub struct EstablishedAllowedSender {
    exact_sender: String,
    identity_key: ed25519::PublicKey,
}

impl EstablishedAllowedSender {
    pub fn new(
        exact_sender: impl Into<String>,
        identity_key: ed25519::PublicKey,
    ) -> Result<Self, &'static str> {
        let exact_sender = normalize_sender(&exact_sender.into())
            .ok_or("established allowed sender address is invalid")?;
        Ok(Self {
            exact_sender,
            identity_key,
        })
    }

    pub fn exact_sender(&self) -> &str {
        &self.exact_sender
    }

    pub fn identity_key(&self) -> ed25519::PublicKey {
        self.identity_key
    }
}

impl fmt::Debug for EstablishedAllowedSender {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("EstablishedAllowedSender(<redacted>)")
    }
}

/// Authority adapter implemented by the application's established-friend
/// store. Provider headers, mailbox possession, and renderer values are not
/// valid implementations of this lookup.
pub trait EstablishedAllowedSenderLookup {
    type Error: fmt::Display;

    fn established_allowed_senders(
        &mut self,
        provider_id: &str,
        mailbox_place: &str,
    ) -> Result<Vec<EstablishedAllowedSender>, Self::Error>;
}

/// Opaque authorization for one provider, recipient, and conversation.
/// Its fields are private so a provider response cannot mint or modify it.
#[derive(Clone)]
pub struct AuthenticatedSenderGrant {
    provider_id: String,
    recipient: String,
    conversation: String,
    request: ProviderPrefilterRequest,
    identity_keys: Vec<ed25519::PublicKey>,
}

impl fmt::Debug for AuthenticatedSenderGrant {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AuthenticatedSenderGrant")
            .field("provider_id", &self.provider_id)
            .field("recipient", &"<redacted>")
            .field("conversation", &"<redacted>")
            .field("identity_key_count", &self.identity_keys.len())
            .finish()
    }
}

/// Resolve body-fetch authority before any provider call is possible.
pub fn lookup_authenticated_sender_grant<L: EstablishedAllowedSenderLookup>(
    provider_id: &str,
    recipient: &str,
    conversation: &str,
    lookup: &mut L,
) -> Result<AuthenticatedSenderGrant, AuthenticatedSenderBoundaryError> {
    validate_text(provider_id, MAX_PROVIDER_ID_BYTES)
        .map_err(|_| boundary_error(provider_id, None, "provider identity is invalid"))?;
    let recipient = normalize_sender(recipient)
        .ok_or_else(|| boundary_error(provider_id, None, "recipient binding is invalid"))?;
    validate_text(&recipient, MAX_RECIPIENT_BYTES)
        .map_err(|_| boundary_error(provider_id, None, "recipient binding is invalid"))?;
    validate_text(conversation, MAX_CONVERSATION_BYTES)
        .map_err(|_| boundary_error(provider_id, None, "conversation binding is invalid"))?;

    let spec =
        provider_spec(provider_id).map_err(|reason| boundary_error(provider_id, None, reason))?;
    if spec.readiness != Readiness::Ready {
        return Err(boundary_error(
            provider_id,
            None,
            format!(
                "NotReady: authenticated pre-body sender proof unavailable; refusing access to {}",
                spec.mailbox_place
            ),
        ));
    }
    let established = lookup
        .established_allowed_senders(provider_id, &spec.mailbox_place)
        .map_err(|_| {
            boundary_error(
                provider_id,
                None,
                "established friend identity lookup failed",
            )
        })?;
    if established.is_empty() {
        return Err(boundary_error(
            provider_id,
            None,
            "established friend identity inventory is empty",
        ));
    }

    let mut sender_addresses = Vec::with_capacity(established.len());
    let mut seen_keys = BTreeSet::new();
    let mut identity_keys = Vec::with_capacity(established.len());
    for sender in established {
        sender_addresses.push(sender.exact_sender);
        if seen_keys.insert(*sender.identity_key.as_bytes()) {
            identity_keys.push(sender.identity_key);
        }
    }
    let request = provider_prefilter_request_from_senders(provider_id, &sender_addresses)
        .map_err(|error| boundary_error(provider_id, None, error.reason))?;
    Ok(AuthenticatedSenderGrant {
        provider_id: provider_id.to_owned(),
        recipient,
        conversation: conversation.to_owned(),
        request,
        identity_keys,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SenderProofEnvelope {
    pub version: u8,
    pub provider_id: String,
    pub provider_message_id: String,
    pub recipient: String,
    pub conversation: String,
    pub body_ciphertext_sha256_b64: String,
    pub signature_b64: String,
}

impl SenderProofEnvelope {
    pub fn sign(
        provider_id: impl Into<String>,
        provider_message_id: impl Into<String>,
        recipient: impl Into<String>,
        conversation: impl Into<String>,
        body_ciphertext: &[u8],
        signer: &ed25519::SecretKey,
    ) -> Result<Self, &'static str> {
        let mut envelope = Self {
            version: SENDER_PROOF_VERSION,
            provider_id: provider_id.into(),
            provider_message_id: provider_message_id.into(),
            recipient: recipient.into(),
            conversation: conversation.into(),
            body_ciphertext_sha256_b64: STANDARD.encode(Sha256::digest(body_ciphertext)),
            signature_b64: String::new(),
        };
        let canonical = envelope.canonical_bytes()?;
        envelope.signature_b64 = STANDARD.encode(ed25519::sign(signer, &canonical).as_bytes());
        Ok(envelope)
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>, &'static str> {
        if self.version != SENDER_PROOF_VERSION {
            return Err("unsupported sender proof version");
        }
        validate_text(&self.provider_id, MAX_PROVIDER_ID_BYTES)?;
        validate_text(&self.provider_message_id, MAX_PROVIDER_MESSAGE_ID_BYTES)?;
        validate_text(&self.recipient, MAX_RECIPIENT_BYTES)?;
        validate_text(&self.conversation, MAX_CONVERSATION_BYTES)?;
        let digest = decode_fixed::<32>(&self.body_ciphertext_sha256_b64)?;

        let mut canonical = Vec::with_capacity(
            SENDER_PROOF_SIGNING_DOMAIN.len()
                + self.provider_id.len()
                + self.provider_message_id.len()
                + self.recipient.len()
                + self.conversation.len()
                + 64,
        );
        append_length_prefixed(&mut canonical, SENDER_PROOF_SIGNING_DOMAIN)?;
        canonical.push(SENDER_PROOF_VERSION);
        append_length_prefixed(&mut canonical, self.provider_id.as_bytes())?;
        append_length_prefixed(&mut canonical, self.provider_message_id.as_bytes())?;
        append_length_prefixed(&mut canonical, self.recipient.as_bytes())?;
        append_length_prefixed(&mut canonical, self.conversation.as_bytes())?;
        canonical.extend_from_slice(&digest);
        Ok(canonical)
    }
}

fn append_length_prefixed(output: &mut Vec<u8>, bytes: &[u8]) -> Result<(), &'static str> {
    let length = u32::try_from(bytes.len()).map_err(|_| "sender proof field is too long")?;
    output.extend_from_slice(&length.to_be_bytes());
    output.extend_from_slice(bytes);
    Ok(())
}

fn decode_fixed<const N: usize>(encoded: &str) -> Result<[u8; N], &'static str> {
    let bytes = STANDARD
        .decode(encoded)
        .map_err(|_| "sender proof binary field is malformed")?;
    bytes
        .try_into()
        .map_err(|_| "sender proof binary field has the wrong length")
}

fn validate_text(value: &str, max_bytes: usize) -> Result<(), &'static str> {
    if value.is_empty()
        || value.len() > max_bytes
        || value.trim() != value
        || value.chars().any(char::is_control)
    {
        Err("sender proof text field is invalid")
    } else {
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthenticatedRawResponseKind {
    CandidateIds,
    ProofEnvelope,
    CiphertextBody,
}

pub trait AuthenticatedSenderProcessEntryObserver {
    type Error: fmt::Display;

    fn observe_before_parsing(
        &mut self,
        provider_id: &str,
        boundary: &str,
        kind: AuthenticatedRawResponseKind,
        message_id: Option<&str>,
        raw_response: &[u8],
    ) -> Result<(), Self::Error>;
}

/// Provider transport for the authenticated path. The first request is still
/// narrowed by sender/folder for efficiency, but only `proof_envelope` can
/// authorize the later `ciphertext_body` call.
pub trait AuthenticatedProviderMailbox {
    type Error: fmt::Display;

    fn candidate_ids(&mut self, request: &ProviderPrefilterRequest)
        -> Result<Vec<u8>, Self::Error>;

    fn proof_envelope(
        &mut self,
        request: &ProofEnvelopeFetchRequest,
    ) -> Result<Vec<u8>, Self::Error>;

    fn ciphertext_body(&mut self, message_id: &str) -> Result<Vec<u8>, Self::Error>;
}

/// Exact bounded provider request for metadata that can authenticate one
/// candidate without fetching any part of its body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProofEnvelopeFetchRequest {
    pub provider_message_id: String,
    pub exact_fields: Vec<&'static str>,
    pub max_response_bytes: usize,
}

impl ProofEnvelopeFetchRequest {
    fn for_message(provider_message_id: &str) -> Self {
        Self {
            provider_message_id: provider_message_id.to_owned(),
            exact_fields: PROOF_ENVELOPE_FIELDS.to_vec(),
            max_response_bytes: MAX_PROOF_ENVELOPE_BYTES,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthenticatedCiphertextBody {
    pub provider_message_id: String,
    pub ciphertext: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SenderProofRefusal {
    provider_id: String,
    provider_message_id: String,
}

impl SenderProofRefusal {
    pub fn provider_id(&self) -> &str {
        &self.provider_id
    }

    pub fn provider_message_id(&self) -> &str {
        &self.provider_message_id
    }

    pub const fn reason(&self) -> &'static str {
        FAILED_SENDER_PROOF
    }
}

impl fmt::Display for SenderProofRefusal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "provider_id={} message_id={} failure={}",
            self.provider_id, self.provider_message_id, FAILED_SENDER_PROOF
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthenticatedSenderRead {
    pub bodies: Vec<AuthenticatedCiphertextBody>,
    pub refusals: Vec<SenderProofRefusal>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthenticatedSenderBoundaryError {
    pub provider_id: String,
    pub message_id: Option<String>,
    pub reason: String,
}

impl fmt::Display for AuthenticatedSenderBoundaryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(message_id) = &self.message_id {
            write!(
                formatter,
                "provider_id={} message_id={} boundary={} failure={}",
                self.provider_id, message_id, PROCESS_ENTRY_BOUNDARY, self.reason
            )
        } else {
            write!(
                formatter,
                "provider_id={} boundary={} failure={}",
                self.provider_id, PROCESS_ENTRY_BOUNDARY, self.reason
            )
        }
    }
}

impl std::error::Error for AuthenticatedSenderBoundaryError {}

fn boundary_error(
    provider_id: &str,
    message_id: Option<&str>,
    reason: impl Into<String>,
) -> AuthenticatedSenderBoundaryError {
    AuthenticatedSenderBoundaryError {
        provider_id: provider_id.to_owned(),
        message_id: message_id.map(str::to_owned),
        reason: reason.into(),
    }
}

fn proof_refusal(provider_id: &str, message_id: &str) -> SenderProofRefusal {
    SenderProofRefusal {
        provider_id: provider_id.to_owned(),
        provider_message_id: message_id.to_owned(),
    }
}

fn authenticate_envelope(
    raw: &[u8],
    provider_id: &str,
    message_id: &str,
    grant: &AuthenticatedSenderGrant,
) -> Option<[u8; 32]> {
    if raw.len() > MAX_PROOF_ENVELOPE_BYTES {
        return None;
    }
    let envelope: SenderProofEnvelope = serde_json::from_slice(raw).ok()?;
    if envelope.provider_id != provider_id
        || envelope.provider_message_id != message_id
        || envelope.recipient != grant.recipient
        || envelope.conversation != grant.conversation
    {
        return None;
    }
    let canonical = envelope.canonical_bytes().ok()?;
    let digest = decode_fixed::<32>(&envelope.body_ciphertext_sha256_b64).ok()?;
    let signature =
        ed25519::Signature::from_bytes(decode_fixed::<64>(&envelope.signature_b64).ok()?);
    let verified = grant
        .identity_keys
        .iter()
        .any(|key| ed25519::verify(key, &canonical, &signature).unwrap_or(false));
    verified.then_some(digest)
}

/// Authenticate every candidate proof before the first body request, then
/// fetch and digest-check only those candidates whose proof verified.
pub fn read_authenticated_conversations<T, O>(
    provider_id: &str,
    grant: AuthenticatedSenderGrant,
    transport: &mut T,
    observer: &mut O,
) -> Result<AuthenticatedSenderRead, AuthenticatedSenderBoundaryError>
where
    T: AuthenticatedProviderMailbox,
    O: AuthenticatedSenderProcessEntryObserver,
{
    if grant.provider_id != provider_id {
        return Err(boundary_error(
            provider_id,
            None,
            "authenticated sender grant belongs to another provider",
        ));
    }

    let raw_candidates = transport
        .candidate_ids(&grant.request)
        .map_err(|_| boundary_error(provider_id, None, "candidate query failed"))?;
    observer
        .observe_before_parsing(
            provider_id,
            PROCESS_ENTRY_BOUNDARY,
            AuthenticatedRawResponseKind::CandidateIds,
            None,
            &raw_candidates,
        )
        .map_err(|_| boundary_error(provider_id, None, "process-entry observer failed"))?;
    if raw_candidates.len() > MAX_CANDIDATE_RESPONSE_BYTES {
        return Err(boundary_error(
            provider_id,
            None,
            "candidate response exceeds the bounded metadata limit",
        ));
    }
    let candidates: super::CandidateIdsResponse = serde_json::from_slice(&raw_candidates)
        .map_err(|_| boundary_error(provider_id, None, "candidate response is malformed"))?;
    if candidates.message_ids.len() > MAX_CANDIDATES {
        return Err(boundary_error(
            provider_id,
            None,
            "candidate response exceeds the bounded candidate limit",
        ));
    }
    let mut seen = BTreeSet::new();
    for message_id in &candidates.message_ids {
        if validate_text(message_id, MAX_PROVIDER_MESSAGE_ID_BYTES).is_err()
            || !seen.insert(message_id.clone())
        {
            return Err(boundary_error(
                provider_id,
                Some(message_id),
                "candidate message ID is invalid or duplicated",
            ));
        }
    }

    let mut authenticated = Vec::with_capacity(candidates.message_ids.len());
    let mut refusals = Vec::new();
    for message_id in &candidates.message_ids {
        let proof_request = ProofEnvelopeFetchRequest::for_message(message_id);
        let raw_proof = match transport.proof_envelope(&proof_request) {
            Ok(raw) => raw,
            Err(_) => {
                refusals.push(proof_refusal(provider_id, message_id));
                continue;
            }
        };
        observer
            .observe_before_parsing(
                provider_id,
                PROCESS_ENTRY_BOUNDARY,
                AuthenticatedRawResponseKind::ProofEnvelope,
                Some(message_id),
                &raw_proof,
            )
            .map_err(|_| {
                boundary_error(
                    provider_id,
                    Some(message_id),
                    "process-entry observer failed",
                )
            })?;
        if let Some(digest) = authenticate_envelope(&raw_proof, provider_id, message_id, &grant) {
            authenticated.push((message_id.clone(), digest));
        } else {
            refusals.push(proof_refusal(provider_id, message_id));
        }
    }

    let mut bodies = Vec::with_capacity(authenticated.len());
    for (message_id, expected_digest) in authenticated {
        let ciphertext = transport.ciphertext_body(&message_id).map_err(|_| {
            boundary_error(
                provider_id,
                Some(&message_id),
                "ciphertext body fetch failed",
            )
        })?;
        observer
            .observe_before_parsing(
                provider_id,
                PROCESS_ENTRY_BOUNDARY,
                AuthenticatedRawResponseKind::CiphertextBody,
                Some(&message_id),
                &ciphertext,
            )
            .map_err(|_| {
                boundary_error(
                    provider_id,
                    Some(&message_id),
                    "process-entry observer failed",
                )
            })?;
        let actual_digest: [u8; 32] = Sha256::digest(&ciphertext).into();
        if actual_digest == expected_digest {
            bodies.push(AuthenticatedCiphertextBody {
                provider_message_id: message_id,
                ciphertext,
            });
        } else {
            refusals.push(proof_refusal(provider_id, &message_id));
        }
    }

    Ok(AuthenticatedSenderRead { bodies, refusals })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    struct Lookup {
        sender: EstablishedAllowedSender,
    }

    impl EstablishedAllowedSenderLookup for Lookup {
        type Error = &'static str;

        fn established_allowed_senders(
            &mut self,
            _provider_id: &str,
            _mailbox_place: &str,
        ) -> Result<Vec<EstablishedAllowedSender>, Self::Error> {
            Ok(vec![self.sender.clone()])
        }
    }

    #[derive(Default)]
    struct Mailbox {
        candidate_ids: Vec<String>,
        proofs: BTreeMap<String, Vec<u8>>,
        bodies: BTreeMap<String, Vec<u8>>,
        calls: Vec<String>,
        proof_requests: Vec<ProofEnvelopeFetchRequest>,
    }

    impl AuthenticatedProviderMailbox for Mailbox {
        type Error = &'static str;

        fn candidate_ids(
            &mut self,
            _request: &ProviderPrefilterRequest,
        ) -> Result<Vec<u8>, Self::Error> {
            self.calls.push("candidates".to_owned());
            Ok(serde_json::to_vec(&super::super::CandidateIdsResponse {
                message_ids: self.candidate_ids.clone(),
            })
            .unwrap())
        }

        fn proof_envelope(
            &mut self,
            request: &ProofEnvelopeFetchRequest,
        ) -> Result<Vec<u8>, Self::Error> {
            self.calls
                .push(format!("proof:{}", request.provider_message_id));
            self.proof_requests.push(request.clone());
            self.proofs
                .get(&request.provider_message_id)
                .cloned()
                .ok_or("missing proof")
        }

        fn ciphertext_body(&mut self, message_id: &str) -> Result<Vec<u8>, Self::Error> {
            self.calls.push(format!("body:{message_id}"));
            self.bodies.get(message_id).cloned().ok_or("missing body")
        }
    }

    #[derive(Default)]
    struct Observer(Vec<(AuthenticatedRawResponseKind, Option<String>, usize)>);

    impl AuthenticatedSenderProcessEntryObserver for Observer {
        type Error = &'static str;

        fn observe_before_parsing(
            &mut self,
            _provider_id: &str,
            boundary: &str,
            kind: AuthenticatedRawResponseKind,
            message_id: Option<&str>,
            raw_response: &[u8],
        ) -> Result<(), Self::Error> {
            assert_eq!(boundary, PROCESS_ENTRY_BOUNDARY);
            self.0
                .push((kind, message_id.map(str::to_owned), raw_response.len()));
            Ok(())
        }
    }

    #[test]
    fn only_a_proof_from_the_established_friend_authorizes_a_body_fetch() {
        let (friend_secret, friend_public) = ed25519::generate_keypair();
        let (attacker_secret, _) = ed25519::generate_keypair();
        let mut lookup = Lookup {
            sender: EstablishedAllowedSender::new("friend@example.test", friend_public).unwrap(),
        };
        let grant = lookup_authenticated_sender_grant(
            "gmail",
            "recipient@example.test",
            "conversation-5902",
            &mut lookup,
        )
        .unwrap();

        let genuine_body = b"genuine-ciphertext".to_vec();
        let hostile_body = b"hostile-body-must-not-cross".to_vec();
        let ids = [
            "genuine",
            "forged-from",
            "allowed-alias",
            "forwarding-rewrite",
            "compromised-mailbox",
        ];
        let mut mailbox = Mailbox {
            candidate_ids: ids.iter().map(|id| (*id).to_owned()).collect(),
            ..Mailbox::default()
        };
        mailbox
            .bodies
            .insert("genuine".into(), genuine_body.clone());
        for id in ids {
            let (body, signer) = if id == "genuine" {
                (genuine_body.as_slice(), &friend_secret)
            } else {
                (hostile_body.as_slice(), &attacker_secret)
            };
            let proof = SenderProofEnvelope::sign(
                "gmail",
                id,
                "recipient@example.test",
                "conversation-5902",
                body,
                signer,
            )
            .unwrap();
            mailbox
                .proofs
                .insert(id.to_owned(), serde_json::to_vec(&proof).unwrap());
        }

        let mut observer = Observer::default();
        let read =
            read_authenticated_conversations("gmail", grant, &mut mailbox, &mut observer).unwrap();

        assert_eq!(read.bodies.len(), 1);
        assert_eq!(read.bodies[0].provider_message_id, "genuine");
        assert_eq!(read.bodies[0].ciphertext, genuine_body);
        assert_eq!(read.refusals.len(), 4);
        assert!(read.refusals.iter().all(|refusal| {
            refusal.reason() == FAILED_SENDER_PROOF
                && !refusal.to_string().contains("hostile-body-must-not-cross")
        }));
        assert_eq!(
            mailbox
                .calls
                .iter()
                .filter(|call| call.starts_with("body:"))
                .cloned()
                .collect::<Vec<_>>(),
            ["body:genuine"]
        );
        assert_eq!(
            observer
                .0
                .iter()
                .filter(|entry| entry.0 == AuthenticatedRawResponseKind::CiphertextBody)
                .map(|entry| entry.1.as_deref().unwrap())
                .collect::<Vec<_>>(),
            ["genuine"]
        );
        println!(
            "TASK5902_CRYPTO provider=gmail candidate_proofs=5 verified=1 refused=4 body_fetches=1 hostile_body_fetches=0 hostile_process_entry_body_bytes=0"
        );
        assert_eq!(mailbox.proof_requests.len(), 5);
        assert!(mailbox.proof_requests.iter().all(|request| {
            request.exact_fields == PROOF_ENVELOPE_FIELDS
                && request.max_response_bytes == MAX_PROOF_ENVELOPE_BYTES
        }));
    }

    #[test]
    fn proof_bindings_and_metadata_bounds_fail_before_body_fetch() {
        let (friend_secret, friend_public) = ed25519::generate_keypair();
        let mut lookup = Lookup {
            sender: EstablishedAllowedSender::new("friend@example.test", friend_public).unwrap(),
        };
        let grant = lookup_authenticated_sender_grant(
            "gmail",
            "recipient@example.test",
            "conversation-5902",
            &mut lookup,
        )
        .unwrap();
        let mut wrong_conversation = SenderProofEnvelope::sign(
            "gmail",
            "hostile",
            "recipient@example.test",
            "other-conversation",
            b"hostile",
            &friend_secret,
        )
        .unwrap();
        // Keep a valid signature for the wrong binding; matching a different
        // signed conversation still cannot authorize this grant.
        wrong_conversation.signature_b64 = STANDARD.encode(
            ed25519::sign(
                &friend_secret,
                &wrong_conversation.canonical_bytes().unwrap(),
            )
            .as_bytes(),
        );
        let mut mailbox = Mailbox {
            candidate_ids: vec!["hostile".into(), "oversized".into()],
            ..Mailbox::default()
        };
        mailbox.proofs.insert(
            "hostile".into(),
            serde_json::to_vec(&wrong_conversation).unwrap(),
        );
        mailbox
            .proofs
            .insert("oversized".into(), vec![b'x'; MAX_PROOF_ENVELOPE_BYTES + 1]);

        let mut observer = Observer::default();
        let read =
            read_authenticated_conversations("gmail", grant, &mut mailbox, &mut observer).unwrap();
        assert!(read.bodies.is_empty());
        assert_eq!(read.refusals.len(), 2);
        assert!(!mailbox.calls.iter().any(|call| call.starts_with("body:")));
        println!(
            "TASK5902_CRYPTO_RED wrong_conversation=refused oversized_proof=refused body_fetches=0"
        );
    }
}
