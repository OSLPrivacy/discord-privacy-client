use std::process::{Command, Stdio};

use osl_privacy_hub::models::ServiceKind;
use osl_privacy_hub::services::{
    direct_service_ready_label_for_facts, generated_tile_label, installed_service_capability_facts,
    ready_decisions_from_service_proof_records, ProtectedDeliveryProof, ServiceCapabilityFacts,
    ServiceReadyLabel, READY_REQUIRES_MATCHING_DELIVERY_PROOF,
    READY_REQUIRES_REAL_TWO_PERSON_CAPABILITY,
};

fn ready_capable_facts() -> ServiceCapabilityFacts {
    ServiceCapabilityFacts {
        service_id: ServiceKind::Discord,
        placing: true,
        reading: true,
        opening: true,
        real_two_person_protected_messaging: true,
    }
}

fn matching_delivery_proof() -> ProtectedDeliveryProof {
    ProtectedDeliveryProof {
        service_id: ServiceKind::Discord,
        protected_message_id: "protected-msg-5087".to_owned(),
        sender_person_id: "person-alice-5087".to_owned(),
        recipient_person_id: "person-bob-5087".to_owned(),
        protected_message_received: true,
        received_by_real_other_person: true,
    }
}

fn check_surface_label(
    forced_label: &str,
    facts: ServiceCapabilityFacts,
    proof: Option<&ProtectedDeliveryProof>,
) -> Result<(), String> {
    let generated = generated_tile_label(facts, proof);
    if forced_label == generated {
        Ok(())
    } else {
        Err(format!(
            "forced carrier label {forced_label:?} disagrees with proof-gated label {generated:?}"
        ))
    }
}

#[test]
fn task_5087_forced_ready_check_entrypoint() {
    if std::env::var_os("OSL_TASK_5087_CHILD_CHECK").is_none() {
        return;
    }

    match check_surface_label("Ready", ready_capable_facts(), None) {
        Ok(()) => std::process::exit(0),
        Err(error) => {
            eprintln!("TASK5087_FORCED_READY_ERROR={error}");
            std::process::exit(1);
        }
    }
}

#[test]
fn task_5087_ready_requires_capability_and_matching_delivery_proof() {
    let installed = installed_service_capability_facts();
    let installed_ready_without_proof = installed
        .iter()
        .filter(|facts| generated_tile_label(**facts, None) == "Ready")
        .count();
    println!("TASK5087_INSTALLED_READY_WITHOUT_PROOF={installed_ready_without_proof}");
    assert_eq!(installed_ready_without_proof, 0);

    let placing_and_reading = installed
        .iter()
        .filter(|facts| facts.placing && facts.reading)
        .collect::<Vec<_>>();
    assert!(!placing_and_reading.is_empty());
    for facts in &placing_and_reading {
        assert_eq!(generated_tile_label(**facts, None), "Placing and reading");
    }
    println!(
        "TASK5087_PLACE_AND_READ_WITHOUT_DELIVERY_COUNT={} label=Placing and reading",
        placing_and_reading.len()
    );

    let facts = ready_capable_facts();
    let proof = matching_delivery_proof();
    let no_proof = direct_service_ready_label_for_facts(facts, None)
        .expect_err("capability without proof must not become Ready");
    assert_eq!(no_proof, READY_REQUIRES_MATCHING_DELIVERY_PROOF);

    let mut wrong_service = proof.clone();
    wrong_service.service_id = ServiceKind::Telegram;
    let wrong_service_refusal = direct_service_ready_label_for_facts(facts, Some(&wrong_service))
        .expect_err("a proof for another service must not become Ready");
    assert_eq!(
        wrong_service_refusal,
        READY_REQUIRES_MATCHING_DELIVERY_PROOF
    );

    let mut missing_capability = facts;
    missing_capability.real_two_person_protected_messaging = false;
    let missing_capability_refusal =
        direct_service_ready_label_for_facts(missing_capability, Some(&proof))
            .expect_err("proof without real two-person capability must not become Ready");
    assert_eq!(
        missing_capability_refusal,
        READY_REQUIRES_REAL_TWO_PERSON_CAPABILITY
    );

    let ready = direct_service_ready_label_for_facts(facts, Some(&proof))
        .expect("real two-person capability with a matching proof may become Ready");
    assert_eq!(ready, ServiceReadyLabel::Ready);
    assert_eq!(generated_tile_label(facts, Some(&proof)), "Ready");
    println!(
        "TASK5087_MATCHING_PROOF_READY_COUNT=1 service=discord sender={} recipient={} label=Ready",
        proof.sender_person_id, proof.recipient_person_id
    );

    let records_ready = ready_decisions_from_service_proof_records(&[proof])
        .iter()
        .filter(|decision| decision.label == Some(ServiceReadyLabel::Ready))
        .count();
    println!("TASK5087_INSTALLED_RECORD_DECISION_READY_COUNT={records_ready}");
    assert_eq!(
        records_ready, 0,
        "a delivery record must not invent missing two-person capability"
    );
}

#[test]
fn task_5087_forcing_ready_without_proof_fails_the_surface_check() {
    let current_exe = std::env::current_exe().expect("test binary path is available");
    let output = Command::new(current_exe)
        .env("OSL_TASK_5087_CHILD_CHECK", "1")
        .arg("--exact")
        .arg("task_5087_forced_ready_check_entrypoint")
        .arg("--nocapture")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("forced Ready child check runs");
    let exit = output.status.code().unwrap_or(-1);
    let stderr = String::from_utf8_lossy(&output.stderr);
    println!("TASK5087_FORCED_READY_CHECK_EXIT={exit}");
    eprintln!("{stderr}");
    assert_eq!(exit, 1);
    assert!(stderr.contains("forced carrier label \"Ready\""));
    assert!(stderr.contains("proof-gated label \"Placing and reading\""));
}
