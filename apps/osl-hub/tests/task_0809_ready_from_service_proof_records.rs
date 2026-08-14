use osl_privacy_hub::models::ServiceKind;
use osl_privacy_hub::services::{
    ready_decisions_from_service_proof_records, ProtectedDeliveryProof, ServiceReadyLabel,
};

fn valid_discord_two_person_proof() -> ProtectedDeliveryProof {
    ProtectedDeliveryProof {
        service_id: ServiceKind::Discord,
        protected_message_id: "protected-msg-0809".to_owned(),
        sender_person_id: "person-alice-0809".to_owned(),
        recipient_person_id: "person-bob-0809".to_owned(),
        protected_message_received: true,
        received_by_real_other_person: true,
    }
}

#[test]
fn task_0809_proof_records_cannot_invent_missing_two_person_capability() {
    let before = ready_decisions_from_service_proof_records(&[]);
    let before_ready_count = before
        .iter()
        .filter(|decision| decision.label == Some(ServiceReadyLabel::Ready))
        .count();
    println!("TASK0809_BEFORE_READY_COUNT={before_ready_count}");
    assert_eq!(before_ready_count, 0);

    let mut single_person = valid_discord_two_person_proof();
    single_person.recipient_person_id = single_person.sender_person_id.clone();
    let single_person_ready_count = ready_decisions_from_service_proof_records(&[single_person])
        .iter()
        .filter(|decision| decision.label == Some(ServiceReadyLabel::Ready))
        .count();
    println!("TASK0809_SINGLE_PERSON_PROOF_READY_COUNT={single_person_ready_count}");
    assert_eq!(single_person_ready_count, 0);

    let proof = valid_discord_two_person_proof();
    let after = ready_decisions_from_service_proof_records(&[proof.clone()]);
    let ready: Vec<_> = after
        .iter()
        .filter(|decision| decision.label == Some(ServiceReadyLabel::Ready))
        .collect();
    println!("TASK0809_AFTER_READY_COUNT={}", ready.len());
    assert_eq!(ready.len(), 0);
    let discord = after
        .iter()
        .find(|decision| decision.service_id == ServiceKind::Discord)
        .expect("Discord has an installed service decision");
    assert_eq!(
        discord.refusal.as_deref(),
        Some("ready_requires_real_two_person_protected_messaging_capability")
    );
    println!(
        "TASK0809_VALID_TWO_PERSON_PROOF_WITHOUT_CAPABILITY service={:?} protected_message_received={} received_by_real_other_person={} sender_person_id={} recipient_person_id={} refusal={}",
        proof.service_id,
        proof.protected_message_received,
        proof.received_by_real_other_person,
        proof.sender_person_id,
        proof.recipient_person_id,
        discord.refusal.as_deref().unwrap()
    );
}
