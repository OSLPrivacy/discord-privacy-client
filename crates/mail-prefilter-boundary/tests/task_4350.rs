use mail_prefilter_boundary::*;
use std::fs;
use std::process::Command;

fn surfaces() -> Vec<SurfaceObservation> {
    REQUIRED_SURFACES
        .into_iter()
        .map(|kind| SurfaceObservation {
            kind,
            observer: format!("independent-{kind:?}-observer"),
            independent: true,
            bounded_scope: "process start through audit completion; exact PID and run nonce".into(),
            observed_full_markers: vec![],
            observed_ids: vec![],
        })
        .collect()
}

fn live_evidence() -> AuditEvidence {
    let inventory = authoritative_inventory();
    let providers = inventory
        .iter()
        .map(|spec| {
            if spec.readiness == Readiness::NotReady {
                return ProviderEvidence {
                    provider_id: spec.id.clone(),
                    mailbox_place: spec.mailbox_place.clone(),
                    readiness: spec.readiness,
                    access_count: 0,
                    refusal: Some(format!(
                        "provider_id={} refusal: no provider-side prefilter; mailbox_place={}",
                        spec.id, spec.mailbox_place
                    )),
                    mailbox: None,
                    provider_request_log: None,
                    post_boundary_surfaces: surfaces(),
                };
            }

            let allowed = (0..5)
                .map(|index| MailMarker {
                    id: format!("allowed-{index}"),
                    full_marker: format!("TASK4350-ALLOWED-FULL-{index}"),
                    sender: format!("friend{index}@example.test"),
                    fresh: true,
                })
                .collect::<Vec<_>>();
            let disallowed = (0..15)
                .map(|index| MailMarker {
                    id: format!("disallowed-{index}"),
                    full_marker: format!("TASK4350-DISALLOWED-FULL-{index}"),
                    sender: format!("stranger{index}@example.test"),
                    fresh: true,
                })
                .collect::<Vec<_>>();
            let allowed_ids = allowed
                .iter()
                .map(|message| message.id.clone())
                .collect::<Vec<_>>();
            let returned_headers = allowed
                .iter()
                .map(|message| CandidateHeader {
                    message_id: message.id.clone(),
                    from: message.sender.clone(),
                    folder: "INBOX".into(),
                })
                .collect::<Vec<_>>();
            let mut request_order = vec![ProviderRequestEvent {
                kind: ProviderRequestKind::CandidateQuery,
                message_id: None,
            }];
            request_order.extend(returned_headers.iter().map(|header| ProviderRequestEvent {
                kind: ProviderRequestKind::Header,
                message_id: Some(header.message_id.clone()),
            }));
            request_order.extend(allowed_ids.iter().map(|message_id| ProviderRequestEvent {
                kind: ProviderRequestKind::Body,
                message_id: Some(message_id.clone()),
            }));
            ProviderEvidence {
                provider_id: spec.id.clone(),
                mailbox_place: spec.mailbox_place.clone(),
                readiness: spec.readiness,
                access_count: 11,
                refusal: None,
                mailbox: Some(FreshMailboxEvidence {
                    real_provider_mailbox: true,
                    run_nonce: "real-run-2026-08-11-unique".into(),
                    allowed,
                    disallowed,
                }),
                provider_request_log: Some(ProviderRequestLog {
                    observer: "provider-owned external request audit log".into(),
                    externally_observed: true,
                    shipping_query: "in:inbox {from:friend0@example.test from:friend1@example.test from:friend2@example.test from:friend3@example.test from:friend4@example.test}".into(),
                    query_returned_ids: allowed_ids.clone(),
                    returned_headers,
                    body_fetch_ids: allowed_ids,
                    request_order,
                }),
                post_boundary_surfaces: surfaces(),
            }
        })
        .collect();
    AuditEvidence {
        schema: EVIDENCE_SCHEMA.into(),
        origin: EvidenceOrigin::RealProviderExternal,
        boundary: BoundaryDefinition {
            name: PROCESS_ENTRY_BOUNDARY.into(),
            defined_before_response_parsing: true,
        },
        authoritative_provider_ids: inventory.into_iter().map(|spec| spec.id).collect(),
        providers,
    }
}

fn assert_red(evidence: &AuditEvidence, provider: &str, needle: &str) {
    let error = audit_live_evidence(evidence).unwrap_err().to_string();
    assert!(
        error.contains(&format!("provider_id={provider}")),
        "{error}"
    );
    assert!(error.contains(PROCESS_ENTRY_BOUNDARY), "{error}");
    assert!(error.contains(needle), "{error}");
}

#[test]
fn exact_live_inventory_passes_with_five_of_twenty_crossing() {
    let summary = audit_live_evidence(&live_evidence()).unwrap();
    assert_eq!(summary.providers, 11);
    assert_eq!(summary.ready_providers, 1);
    assert_eq!(summary.allowed_messages_per_ready, 5);
    assert_eq!(summary.disallowed_messages_per_ready, 15);
    assert_eq!(summary.body_crossings_per_ready, 5);
    assert_eq!(summary.surfaces_per_provider, 6);
    assert_eq!(summary.disallowed_post_boundary_hits, 0);
}

#[test]
fn mutants_late_filter_and_one_disallowed_header_or_body_go_red() {
    let mut late = live_evidence();
    late.providers
        .iter_mut()
        .find(|provider| provider.provider_id == "gmail")
        .unwrap()
        .provider_request_log
        .as_mut()
        .unwrap()
        .shipping_query = "in:inbox".into();
    assert_red(&late, "gmail", "shipping query");

    let mut header = live_evidence();
    let gmail = header
        .providers
        .iter_mut()
        .find(|provider| provider.provider_id == "gmail")
        .unwrap();
    gmail
        .provider_request_log
        .as_mut()
        .unwrap()
        .returned_headers[0]
        .message_id = "disallowed-0".into();
    assert_red(&header, "gmail", "header");

    let mut body = live_evidence();
    let gmail = body
        .providers
        .iter_mut()
        .find(|provider| provider.provider_id == "gmail")
        .unwrap();
    gmail.provider_request_log.as_mut().unwrap().body_fetch_ids[0] = "disallowed-0".into();
    assert_red(&body, "gmail", "body crossings");

    let mut body_before_headers = live_evidence();
    let gmail = body_before_headers
        .providers
        .iter_mut()
        .find(|provider| provider.provider_id == "gmail")
        .unwrap();
    gmail
        .provider_request_log
        .as_mut()
        .unwrap()
        .request_order
        .swap(1, 6);
    assert_red(&body_before_headers, "gmail", "request order");
}

#[test]
fn mutants_omitted_provider_surface_and_empty_inventory_go_red() {
    let mut omitted_provider = live_evidence();
    omitted_provider
        .providers
        .retain(|provider| provider.provider_id != "gmail");
    assert_red(&omitted_provider, "gmail", "provider evidence");

    let mut omitted_surface = live_evidence();
    omitted_surface
        .providers
        .iter_mut()
        .find(|provider| provider.provider_id == "gmail")
        .unwrap()
        .post_boundary_surfaces
        .retain(|surface| surface.kind != SurfaceKind::Queues);
    assert_red(&omitted_surface, "gmail", "surface inventory");

    let mut empty = live_evidence();
    empty.authoritative_provider_ids.clear();
    assert_red(&empty, "gmail", "provider inventory");
}

#[test]
fn mutants_fixture_and_post_ingest_deletion_claim_go_red() {
    let mut fixture = live_evidence();
    fixture.origin = EvidenceOrigin::Fixture;
    assert_red(&fixture, "inventory", "fixtures");

    let mut deletion_claim = live_evidence();
    let gmail = deletion_claim
        .providers
        .iter_mut()
        .find(|provider| provider.provider_id == "gmail")
        .unwrap();
    gmail.post_boundary_surfaces[0]
        .observed_full_markers
        .push("TASK4350-DISALLOWED-FULL-0".into());
    assert_red(&deletion_claim, "gmail", "contains disallowed");
}

#[test]
fn not_ready_access_and_refusal_without_mailbox_place_go_red() {
    let mut accessed = live_evidence();
    accessed
        .providers
        .iter_mut()
        .find(|provider| provider.provider_id == "tuta")
        .unwrap()
        .access_count = 1;
    assert_red(&accessed, "tuta", "NotReady provider accessed");

    let mut vague = live_evidence();
    vague
        .providers
        .iter_mut()
        .find(|provider| provider.provider_id == "outlook-web")
        .unwrap()
        .refusal = Some("outlook-web unavailable".into());
    assert_red(&vague, "outlook-web", "mailbox place");
}

#[test]
fn acceptance_binary_reports_counts_and_names_provider_and_boundary_on_failure() {
    let directory = std::env::temp_dir().join(format!("task4350-{}", std::process::id()));
    fs::create_dir_all(&directory).unwrap();
    let green_path = directory.join("green.json");
    fs::write(
        &green_path,
        serde_json::to_vec_pretty(&live_evidence()).unwrap(),
    )
    .unwrap();
    let green = Command::new(env!("CARGO_BIN_EXE_audit-4350"))
        .arg(&green_path)
        .output()
        .unwrap();
    assert!(
        green.status.success(),
        "{}",
        String::from_utf8_lossy(&green.stderr)
    );
    let stdout = String::from_utf8_lossy(&green.stdout);
    assert!(stdout.contains("providers=11"));
    assert!(stdout.contains("allowed=5 disallowed=15 body_crossings=5 surfaces=6"));

    let mut red_evidence = live_evidence();
    let gmail = red_evidence
        .providers
        .iter_mut()
        .find(|provider| provider.provider_id == "gmail")
        .unwrap();
    gmail.provider_request_log.as_mut().unwrap().body_fetch_ids[0] = "disallowed-0".into();
    let red_path = directory.join("red.json");
    fs::write(&red_path, serde_json::to_vec_pretty(&red_evidence).unwrap()).unwrap();
    let red = Command::new(env!("CARGO_BIN_EXE_audit-4350"))
        .arg(&red_path)
        .output()
        .unwrap();
    assert_eq!(red.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&red.stderr);
    assert!(stderr.contains("provider_id=gmail"), "{stderr}");
    assert!(stderr.contains(PROCESS_ENTRY_BOUNDARY), "{stderr}");

    fs::remove_dir_all(directory).unwrap();
}
