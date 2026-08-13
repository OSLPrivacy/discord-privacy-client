use mail_prefilter_boundary::task_5902_audit::*;
use mail_prefilter_boundary::{
    authoritative_inventory, EvidenceOrigin, Readiness, PROCESS_ENTRY_BOUNDARY,
};
use std::fs;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

const PROVIDER: &str = "gmail";
const QUERY_BYTES: u64 = 311;
const PROOF_BYTES: u64 = 704;
const GENUINE_BODY_BYTES: u64 = 2_048;
static RED_CASE: AtomicUsize = AtomicUsize::new(0);

fn all_checks(signature_valid: bool) -> ProofCheckEvidence {
    ProofCheckEvidence {
        established_friend_key_checked: true,
        signature_checked: true,
        signature_valid,
        provider_id_bound: true,
        provider_message_id_bound: true,
        recipient_bound: true,
        conversation_bound: true,
        body_ciphertext_digest_bound: true,
        proof_verified_before_body_fetch: true,
    }
}

fn message_id(scenario: SenderScenario) -> String {
    format!("gmail-{}", scenario_name(scenario).replace('_', "-"))
}

fn candidate(scenario: SenderScenario) -> ProofCandidateEvidence {
    let id = message_id(scenario);
    let genuine = scenario == SenderScenario::GenuineAllowedProof;
    ProofCandidateEvidence {
        message_id: id.clone(),
        scenario,
        // Every hostile message deliberately passes the provider selector. The
        // selector is only a bandwidth optimization, never authentication.
        provider_sender_and_folder_match: true,
        proof_envelope_bytes: PROOF_BYTES,
        checks: all_checks(genuine),
        authenticated: genuine,
        body_fetch_bytes: if genuine { GENUINE_BODY_BYTES } else { 0 },
        process_entry_body_bytes: if genuine { GENUINE_BODY_BYTES } else { 0 },
        refusal: (!genuine).then(|| {
            format!("provider_id={PROVIDER} message_id={id} failure={FAILED_SENDER_PROOF}")
        }),
        confidential_content_canary: format!("SECRET-BODY-CANARY-{id}"),
    }
}

fn ready_provider(mailbox_place: String) -> Provider5902Evidence {
    let candidates = REQUIRED_SCENARIOS
        .into_iter()
        .map(candidate)
        .collect::<Vec<_>>();
    let mut provider_events = vec![Provider5902Event {
        kind: Task5902EventKind::CandidateQuery,
        message_id: None,
        transferred_bytes: QUERY_BYTES,
    }];
    provider_events.extend(candidates.iter().map(|candidate| Provider5902Event {
        kind: Task5902EventKind::ProofEnvelope,
        message_id: Some(candidate.message_id.clone()),
        transferred_bytes: candidate.proof_envelope_bytes,
    }));
    let genuine = candidates
        .iter()
        .find(|candidate| candidate.scenario == SenderScenario::GenuineAllowedProof)
        .unwrap();
    provider_events.push(Provider5902Event {
        kind: Task5902EventKind::BodyCiphertext,
        message_id: Some(genuine.message_id.clone()),
        transferred_bytes: genuine.body_fetch_bytes,
    });
    let process_events = provider_events
        .iter()
        .map(|event| ProcessEntry5902Event {
            kind: event.kind,
            message_id: event.message_id.clone(),
            crossed_bytes: event.transferred_bytes,
        })
        .collect();

    Provider5902Evidence {
        provider_id: PROVIDER.into(),
        mailbox_place,
        readiness: Readiness::Ready,
        real_provider_mailbox: true,
        metadata_prebody_supported: true,
        access_count: provider_events.len() as u64,
        not_ready_refusal: None,
        provider_log: Some(Provider5902RequestLog {
            observer: "provider-owned real Gmail request log / run 5902".into(),
            externally_observed: true,
            events: provider_events,
        }),
        process_entry_observer: Some(ProcessEntry5902Observer {
            observer: "independent PID-scoped process-entry byte observer / run 5902".into(),
            independent: true,
            boundary: PROCESS_ENTRY_BOUNDARY.into(),
            events: process_events,
        }),
        candidates,
    }
}

fn not_ready_provider(
    provider_id: String,
    mailbox_place: String,
    readiness: Readiness,
) -> Provider5902Evidence {
    Provider5902Evidence {
        not_ready_refusal: Some(format!(
            "provider_id={provider_id} mailbox_place={mailbox_place} NotReady: cannot expose authenticated pre-body sender-proof metadata"
        )),
        provider_id,
        mailbox_place,
        readiness,
        real_provider_mailbox: false,
        metadata_prebody_supported: false,
        access_count: 0,
        provider_log: None,
        process_entry_observer: None,
        candidates: vec![],
    }
}

fn green_evidence() -> Task5902Evidence {
    let inventory = authoritative_inventory();
    let providers = inventory
        .iter()
        .map(|spec| match spec.readiness {
            Readiness::Ready => ready_provider(spec.mailbox_place.clone()),
            Readiness::NotReady => {
                not_ready_provider(spec.id.clone(), spec.mailbox_place.clone(), spec.readiness)
            }
        })
        .collect();
    Task5902Evidence {
        schema: TASK_5902_SCHEMA.into(),
        origin: EvidenceOrigin::RealProviderExternal,
        authoritative_provider_ids: inventory.into_iter().map(|spec| spec.id).collect(),
        providers,
    }
}

fn gmail(evidence: &Task5902Evidence) -> &Provider5902Evidence {
    evidence
        .providers
        .iter()
        .find(|provider| provider.provider_id == PROVIDER)
        .unwrap()
}

fn gmail_mut(evidence: &mut Task5902Evidence) -> &mut Provider5902Evidence {
    evidence
        .providers
        .iter_mut()
        .find(|provider| provider.provider_id == PROVIDER)
        .unwrap()
}

fn candidate_mut(
    evidence: &mut Task5902Evidence,
    scenario: SenderScenario,
) -> &mut ProofCandidateEvidence {
    gmail_mut(evidence)
        .candidates
        .iter_mut()
        .find(|candidate| candidate.scenario == scenario)
        .unwrap()
}

fn assert_red(
    evidence: &Task5902Evidence,
    provider_id: &str,
    hostile_id: &str,
    first_crossed_byte: &str,
) -> String {
    assert!(audit_task_5902(evidence).is_err());
    let case = RED_CASE.fetch_add(1, Ordering::Relaxed);
    let directory =
        std::env::temp_dir().join(format!("task5902-red-{}-{case}", std::process::id()));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir_all(&directory).unwrap();
    let path = directory.join("evidence.json");
    fs::write(&path, serde_json::to_vec_pretty(evidence).unwrap()).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_audit-5902"))
        .arg(path)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let rendered = String::from_utf8(output.stderr).unwrap();
    fs::remove_dir_all(directory).unwrap();
    assert!(
        rendered.contains(&format!("provider_id={provider_id}")),
        "{rendered}"
    );
    assert!(
        rendered.contains(&format!("hostile_id={hostile_id}")),
        "{rendered}"
    );
    assert!(
        rendered.contains(&format!("first_crossed_byte={first_crossed_byte}")),
        "{rendered}"
    );
    assert!(rendered.contains(PROCESS_ENTRY_BOUNDARY), "{rendered}");
    rendered
}

#[test]
fn real_provider_matrix_fetches_one_genuine_body_and_zero_hostile_bytes() {
    let evidence = green_evidence();
    let summary = audit_task_5902(&evidence).unwrap();
    assert_eq!(summary.providers, 11);
    assert_eq!(summary.candidate_ready, 1);
    assert_eq!(summary.genuine_bodies, 1);
    assert_eq!(summary.hostile_attacks, 4);
    assert_eq!(summary.hostile_body_fetch_bytes, 0);
    assert_eq!(summary.hostile_process_entry_body_bytes, 0);
    assert_eq!(summary.refusals, 4);

    let ready = gmail(&evidence);
    assert_eq!(ready.access_count, 7);
    assert_eq!(ready.provider_log.as_ref().unwrap().events.len(), 7);
    assert_eq!(
        ready.process_entry_observer.as_ref().unwrap().events.len(),
        7
    );
    for hostile in ready
        .candidates
        .iter()
        .filter(|candidate| candidate.scenario != SenderScenario::GenuineAllowedProof)
    {
        assert_eq!(hostile.body_fetch_bytes, 0, "{}", hostile.message_id);
        assert_eq!(
            hostile.process_entry_body_bytes, 0,
            "{}",
            hostile.message_id
        );
        let refusal = hostile.refusal.as_deref().unwrap();
        assert!(refusal.contains(FAILED_SENDER_PROOF));
        assert!(!refusal.contains(&hostile.confidential_content_canary));
    }
    println!(
        "TASK5902 providers=11 candidate_ready=1 genuine_bodies=1 hostile_attacks=4 hostile_body_fetch_bytes=0 hostile_process_entry_body_bytes=0 refusals=4 provider_events=7 process_entry_events=7"
    );
}

#[test]
fn starving_each_required_proof_binding_goes_red_before_a_body_crosses() {
    let cases: [(&str, fn(&mut ProofCheckEvidence)); 3] = [
        ("signature", |checks| checks.signature_checked = false),
        ("digest", |checks| {
            checks.body_ciphertext_digest_bound = false
        }),
        ("conversation", |checks| checks.conversation_bound = false),
    ];
    for (name, starve) in cases {
        let mut evidence = green_evidence();
        let hostile = candidate_mut(&mut evidence, SenderScenario::ForgedFrom);
        let hostile_id = hostile.message_id.clone();
        starve(&mut hostile.checks);
        let rendered = assert_red(&evidence, PROVIDER, &hostile_id, "none");
        assert!(rendered.contains("sender proof starved"), "{rendered}");
        println!(
            "TASK5902_RED mutation=starve_{name} provider={PROVIDER} hostile_id={hostile_id} first_crossed_byte=none exit=1"
        );
    }
}

#[test]
fn fetching_before_verification_and_trusting_header_variants_go_red() {
    let mut early = green_evidence();
    let early_hostile = candidate_mut(&mut early, SenderScenario::ForwardingRewrite);
    let early_id = early_hostile.message_id.clone();
    early_hostile.checks.proof_verified_before_body_fetch = false;
    early_hostile.body_fetch_bytes = 1;
    early_hostile.process_entry_body_bytes = 1;
    let rendered = assert_red(&early, PROVIDER, &early_id, "1");
    assert!(rendered.contains("ran after body fetch"), "{rendered}");
    println!(
        "TASK5902_RED mutation=fetch_before_proof provider={PROVIDER} hostile_id={early_id} first_crossed_byte=1 exit=1"
    );

    for scenario in [
        SenderScenario::ForgedFrom,
        SenderScenario::AllowedAlias,
        SenderScenario::ForwardingRewrite,
        SenderScenario::CompromisedAllowedMailboxWithoutFriendKey,
    ] {
        let mut trusted = green_evidence();
        let hostile = candidate_mut(&mut trusted, scenario);
        let hostile_id = hostile.message_id.clone();
        hostile.authenticated = true;
        let rendered = assert_red(&trusted, PROVIDER, &hostile_id, "none");
        assert!(rendered.contains("was authenticated"), "{rendered}");
        println!(
            "TASK5902_RED mutation=trust_{} provider={PROVIDER} hostile_id={hostile_id} first_crossed_byte=none exit=1",
            scenario_name(scenario)
        );
    }
}

#[test]
fn every_omitted_provider_and_scenario_goes_red_with_identity() {
    for provider_id in authoritative_inventory()
        .into_iter()
        .map(|provider| provider.id)
    {
        let mut evidence = green_evidence();
        evidence
            .providers
            .retain(|provider| provider.provider_id != provider_id);
        let rendered = assert_red(&evidence, &provider_id, "inventory", "none");
        assert!(rendered.contains("provider evidence"), "{rendered}");
        println!(
            "TASK5902_RED mutation=omit_provider provider={provider_id} hostile_id=inventory first_crossed_byte=none exit=1"
        );
    }

    for scenario in REQUIRED_SCENARIOS {
        let mut evidence = green_evidence();
        gmail_mut(&mut evidence)
            .candidates
            .retain(|candidate| candidate.scenario != scenario);
        let hostile_id = scenario_name(scenario);
        let rendered = assert_red(&evidence, PROVIDER, hostile_id, "none");
        assert!(rendered.contains("scenario is omitted"), "{rendered}");
        println!(
            "TASK5902_RED mutation=omit_scenario provider={PROVIDER} hostile_id={hostile_id} first_crossed_byte=none exit=1"
        );
    }
}

#[test]
fn empty_authoritative_and_evidence_inventories_go_red() {
    let mut fixture = green_evidence();
    fixture.origin = EvidenceOrigin::Fixture;
    let rendered = assert_red(&fixture, PROVIDER, "inventory", "none");
    assert!(
        rendered.contains("simulated provider evidence"),
        "{rendered}"
    );

    let mut authoritative = green_evidence();
    authoritative.authoritative_provider_ids.clear();
    let rendered = assert_red(&authoritative, "aol", "inventory", "none");
    assert!(
        rendered.contains("provider inventory is empty"),
        "{rendered}"
    );

    let mut providers = green_evidence();
    providers.providers.clear();
    let rendered = assert_red(&providers, "aol", "inventory", "none");
    assert!(
        rendered.contains("provider evidence is empty"),
        "{rendered}"
    );
    println!(
        "TASK5902_RED non_real_origin=1 empty_inventories=2 provider=aol hostile_id=inventory first_crossed_byte=none exit=1"
    );
}

#[test]
fn audit_5902_binary_reports_exact_counts_and_named_first_byte_failure() {
    let directory =
        std::env::temp_dir().join(format!("task5902-acceptance-{}", std::process::id()));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir_all(&directory).unwrap();

    let green_path = directory.join("green.json");
    fs::write(
        &green_path,
        serde_json::to_vec_pretty(&green_evidence()).unwrap(),
    )
    .unwrap();
    let green = Command::new(env!("CARGO_BIN_EXE_audit-5902"))
        .arg(&green_path)
        .output()
        .unwrap();
    let green_stdout = String::from_utf8_lossy(&green.stdout);
    assert!(
        green.status.success(),
        "{}",
        String::from_utf8_lossy(&green.stderr)
    );
    assert_eq!(
        green_stdout.trim(),
        "TASK5902_PASS providers=11 candidate_ready=1 genuine_bodies=1 hostile_attacks=4 hostile_body_fetch_bytes=0 hostile_process_entry_body_bytes=0 refusals=4 proof_bindings=signature+digest+conversation+provider_message_id+recipient"
    );
    println!("{}", green_stdout.trim());

    let mut red_evidence = green_evidence();
    let hostile = candidate_mut(
        &mut red_evidence,
        SenderScenario::CompromisedAllowedMailboxWithoutFriendKey,
    );
    let hostile_id = hostile.message_id.clone();
    hostile.body_fetch_bytes = 1;
    hostile.process_entry_body_bytes = 1;
    let red_path = directory.join("red.json");
    fs::write(&red_path, serde_json::to_vec_pretty(&red_evidence).unwrap()).unwrap();
    let red = Command::new(env!("CARGO_BIN_EXE_audit-5902"))
        .arg(&red_path)
        .output()
        .unwrap();
    assert_eq!(red.status.code(), Some(1));
    let red_stderr = String::from_utf8_lossy(&red.stderr);
    assert!(red_stderr.contains("provider_id=gmail"), "{red_stderr}");
    assert!(
        red_stderr.contains(&format!("hostile_id={hostile_id}")),
        "{red_stderr}"
    );
    assert!(red_stderr.contains("first_crossed_byte=1"), "{red_stderr}");
    println!(
        "TASK5902_RED binary_exit=1 provider={PROVIDER} hostile_id={hostile_id} first_crossed_byte=1"
    );

    fs::remove_dir_all(directory).unwrap();
}
