//! Acceptance evidence verifier for TASK 5902.
//!
//! The older 4350 audit proves that a provider-side selector exists. This audit
//! deliberately treats that selector as untrusted and proves that body access
//! is instead gated by an OSL sender proof.

use super::{
    authoritative_inventory, failure, BoundaryFailure, EvidenceOrigin, Readiness,
    PROCESS_ENTRY_BOUNDARY,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const TASK_5902_SCHEMA: &str = "osl-task-5902-authenticated-mail-sender-v1";
pub const FAILED_SENDER_PROOF: &str = "failed cryptographic sender proof";
pub const MAX_CANDIDATE_RESPONSE_BYTES: u64 = 64 * 1024;
pub const MAX_PROOF_ENVELOPE_BYTES: u64 = 8 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Task5902Evidence {
    pub schema: String,
    pub origin: EvidenceOrigin,
    pub authoritative_provider_ids: Vec<String>,
    pub providers: Vec<Provider5902Evidence>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Provider5902Evidence {
    pub provider_id: String,
    pub mailbox_place: String,
    pub readiness: Readiness,
    pub real_provider_mailbox: bool,
    pub metadata_prebody_supported: bool,
    pub access_count: u64,
    pub not_ready_refusal: Option<String>,
    pub provider_log: Option<Provider5902RequestLog>,
    pub process_entry_observer: Option<ProcessEntry5902Observer>,
    pub candidates: Vec<ProofCandidateEvidence>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Provider5902RequestLog {
    pub observer: String,
    pub externally_observed: bool,
    pub events: Vec<Provider5902Event>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProcessEntry5902Observer {
    pub observer: String,
    pub independent: bool,
    pub boundary: String,
    pub events: Vec<ProcessEntry5902Event>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Provider5902Event {
    pub kind: Task5902EventKind,
    pub message_id: Option<String>,
    pub transferred_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProcessEntry5902Event {
    pub kind: Task5902EventKind,
    pub message_id: Option<String>,
    pub crossed_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Task5902EventKind {
    CandidateQuery,
    ProofEnvelope,
    BodyCiphertext,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SenderScenario {
    GenuineAllowedProof,
    ForgedFrom,
    AllowedAlias,
    ForwardingRewrite,
    CompromisedAllowedMailboxWithoutFriendKey,
}

pub const REQUIRED_SCENARIOS: [SenderScenario; 5] = [
    SenderScenario::GenuineAllowedProof,
    SenderScenario::ForgedFrom,
    SenderScenario::AllowedAlias,
    SenderScenario::ForwardingRewrite,
    SenderScenario::CompromisedAllowedMailboxWithoutFriendKey,
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProofCandidateEvidence {
    pub message_id: String,
    pub scenario: SenderScenario,
    pub provider_sender_and_folder_match: bool,
    pub proof_envelope_bytes: u64,
    pub checks: ProofCheckEvidence,
    pub authenticated: bool,
    pub body_fetch_bytes: u64,
    pub process_entry_body_bytes: u64,
    pub refusal: Option<String>,
    /// An external canary used only to ensure refusal diagnostics reveal no content.
    pub confidential_content_canary: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProofCheckEvidence {
    pub established_friend_key_checked: bool,
    pub signature_checked: bool,
    pub signature_valid: bool,
    pub provider_id_bound: bool,
    pub provider_message_id_bound: bool,
    pub recipient_bound: bool,
    pub conversation_bound: bool,
    pub body_ciphertext_digest_bound: bool,
    pub proof_verified_before_body_fetch: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task5902Summary {
    pub providers: usize,
    pub candidate_ready: usize,
    pub genuine_bodies: usize,
    pub hostile_attacks: usize,
    pub hostile_body_fetch_bytes: u64,
    pub hostile_process_entry_body_bytes: u64,
    pub refusals: usize,
}

fn audit_failure(
    provider_id: &str,
    hostile_id: &str,
    first_crossed_byte: &str,
    reason: impl AsRef<str>,
) -> BoundaryFailure {
    failure(
        provider_id,
        format!(
            "hostile_id={hostile_id} first_crossed_byte={first_crossed_byte} {}",
            reason.as_ref()
        ),
    )
}

pub fn audit_task_5902(evidence: &Task5902Evidence) -> Result<Task5902Summary, BoundaryFailure> {
    if evidence.schema != TASK_5902_SCHEMA {
        return Err(audit_failure("gmail", "inventory", "none", "wrong schema"));
    }
    if evidence.origin != EvidenceOrigin::RealProviderExternal {
        return Err(audit_failure(
            "gmail",
            "inventory",
            "none",
            "fixture, proxy, or simulated provider evidence is forbidden",
        ));
    }
    let inventory = authoritative_inventory();
    let expected_ids: BTreeSet<_> = inventory
        .iter()
        .map(|provider| provider.id.as_str())
        .collect();
    let inventoried_ids: BTreeSet<_> = evidence
        .authoritative_provider_ids
        .iter()
        .map(String::as_str)
        .collect();
    if evidence.authoritative_provider_ids.is_empty()
        || evidence.authoritative_provider_ids.len() != inventoried_ids.len()
        || inventoried_ids != expected_ids
    {
        let missing = expected_ids
            .difference(&inventoried_ids)
            .next()
            .copied()
            .unwrap_or("gmail");
        return Err(audit_failure(
            missing,
            "inventory",
            "none",
            "provider inventory is empty, duplicate, or incomplete",
        ));
    }
    let providers: BTreeMap<_, _> = evidence
        .providers
        .iter()
        .map(|provider| (provider.provider_id.as_str(), provider))
        .collect();
    if evidence.providers.len() != providers.len()
        || providers.keys().copied().collect::<BTreeSet<_>>() != expected_ids
    {
        let missing = expected_ids
            .iter()
            .find(|id| !providers.contains_key(**id))
            .copied()
            .unwrap_or("gmail");
        return Err(audit_failure(
            missing,
            "inventory",
            "none",
            "provider evidence is empty, duplicate, or incomplete",
        ));
    }

    let mut candidate_ready = 0;
    let mut genuine_bodies = 0;
    let mut hostile_attacks = 0;
    let mut refusals = 0;
    for spec in &inventory {
        let provider = providers[spec.id.as_str()];
        if provider.mailbox_place != spec.mailbox_place || provider.readiness != spec.readiness {
            return Err(audit_failure(
                &spec.id,
                "inventory",
                "none",
                "readiness or mailbox place differs from policy",
            ));
        }
        match spec.readiness {
            Readiness::NotReady => audit_not_ready(provider)?,
            Readiness::Ready => {
                candidate_ready += 1;
                let (genuine, hostile, rejected) = audit_ready(provider)?;
                genuine_bodies += genuine;
                hostile_attacks += hostile;
                refusals += rejected;
            }
        }
    }
    if candidate_ready == 0 {
        return Err(audit_failure(
            "inventory",
            "inventory",
            "none",
            "candidate-Ready provider inventory is empty",
        ));
    }
    Ok(Task5902Summary {
        providers: inventory.len(),
        candidate_ready,
        genuine_bodies,
        hostile_attacks,
        hostile_body_fetch_bytes: 0,
        hostile_process_entry_body_bytes: 0,
        refusals,
    })
}

fn audit_not_ready(provider: &Provider5902Evidence) -> Result<(), BoundaryFailure> {
    if provider.real_provider_mailbox
        || provider.metadata_prebody_supported
        || provider.access_count != 0
        || provider.provider_log.is_some()
        || provider.process_entry_observer.is_some()
        || !provider.candidates.is_empty()
    {
        return Err(audit_failure(
            &provider.provider_id,
            "not_ready",
            "1",
            "provider without authenticated pre-body metadata was accessed",
        ));
    }
    let refusal = provider.not_ready_refusal.as_deref().unwrap_or_default();
    if !refusal.contains(&provider.provider_id)
        || !refusal.contains(&provider.mailbox_place)
        || !refusal.contains("cannot expose authenticated pre-body sender-proof metadata")
    {
        return Err(audit_failure(
            &provider.provider_id,
            "not_ready",
            "none",
            "NotReady refusal is missing provider, mailbox, or metadata reason",
        ));
    }
    Ok(())
}

fn audit_ready(provider: &Provider5902Evidence) -> Result<(usize, usize, usize), BoundaryFailure> {
    if !provider.real_provider_mailbox || !provider.metadata_prebody_supported {
        return Err(audit_failure(
            &provider.provider_id,
            "inventory",
            "none",
            "candidate-Ready evidence is not from a real metadata-capable provider",
        ));
    }
    let log = provider.provider_log.as_ref().ok_or_else(|| {
        audit_failure(
            &provider.provider_id,
            "inventory",
            "none",
            "external provider request log is missing",
        )
    })?;
    if !log.externally_observed || log.observer.trim().is_empty() {
        return Err(audit_failure(
            &provider.provider_id,
            "inventory",
            "none",
            "provider request log is not externally observed",
        ));
    }
    let observer = provider.process_entry_observer.as_ref().ok_or_else(|| {
        audit_failure(
            &provider.provider_id,
            "inventory",
            "none",
            "independent process-entry observer is missing",
        )
    })?;
    if !observer.independent
        || observer.observer.trim().is_empty()
        || observer.boundary != PROCESS_ENTRY_BOUNDARY
    {
        return Err(audit_failure(
            &provider.provider_id,
            "inventory",
            "none",
            "process-entry observer is not independent or uses the wrong boundary",
        ));
    }
    if provider.candidates.is_empty() {
        return Err(audit_failure(
            &provider.provider_id,
            "inventory",
            "none",
            "candidate inventory is empty",
        ));
    }
    let scenarios: BTreeMap<_, _> = provider
        .candidates
        .iter()
        .map(|candidate| (candidate.scenario, candidate))
        .collect();
    if scenarios.len() != provider.candidates.len() {
        return Err(audit_failure(
            &provider.provider_id,
            "duplicate",
            "none",
            "candidate scenarios are duplicated",
        ));
    }
    for required in REQUIRED_SCENARIOS {
        if !scenarios.contains_key(&required) {
            return Err(audit_failure(
                &provider.provider_id,
                scenario_name(required),
                "none",
                "required genuine/hostile scenario is omitted",
            ));
        }
    }
    if scenarios.len() != REQUIRED_SCENARIOS.len() {
        return Err(audit_failure(
            &provider.provider_id,
            "inventory",
            "none",
            "candidate scenario inventory has extras",
        ));
    }

    let mut ids = BTreeSet::new();
    let mut genuine_id = None;
    let mut refusals = 0;
    for candidate in &provider.candidates {
        let hostile_id = candidate.message_id.as_str();
        if hostile_id.trim().is_empty() || !ids.insert(hostile_id) {
            return Err(audit_failure(
                &provider.provider_id,
                hostile_id,
                "none",
                "message ID is empty or duplicate",
            ));
        }
        if !candidate.provider_sender_and_folder_match {
            return Err(audit_failure(
                &provider.provider_id,
                hostile_id,
                "none",
                "provider sender/folder fields did not match the allowed query",
            ));
        }
        if candidate.proof_envelope_bytes == 0
            || candidate.proof_envelope_bytes > MAX_PROOF_ENVELOPE_BYTES
        {
            return Err(audit_failure(
                &provider.provider_id,
                hostile_id,
                "none",
                "proof envelope fetch is empty or unbounded",
            ));
        }
        audit_checks(provider, candidate)?;
        if candidate.scenario == SenderScenario::GenuineAllowedProof {
            genuine_id = Some(hostile_id);
            if !candidate.authenticated
                || !candidate.checks.signature_valid
                || candidate.body_fetch_bytes == 0
                || candidate.process_entry_body_bytes == 0
                || candidate.body_fetch_bytes != candidate.process_entry_body_bytes
                || candidate.refusal.is_some()
            {
                return Err(audit_failure(
                    &provider.provider_id,
                    hostile_id,
                    if candidate.process_entry_body_bytes > 0 {
                        "1"
                    } else {
                        "none"
                    },
                    "genuine proof/body result is incomplete",
                ));
            }
        } else {
            if candidate.authenticated || candidate.checks.signature_valid {
                return Err(audit_failure(
                    &provider.provider_id,
                    hostile_id,
                    if candidate.process_entry_body_bytes > 0 {
                        "1"
                    } else {
                        "none"
                    },
                    "hostile allowed-looking message was authenticated",
                ));
            }
            if candidate.body_fetch_bytes != 0 || candidate.process_entry_body_bytes != 0 {
                return Err(audit_failure(
                    &provider.provider_id,
                    hostile_id,
                    "1",
                    "hostile body crossed before cryptographic sender proof",
                ));
            }
            let exact = format!(
                "provider_id={} message_id={} failure={}",
                provider.provider_id, candidate.message_id, FAILED_SENDER_PROOF
            );
            if candidate.refusal.as_deref() != Some(exact.as_str())
                || (!candidate.confidential_content_canary.is_empty()
                    && exact.contains(&candidate.confidential_content_canary))
            {
                return Err(audit_failure(
                    &provider.provider_id,
                    hostile_id,
                    "none",
                    "refusal did not name only provider, hostile ID, and failed cryptographic sender proof",
                ));
            }
            refusals += 1;
        }
    }
    let genuine_id = genuine_id.expect("required scenario checked above");
    audit_event_streams(provider, log, observer, genuine_id)?;
    Ok((1, 4, refusals))
}

fn audit_checks(
    provider: &Provider5902Evidence,
    candidate: &ProofCandidateEvidence,
) -> Result<(), BoundaryFailure> {
    let checks = &candidate.checks;
    if !checks.established_friend_key_checked
        || !checks.signature_checked
        || !checks.provider_id_bound
        || !checks.provider_message_id_bound
        || !checks.recipient_bound
        || !checks.conversation_bound
        || !checks.body_ciphertext_digest_bound
        || !checks.proof_verified_before_body_fetch
    {
        return Err(audit_failure(
            &provider.provider_id,
            &candidate.message_id,
            if candidate.process_entry_body_bytes > 0 { "1" } else { "none" },
            "sender proof starved signature/digest/conversation/provider/message/recipient binding or ran after body fetch",
        ));
    }
    Ok(())
}

fn audit_event_streams(
    provider: &Provider5902Evidence,
    log: &Provider5902RequestLog,
    observer: &ProcessEntry5902Observer,
    genuine_id: &str,
) -> Result<(), BoundaryFailure> {
    let mut expected_log = vec![Provider5902Event {
        kind: Task5902EventKind::CandidateQuery,
        message_id: None,
        transferred_bytes: log
            .events
            .first()
            .map(|event| event.transferred_bytes)
            .unwrap_or(0),
    }];
    let query_bytes = expected_log[0].transferred_bytes;
    if query_bytes == 0 || query_bytes > MAX_CANDIDATE_RESPONSE_BYTES {
        return Err(audit_failure(
            &provider.provider_id,
            "inventory",
            "none",
            "candidate response is empty or unbounded",
        ));
    }
    expected_log.extend(
        provider
            .candidates
            .iter()
            .map(|candidate| Provider5902Event {
                kind: Task5902EventKind::ProofEnvelope,
                message_id: Some(candidate.message_id.clone()),
                transferred_bytes: candidate.proof_envelope_bytes,
            }),
    );
    let genuine = provider
        .candidates
        .iter()
        .find(|candidate| candidate.message_id == genuine_id)
        .expect("genuine ID comes from candidates");
    expected_log.push(Provider5902Event {
        kind: Task5902EventKind::BodyCiphertext,
        message_id: Some(genuine_id.to_owned()),
        transferred_bytes: genuine.body_fetch_bytes,
    });
    if log.events != expected_log {
        let first_hostile_body = log.events.iter().find(|event| {
            event.kind == Task5902EventKind::BodyCiphertext
                && event.message_id.as_deref() != Some(genuine_id)
                && event.transferred_bytes > 0
        });
        return Err(audit_failure(
            &provider.provider_id,
            first_hostile_body
                .and_then(|event| event.message_id.as_deref())
                .unwrap_or("request_order"),
            if first_hostile_body.is_some() {
                "1"
            } else {
                "none"
            },
            "provider order must be query, five proofs, then genuine body only",
        ));
    }
    let expected_observer = expected_log
        .iter()
        .map(|event| ProcessEntry5902Event {
            kind: event.kind,
            message_id: event.message_id.clone(),
            crossed_bytes: event.transferred_bytes,
        })
        .collect::<Vec<_>>();
    if observer.events != expected_observer {
        let hostile = observer.events.iter().find(|event| {
            event.kind == Task5902EventKind::BodyCiphertext
                && event.message_id.as_deref() != Some(genuine_id)
                && event.crossed_bytes > 0
        });
        return Err(audit_failure(
            &provider.provider_id,
            hostile
                .and_then(|event| event.message_id.as_deref())
                .unwrap_or("process_entry"),
            if hostile.is_some() { "1" } else { "none" },
            "independent process-entry byte stream differs from provider log",
        ));
    }
    if provider.access_count != expected_log.len() as u64 {
        return Err(audit_failure(
            &provider.provider_id,
            "access_count",
            "none",
            "access count must be query + five proof envelopes + genuine body",
        ));
    }
    Ok(())
}

pub fn scenario_name(scenario: SenderScenario) -> &'static str {
    match scenario {
        SenderScenario::GenuineAllowedProof => "genuine_allowed_proof",
        SenderScenario::ForgedFrom => "forged_from",
        SenderScenario::AllowedAlias => "allowed_alias",
        SenderScenario::ForwardingRewrite => "forwarding_rewrite",
        SenderScenario::CompromisedAllowedMailboxWithoutFriendKey => {
            "compromised_allowed_mailbox_without_friend_key"
        }
    }
}
