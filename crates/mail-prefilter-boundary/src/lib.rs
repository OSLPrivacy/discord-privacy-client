//! TASK 4350: provider-side mail filtering and the pre-parse process-entry boundary.
//!
//! There is deliberately no API that accepts an unfiltered mailbox response. The shipping
//! reader can start only from a provider policy that can build a sender/folder prefilter.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

pub const EVIDENCE_SCHEMA: &str = "osl-task-4350-live-evidence-v1";
pub const PROCESS_ENTRY_BOUNDARY: &str =
    "mail-prefilter-boundary/raw-provider-response-before-serde-parsing";
pub const ALLOWED_MESSAGE_COUNT: usize = 5;
pub const DISALLOWED_MESSAGE_COUNT: usize = 15;

#[derive(Debug, Deserialize)]
struct SurfaceRuling {
    schema: String,
    email_carriers: Vec<String>,
    native_email_carriers: Vec<String>,
    first_party_surfaces: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderSpec {
    pub id: String,
    pub mailbox_place: String,
    pub readiness: Readiness,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Readiness {
    Ready,
    NotReady,
}

/// Derives the complete mail identity inventory from the owner ruling. Outlook's web identity
/// is derived from the email-carrier row and its desktop identity from native-email-carriers.
pub fn inventory_from_ruling_json(json: &str) -> Result<Vec<ProviderSpec>, String> {
    let ruling: SurfaceRuling =
        serde_json::from_str(json).map_err(|error| format!("invalid surface ruling: {error}"))?;
    if ruling.schema != "osl-surface-ruling-v1" {
        return Err(format!(
            "unexpected surface ruling schema: {}",
            ruling.schema
        ));
    }
    if !ruling
        .first_party_surfaces
        .iter()
        .any(|id| id == "osl-mail")
    {
        return Err("surface ruling omits first-party osl-mail".into());
    }
    if ruling.native_email_carriers != ["outlook"] {
        return Err("surface ruling native mail identity must be exactly outlook".into());
    }

    let expected = [
        "gmail",
        "outlook",
        "proton",
        "yahoo",
        "aol",
        "gmx",
        "maildotcom",
        "icloud",
        "tuta",
    ];
    let actual: BTreeSet<_> = ruling.email_carriers.iter().map(String::as_str).collect();
    if actual != expected.into_iter().collect() {
        return Err("surface ruling web email identities differ from the authoritative set".into());
    }

    let mut inventory = vec![provider_spec("osl-mail")?];
    for carrier in &ruling.email_carriers {
        let id = if carrier == "outlook" {
            "outlook-web"
        } else {
            carrier
        };
        inventory.push(provider_spec(id)?);
    }
    inventory.push(provider_spec("outlook-desktop")?);
    if inventory.len() != 11 {
        return Err(format!(
            "authoritative provider inventory must have 11 identities, found {}",
            inventory.len()
        ));
    }
    Ok(inventory)
}

pub fn authoritative_inventory() -> Vec<ProviderSpec> {
    inventory_from_ruling_json(include_str!("../../../data/surface-ruling-2026-08-05.json"))
        .expect("checked-in authoritative surface ruling must remain valid")
}

fn provider_spec(id: &str) -> Result<ProviderSpec, String> {
    let (mailbox_place, readiness) = match id {
        "gmail" => ("Gmail Inbox", Readiness::Ready),
        "osl-mail" => ("OSL Mail local mailbox", Readiness::NotReady),
        "outlook-web" => ("Outlook web Inbox", Readiness::NotReady),
        "outlook-desktop" => ("Outlook desktop Inbox", Readiness::NotReady),
        "proton" => ("Proton Mail web Inbox", Readiness::NotReady),
        "yahoo" => ("Yahoo Mail web Inbox", Readiness::NotReady),
        "aol" => ("AOL Mail web Inbox", Readiness::NotReady),
        "gmx" => ("GMX Mail web Inbox", Readiness::NotReady),
        "maildotcom" => ("Mail.com web Inbox", Readiness::NotReady),
        "icloud" => ("iCloud Mail web Inbox", Readiness::NotReady),
        "tuta" => ("Tuta Mail web Inbox", Readiness::NotReady),
        other => return Err(format!("unknown provider identity {other}")),
    };
    Ok(ProviderSpec {
        id: id.into(),
        mailbox_place: mailbox_place.into(),
        readiness,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderPrefilterRequest {
    pub provider_id: String,
    pub mailbox_place: String,
    pub folder: String,
    pub exact_allowed_senders: Vec<String>,
    pub shipping_query: String,
    pub response_fields: Vec<String>,
}

fn provider_prefilter_request_from_senders(
    provider_id: &str,
    allowed_senders: &[String],
) -> Result<ProviderPrefilterRequest, BoundaryFailure> {
    let spec = provider_spec(provider_id).map_err(|reason| failure(provider_id, reason))?;
    if spec.readiness != Readiness::Ready {
        return Err(failure(
            provider_id,
            format!(
                "NotReady: provider-side sender/folder prefilter unavailable; refusing access to {}",
                spec.mailbox_place
            ),
        ));
    }
    if allowed_senders.is_empty() {
        return Err(failure(provider_id, "allowed sender inventory is empty"));
    }
    let mut senders = BTreeSet::new();
    for sender in allowed_senders {
        let normalized = normalize_sender(sender).ok_or_else(|| {
            failure(
                provider_id,
                format!("unsafe or non-exact allowed sender address {sender:?}"),
            )
        })?;
        senders.insert(normalized);
    }
    let exact_allowed_senders: Vec<_> = senders.into_iter().collect();
    let clauses = exact_allowed_senders
        .iter()
        .map(|sender| format!("from:{sender}"))
        .collect::<Vec<_>>()
        .join(" ");
    Ok(ProviderPrefilterRequest {
        provider_id: spec.id,
        mailbox_place: spec.mailbox_place,
        folder: "INBOX".into(),
        exact_allowed_senders,
        shipping_query: format!("in:inbox {{{clauses}}}"),
        response_fields: vec!["message_id".into()],
    })
}

/// Opaque authorization minted only by resolving the application's friend-authority lookup.
/// Callers cannot construct or alter one and the shipping reader never accepts sender strings.
#[derive(Debug, Clone)]
pub struct AllowedSenderGrant {
    provider_id: String,
    request: ProviderPrefilterRequest,
}

pub trait AllowedSenderLookup {
    type Error: fmt::Display;

    fn exact_allowed_senders(
        &mut self,
        provider_id: &str,
        mailbox_place: &str,
    ) -> Result<Vec<String>, Self::Error>;
}

pub fn lookup_allowed_sender_grant<L: AllowedSenderLookup>(
    provider_id: &str,
    lookup: &mut L,
) -> Result<AllowedSenderGrant, BoundaryFailure> {
    let spec = provider_spec(provider_id).map_err(|reason| failure(provider_id, reason))?;
    if spec.readiness != Readiness::Ready {
        return Err(failure(
            provider_id,
            format!(
                "NotReady: provider-side sender/folder prefilter unavailable; refusing access to {}",
                spec.mailbox_place
            ),
        ));
    }
    let senders = lookup
        .exact_allowed_senders(provider_id, &spec.mailbox_place)
        .map_err(|error| {
            failure(
                provider_id,
                format!("allowed-friend lookup failed: {error}"),
            )
        })?;
    let request = provider_prefilter_request_from_senders(provider_id, &senders)?;
    Ok(AllowedSenderGrant {
        provider_id: provider_id.into(),
        request,
    })
}

fn normalize_sender(sender: &str) -> Option<String> {
    let normalized = sender.trim().to_ascii_lowercase();
    let mut parts = normalized.split('@');
    let local = parts.next()?;
    let domain = parts.next()?;
    if parts.next().is_some()
        || local.is_empty()
        || domain.is_empty()
        || !domain.contains('.')
        || normalized.bytes().any(|byte| {
            !(byte.is_ascii_alphanumeric() || matches!(byte, b'@' | b'.' | b'_' | b'+' | b'-'))
        })
    {
        return None;
    }
    Some(normalized)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CandidateHeader {
    pub message_id: String,
    pub from: String,
    pub folder: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CandidateIdsResponse {
    pub message_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BodyResponse {
    pub message_id: String,
    pub body: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RawResponseKind {
    CandidateIds,
    Header,
    Body,
}

/// Provider transport. The only mailbox-enumeration call requires a policy-built prefilter.
pub trait ProviderMailbox {
    type Error: fmt::Display;

    fn candidate_ids(&mut self, request: &ProviderPrefilterRequest)
        -> Result<Vec<u8>, Self::Error>;

    fn header(&mut self, message_id: &str) -> Result<Vec<u8>, Self::Error>;

    fn body(&mut self, message_id: &str) -> Result<Vec<u8>, Self::Error>;
}

/// Called synchronously on the raw response bytes, before any serde parser sees those bytes.
pub trait ProcessEntryObserver {
    type Error: fmt::Display;

    fn observe_before_parsing(
        &mut self,
        provider_id: &str,
        boundary: &str,
        kind: RawResponseKind,
        raw_response: &[u8],
    ) -> Result<(), Self::Error>;
}

/// Shipping read pipeline. It has no unfiltered/late-filter entry point.
pub fn read_allowed_conversations<T, O>(
    provider_id: &str,
    grant: AllowedSenderGrant,
    transport: &mut T,
    observer: &mut O,
) -> Result<Vec<BodyResponse>, BoundaryFailure>
where
    T: ProviderMailbox,
    O: ProcessEntryObserver,
{
    if grant.provider_id != provider_id {
        return Err(failure(
            provider_id,
            "allowed-sender grant belongs to another provider",
        ));
    }
    let request = grant.request;
    let raw_ids = transport
        .candidate_ids(&request)
        .map_err(|error| failure(provider_id, format!("candidate query failed: {error}")))?;
    observer
        .observe_before_parsing(
            provider_id,
            PROCESS_ENTRY_BOUNDARY,
            RawResponseKind::CandidateIds,
            &raw_ids,
        )
        .map_err(|error| failure(provider_id, format!("boundary observer failed: {error}")))?;
    let candidates: CandidateIdsResponse = serde_json::from_slice(&raw_ids).map_err(|error| {
        failure(
            provider_id,
            format!("candidate response parse failed: {error}"),
        )
    })?;

    let allowed: BTreeSet<_> = request.exact_allowed_senders.iter().cloned().collect();
    let mut candidate_ids = BTreeSet::new();
    for message_id in &candidates.message_ids {
        if message_id.trim().is_empty() || !candidate_ids.insert(message_id.clone()) {
            return Err(failure(
                provider_id,
                format!(
                    "invalid or duplicate candidate ID at {}",
                    request.mailbox_place
                ),
            ));
        }
    }

    // Fetch and validate every candidate's metadata before the first body call. A header that
    // contradicts the provider-side filter aborts the batch without downloading any body.
    let mut headers = Vec::with_capacity(candidates.message_ids.len());
    for message_id in &candidates.message_ids {
        let raw_header = transport
            .header(message_id)
            .map_err(|error| failure(provider_id, format!("header fetch failed: {error}")))?;
        observer
            .observe_before_parsing(
                provider_id,
                PROCESS_ENTRY_BOUNDARY,
                RawResponseKind::Header,
                &raw_header,
            )
            .map_err(|error| failure(provider_id, format!("boundary observer failed: {error}")))?;
        let header: CandidateHeader = serde_json::from_slice(&raw_header).map_err(|error| {
            failure(
                provider_id,
                format!("header response parse failed: {error}"),
            )
        })?;
        if header.message_id != *message_id {
            return Err(failure(
                provider_id,
                format!(
                    "header response ID {} differs from candidate {} at {}",
                    header.message_id, message_id, request.mailbox_place
                ),
            ));
        }
        let sender = normalize_sender(&header.from).ok_or_else(|| {
            failure(
                provider_id,
                format!("invalid returned sender at {}", request.mailbox_place),
            )
        })?;
        if !allowed.contains(&sender) || header.folder != request.folder {
            return Err(failure(
                provider_id,
                format!(
                    "provider prefilter leaked header id={} sender={} folder={} at {}",
                    header.message_id, header.from, header.folder, request.mailbox_place
                ),
            ));
        }
        headers.push(header);
    }

    let mut bodies = Vec::with_capacity(headers.len());
    for candidate in &headers {
        // This loop is the sole body-call site; its IDs exist only after the prefilter validation.
        let raw_body = transport
            .body(&candidate.message_id)
            .map_err(|error| failure(provider_id, format!("body fetch failed: {error}")))?;
        observer
            .observe_before_parsing(
                provider_id,
                PROCESS_ENTRY_BOUNDARY,
                RawResponseKind::Body,
                &raw_body,
            )
            .map_err(|error| failure(provider_id, format!("boundary observer failed: {error}")))?;
        let body: BodyResponse = serde_json::from_slice(&raw_body).map_err(|error| {
            failure(provider_id, format!("body response parse failed: {error}"))
        })?;
        if body.message_id != candidate.message_id {
            return Err(failure(
                provider_id,
                format!(
                    "body response ID {} differs from authorized candidate {} at {}",
                    body.message_id, candidate.message_id, request.mailbox_place
                ),
            ));
        }
        bodies.push(body);
    }
    Ok(bodies)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuditEvidence {
    pub schema: String,
    pub origin: EvidenceOrigin,
    pub boundary: BoundaryDefinition,
    pub authoritative_provider_ids: Vec<String>,
    pub providers: Vec<ProviderEvidence>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceOrigin {
    RealProviderExternal,
    Fixture,
    Proxy,
    Simulated,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BoundaryDefinition {
    pub name: String,
    pub defined_before_response_parsing: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderEvidence {
    pub provider_id: String,
    pub mailbox_place: String,
    pub readiness: Readiness,
    pub access_count: u64,
    pub refusal: Option<String>,
    pub mailbox: Option<FreshMailboxEvidence>,
    pub provider_request_log: Option<ProviderRequestLog>,
    pub post_boundary_surfaces: Vec<SurfaceObservation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FreshMailboxEvidence {
    pub real_provider_mailbox: bool,
    pub run_nonce: String,
    pub allowed: Vec<MailMarker>,
    pub disallowed: Vec<MailMarker>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MailMarker {
    pub id: String,
    pub full_marker: String,
    pub sender: String,
    pub fresh: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderRequestLog {
    pub observer: String,
    pub externally_observed: bool,
    pub shipping_query: String,
    pub query_returned_ids: Vec<String>,
    pub returned_headers: Vec<CandidateHeader>,
    pub body_fetch_ids: Vec<String>,
    pub request_order: Vec<ProviderRequestEvent>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderRequestEvent {
    pub kind: ProviderRequestKind,
    pub message_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderRequestKind {
    CandidateQuery,
    Header,
    Body,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SurfaceKind {
    Memory,
    Logs,
    CrashOutput,
    Caches,
    Queues,
    PersistentStores,
}

pub const REQUIRED_SURFACES: [SurfaceKind; 6] = [
    SurfaceKind::Memory,
    SurfaceKind::Logs,
    SurfaceKind::CrashOutput,
    SurfaceKind::Caches,
    SurfaceKind::Queues,
    SurfaceKind::PersistentStores,
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SurfaceObservation {
    pub kind: SurfaceKind,
    pub observer: String,
    pub independent: bool,
    pub bounded_scope: String,
    pub observed_full_markers: Vec<String>,
    pub observed_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditSummary {
    pub providers: usize,
    pub ready_providers: usize,
    pub allowed_messages_per_ready: usize,
    pub disallowed_messages_per_ready: usize,
    pub body_crossings_per_ready: usize,
    pub surfaces_per_provider: usize,
    pub disallowed_post_boundary_hits: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundaryFailure {
    pub provider_id: String,
    pub boundary: &'static str,
    pub reason: String,
}

impl fmt::Display for BoundaryFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "provider_id={} boundary={} failure={}",
            self.provider_id, self.boundary, self.reason
        )
    }
}

impl std::error::Error for BoundaryFailure {}

fn failure(provider_id: impl Into<String>, reason: impl Into<String>) -> BoundaryFailure {
    BoundaryFailure {
        provider_id: provider_id.into(),
        boundary: PROCESS_ENTRY_BOUNDARY,
        reason: reason.into(),
    }
}

pub fn audit_live_evidence(evidence: &AuditEvidence) -> Result<AuditSummary, BoundaryFailure> {
    if evidence.schema != EVIDENCE_SCHEMA {
        return Err(failure(
            "inventory",
            "wrong or missing live-evidence schema",
        ));
    }
    if evidence.origin != EvidenceOrigin::RealProviderExternal {
        return Err(failure(
            "inventory",
            "fixtures, proxies, and simulated evidence are forbidden",
        ));
    }
    if evidence.boundary.name != PROCESS_ENTRY_BOUNDARY
        || !evidence.boundary.defined_before_response_parsing
    {
        return Err(failure(
            "inventory",
            "process-entry boundary is missing or not before response parsing",
        ));
    }

    let inventory = authoritative_inventory();
    let expected_ids: BTreeSet<_> = inventory.iter().map(|spec| spec.id.as_str()).collect();
    let inventoried_ids: BTreeSet<_> = evidence
        .authoritative_provider_ids
        .iter()
        .map(String::as_str)
        .collect();
    if evidence.authoritative_provider_ids.is_empty() {
        return Err(failure(
            "gmail",
            "provider inventory is empty; Ready provider gmail is omitted",
        ));
    }
    if evidence.authoritative_provider_ids.len() != inventoried_ids.len()
        || inventoried_ids != expected_ids
    {
        let missing = expected_ids
            .difference(&inventoried_ids)
            .next()
            .copied()
            .unwrap_or("inventory");
        return Err(failure(
            missing,
            "provider inventory is empty, duplicate, or incomplete",
        ));
    }
    if evidence.providers.is_empty() {
        return Err(failure("inventory", "provider evidence inventory is empty"));
    }
    let providers: BTreeMap<_, _> = evidence
        .providers
        .iter()
        .map(|provider| (provider.provider_id.as_str(), provider))
        .collect();
    if providers.len() != evidence.providers.len()
        || providers.keys().copied().collect::<BTreeSet<_>>() != expected_ids
    {
        let missing = expected_ids
            .iter()
            .find(|id| !providers.contains_key(**id))
            .copied()
            .unwrap_or("inventory");
        return Err(failure(
            missing,
            "provider evidence is duplicate, extra, or omitted",
        ));
    }

    let mut ready_count = 0;
    for spec in &inventory {
        let provider = providers[spec.id.as_str()];
        if provider.mailbox_place != spec.mailbox_place || provider.readiness != spec.readiness {
            return Err(failure(
                &spec.id,
                format!(
                    "readiness or mailbox place differs from policy ({})",
                    spec.mailbox_place
                ),
            ));
        }
        audit_surface_inventory(provider, &[])?;
        match spec.readiness {
            Readiness::NotReady => {
                if provider.access_count != 0 {
                    return Err(failure(
                        &spec.id,
                        format!("NotReady provider accessed {}", spec.mailbox_place),
                    ));
                }
                let refusal = provider.refusal.as_deref().unwrap_or_default();
                if !refusal.contains(&spec.id) || !refusal.contains(&spec.mailbox_place) {
                    return Err(failure(
                        &spec.id,
                        format!("refusal does not name mailbox place {}", spec.mailbox_place),
                    ));
                }
                if provider.mailbox.is_some() || provider.provider_request_log.is_some() {
                    return Err(failure(
                        &spec.id,
                        "NotReady provider must have no mailbox access evidence",
                    ));
                }
            }
            Readiness::Ready => {
                ready_count += 1;
                audit_ready_provider(provider)?;
            }
        }
    }
    if ready_count == 0 {
        return Err(failure("inventory", "Ready provider inventory is empty"));
    }

    Ok(AuditSummary {
        providers: inventory.len(),
        ready_providers: ready_count,
        allowed_messages_per_ready: ALLOWED_MESSAGE_COUNT,
        disallowed_messages_per_ready: DISALLOWED_MESSAGE_COUNT,
        body_crossings_per_ready: ALLOWED_MESSAGE_COUNT,
        surfaces_per_provider: REQUIRED_SURFACES.len(),
        disallowed_post_boundary_hits: 0,
    })
}

fn audit_ready_provider(provider: &ProviderEvidence) -> Result<(), BoundaryFailure> {
    let mailbox = provider.mailbox.as_ref().ok_or_else(|| {
        failure(
            &provider.provider_id,
            "Ready provider mailbox evidence omitted",
        )
    })?;
    if !mailbox.real_provider_mailbox || mailbox.run_nonce.trim().is_empty() {
        return Err(failure(
            &provider.provider_id,
            "mailbox is not a fresh real-provider run",
        ));
    }
    if mailbox.allowed.len() != ALLOWED_MESSAGE_COUNT
        || mailbox.disallowed.len() != DISALLOWED_MESSAGE_COUNT
    {
        return Err(failure(
            &provider.provider_id,
            format!(
                "real mailbox counts must be allowed={} disallowed={}, found allowed={} disallowed={}",
                ALLOWED_MESSAGE_COUNT,
                DISALLOWED_MESSAGE_COUNT,
                mailbox.allowed.len(),
                mailbox.disallowed.len()
            ),
        ));
    }
    let all_messages = mailbox.allowed.iter().chain(&mailbox.disallowed);
    let ids: BTreeSet<_> = all_messages
        .clone()
        .map(|message| message.id.as_str())
        .collect();
    let markers: BTreeSet<_> = all_messages
        .clone()
        .map(|message| message.full_marker.as_str())
        .collect();
    if ids.len() != ALLOWED_MESSAGE_COUNT + DISALLOWED_MESSAGE_COUNT
        || markers.len() != ALLOWED_MESSAGE_COUNT + DISALLOWED_MESSAGE_COUNT
        || all_messages.clone().any(|message| {
            !message.fresh
                || message.id.trim().is_empty()
                || message.full_marker.trim().is_empty()
                || normalize_sender(&message.sender).is_none()
        })
    {
        return Err(failure(
            &provider.provider_id,
            "mail markers/IDs must be unique, nonempty, and fresh",
        ));
    }

    let allowed_ids: BTreeSet<_> = mailbox
        .allowed
        .iter()
        .map(|message| message.id.as_str())
        .collect();
    let disallowed_ids: BTreeSet<_> = mailbox
        .disallowed
        .iter()
        .map(|message| message.id.as_str())
        .collect();
    let disallowed_markers: BTreeSet<_> = mailbox
        .disallowed
        .iter()
        .map(|message| message.full_marker.as_str())
        .collect();
    let senders: Vec<_> = mailbox
        .allowed
        .iter()
        .map(|message| message.sender.clone())
        .collect();
    let expected_request =
        provider_prefilter_request_from_senders(&provider.provider_id, &senders)?;
    let log = provider.provider_request_log.as_ref().ok_or_else(|| {
        failure(
            &provider.provider_id,
            "externally observed provider request log omitted",
        )
    })?;
    if !log.externally_observed || log.observer.trim().is_empty() {
        return Err(failure(
            &provider.provider_id,
            "provider request log is not externally observed",
        ));
    }
    if log.shipping_query != expected_request.shipping_query {
        return Err(failure(
            &provider.provider_id,
            "shipping query is not the exact provider-side sender/Inbox query",
        ));
    }
    let query_ids: BTreeSet<_> = log.query_returned_ids.iter().map(String::as_str).collect();
    if log.query_returned_ids.len() != ALLOWED_MESSAGE_COUNT || query_ids != allowed_ids {
        let leaked = query_ids.intersection(&disallowed_ids).next().copied();
        return Err(failure(
            &provider.provider_id,
            format!(
                "provider query did not return only the 5 allowed IDs{}",
                leaked
                    .map(|id| format!("; disallowed id={id}"))
                    .unwrap_or_default()
            ),
        ));
    }
    if log.returned_headers.len() != ALLOWED_MESSAGE_COUNT {
        return Err(failure(
            &provider.provider_id,
            "provider query did not return exactly 5 allowed headers",
        ));
    }
    let allowed_senders: BTreeSet<_> = expected_request.exact_allowed_senders.iter().collect();
    let header_ids: BTreeSet<_> = log
        .returned_headers
        .iter()
        .map(|header| header.message_id.as_str())
        .collect();
    if header_ids != allowed_ids
        || log.returned_headers.iter().any(|header| {
            header.folder != "INBOX"
                || normalize_sender(&header.from)
                    .as_ref()
                    .is_none_or(|sender| !allowed_senders.contains(sender))
        })
    {
        let leaked = header_ids.intersection(&disallowed_ids).next().copied();
        return Err(failure(
            &provider.provider_id,
            format!(
                "a returned header is disallowed or outside Inbox{}",
                leaked
                    .map(|id| format!("; disallowed id={id}"))
                    .unwrap_or_default()
            ),
        ));
    }
    let body_ids: BTreeSet<_> = log.body_fetch_ids.iter().map(String::as_str).collect();
    if log.body_fetch_ids.len() != ALLOWED_MESSAGE_COUNT || body_ids != allowed_ids {
        let leaked = body_ids.intersection(&disallowed_ids).next().copied();
        return Err(failure(
            &provider.provider_id,
            format!(
                "body crossings must be exactly the 5 allowed IDs{}",
                leaked
                    .map(|id| format!("; disallowed id={id}"))
                    .unwrap_or_default()
            ),
        ));
    }
    if provider.access_count != 1 + (2 * ALLOWED_MESSAGE_COUNT) as u64 {
        return Err(failure(
            &provider.provider_id,
            "access count must be one prefilter query plus five headers plus five bodies",
        ));
    }
    let mut expected_order = vec![ProviderRequestEvent {
        kind: ProviderRequestKind::CandidateQuery,
        message_id: None,
    }];
    expected_order.extend(
        log.returned_headers
            .iter()
            .map(|header| ProviderRequestEvent {
                kind: ProviderRequestKind::Header,
                message_id: Some(header.message_id.clone()),
            }),
    );
    expected_order.extend(
        log.body_fetch_ids
            .iter()
            .map(|message_id| ProviderRequestEvent {
                kind: ProviderRequestKind::Body,
                message_id: Some(message_id.clone()),
            }),
    );
    if log.request_order != expected_order {
        return Err(failure(
            &provider.provider_id,
            "provider request order must be query, all five headers, then only five bodies",
        ));
    }
    audit_surface_inventory(provider, &[&disallowed_ids, &disallowed_markers])
}

fn audit_surface_inventory(
    provider: &ProviderEvidence,
    forbidden_sets: &[&BTreeSet<&str>],
) -> Result<(), BoundaryFailure> {
    if provider.post_boundary_surfaces.is_empty() {
        return Err(failure(
            &provider.provider_id,
            "post-boundary surface inventory is empty",
        ));
    }
    let surfaces: BTreeMap<_, _> = provider
        .post_boundary_surfaces
        .iter()
        .map(|surface| (surface.kind, surface))
        .collect();
    if surfaces.len() != provider.post_boundary_surfaces.len()
        || surfaces.keys().copied().collect::<BTreeSet<_>>()
            != REQUIRED_SURFACES.into_iter().collect()
    {
        let missing = REQUIRED_SURFACES
            .iter()
            .find(|kind| !surfaces.contains_key(kind))
            .copied();
        return Err(failure(
            &provider.provider_id,
            format!("post-boundary surface inventory missing {missing:?}"),
        ));
    }
    for surface in surfaces.values() {
        if !surface.independent
            || surface.observer.trim().is_empty()
            || surface.bounded_scope.trim().is_empty()
        {
            return Err(failure(
                &provider.provider_id,
                format!(
                    "surface {:?} lacks an independent bounded observer",
                    surface.kind
                ),
            ));
        }
        for forbidden in forbidden_sets {
            if let Some(hit) = surface
                .observed_ids
                .iter()
                .chain(&surface.observed_full_markers)
                .find(|value| forbidden.contains(value.as_str()))
            {
                return Err(failure(
                    &provider.provider_id,
                    format!(
                        "surface {:?} contains disallowed marker/ID {hit}",
                        surface.kind
                    ),
                ));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[derive(Default)]
    struct Lookup {
        calls: Vec<(String, String)>,
    }

    impl AllowedSenderLookup for Lookup {
        type Error = &'static str;

        fn exact_allowed_senders(
            &mut self,
            provider_id: &str,
            mailbox_place: &str,
        ) -> Result<Vec<String>, Self::Error> {
            self.calls
                .push((provider_id.to_owned(), mailbox_place.to_owned()));
            Ok(vec!["ALICE@example.test".into(), "bob@example.test".into()])
        }
    }

    struct Mailbox {
        ids: Vec<u8>,
        headers: BTreeMap<String, Vec<u8>>,
        calls: Vec<String>,
    }

    impl ProviderMailbox for Mailbox {
        type Error = &'static str;

        fn candidate_ids(
            &mut self,
            request: &ProviderPrefilterRequest,
        ) -> Result<Vec<u8>, Self::Error> {
            self.calls.push(format!("query:{}", request.shipping_query));
            Ok(self.ids.clone())
        }

        fn header(&mut self, message_id: &str) -> Result<Vec<u8>, Self::Error> {
            self.calls.push(format!("header:{message_id}"));
            Ok(self.headers[message_id].clone())
        }

        fn body(&mut self, message_id: &str) -> Result<Vec<u8>, Self::Error> {
            self.calls.push(format!("body:{message_id}"));
            Ok(serde_json::to_vec(&BodyResponse {
                message_id: message_id.into(),
                body: format!("body-{message_id}"),
            })
            .unwrap())
        }
    }

    #[derive(Default)]
    struct Observer(Vec<(RawResponseKind, Vec<u8>)>);

    impl ProcessEntryObserver for Observer {
        type Error = &'static str;

        fn observe_before_parsing(
            &mut self,
            _provider_id: &str,
            boundary: &str,
            kind: RawResponseKind,
            raw_response: &[u8],
        ) -> Result<(), Self::Error> {
            assert_eq!(boundary, PROCESS_ENTRY_BOUNDARY);
            self.0.push((kind, raw_response.to_vec()));
            Ok(())
        }
    }

    fn mailbox(candidate_headers: Vec<CandidateHeader>) -> Mailbox {
        let ids = serde_json::to_vec(&CandidateIdsResponse {
            message_ids: candidate_headers
                .iter()
                .map(|header| header.message_id.clone())
                .collect(),
        })
        .unwrap();
        let headers = candidate_headers
            .into_iter()
            .map(|header| {
                (
                    header.message_id.clone(),
                    serde_json::to_vec(&header).unwrap(),
                )
            })
            .collect();
        Mailbox {
            ids,
            headers,
            calls: vec![],
        }
    }

    #[test]
    fn authoritative_inventory_is_derived_as_eleven_identities_with_one_ready_reader() {
        let inventory = authoritative_inventory();
        assert_eq!(inventory.len(), 11);
        assert_eq!(
            inventory
                .iter()
                .filter(|provider| provider.readiness == Readiness::Ready)
                .map(|provider| provider.id.as_str())
                .collect::<Vec<_>>(),
            ["gmail"]
        );
        assert!(inventory
            .iter()
            .any(|provider| provider.id == "outlook-web"));
        assert!(inventory
            .iter()
            .any(|provider| provider.id == "outlook-desktop"));
    }

    #[test]
    fn shipping_pipeline_queries_first_observes_raw_bytes_and_fetches_only_candidates() {
        let mut lookup = Lookup::default();
        let grant = lookup_allowed_sender_grant("gmail", &mut lookup).unwrap();
        let mut mailbox = Mailbox {
            ..mailbox(vec![
                CandidateHeader {
                    message_id: "a".into(),
                    from: "alice@example.test".into(),
                    folder: "INBOX".into(),
                },
                CandidateHeader {
                    message_id: "b".into(),
                    from: "BOB@example.test".into(),
                    folder: "INBOX".into(),
                },
            ])
        };
        let mut observer = Observer::default();

        let bodies =
            read_allowed_conversations("gmail", grant, &mut mailbox, &mut observer).unwrap();

        assert_eq!(lookup.calls, [("gmail".into(), "Gmail Inbox".into())]);
        assert_eq!(bodies.len(), 2);
        assert_eq!(
            mailbox.calls,
            [
                "query:in:inbox {from:alice@example.test from:bob@example.test}",
                "header:a",
                "header:b",
                "body:a",
                "body:b"
            ]
        );
        assert_eq!(
            observer.0.iter().map(|entry| entry.0).collect::<Vec<_>>(),
            [
                RawResponseKind::CandidateIds,
                RawResponseKind::Header,
                RawResponseKind::Header,
                RawResponseKind::Body,
                RawResponseKind::Body
            ]
        );
    }

    #[test]
    fn disallowed_header_is_observed_then_refused_before_any_body_fetch() {
        let grant = lookup_allowed_sender_grant("gmail", &mut Lookup::default()).unwrap();
        let mut mailbox = mailbox(vec![CandidateHeader {
            message_id: "disallowed-id".into(),
            from: "mallory@example.test".into(),
            folder: "INBOX".into(),
        }]);
        let mut observer = Observer::default();
        let error =
            read_allowed_conversations("gmail", grant, &mut mailbox, &mut observer).unwrap_err();
        assert!(error
            .to_string()
            .contains("provider prefilter leaked header"));
        assert_eq!(mailbox.calls.len(), 2);
        assert_eq!(observer.0.len(), 2);
        assert!(!mailbox.calls.iter().any(|call| call.starts_with("body:")));
    }

    #[test]
    fn malformed_response_is_observed_before_parse_failure() {
        let grant = lookup_allowed_sender_grant("gmail", &mut Lookup::default()).unwrap();
        let mut mailbox = Mailbox {
            ids: b"not-json".to_vec(),
            headers: BTreeMap::new(),
            calls: vec![],
        };
        let mut observer = Observer::default();
        let error =
            read_allowed_conversations("gmail", grant, &mut mailbox, &mut observer).unwrap_err();
        assert_eq!(observer.0[0].1, b"not-json");
        assert!(error
            .to_string()
            .contains("candidate response parse failed"));
    }

    #[test]
    fn unsupported_provider_refuses_before_friend_lookup_and_names_place() {
        let mut lookup = Lookup::default();
        let error = lookup_allowed_sender_grant("outlook-desktop", &mut lookup).unwrap_err();
        let rendered = error.to_string();
        assert!(rendered.contains("provider_id=outlook-desktop"));
        assert!(rendered.contains("Outlook desktop Inbox"));
        assert!(lookup.calls.is_empty());
    }
}
