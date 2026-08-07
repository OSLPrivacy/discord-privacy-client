use osl_privacy_hub::models::ServiceKind;
use osl_privacy_hub::services::{
    direct_service_ready_label, direct_service_ready_label_for_facts,
    installed_service_capability_facts, installed_service_count, ProtectedDeliveryProof,
    ServiceCapabilityFacts, ServiceReadyLabel, READY_REQUIRES_MATCHING_DELIVERY_PROOF,
    READY_REQUIRES_REAL_TWO_PERSON_CAPABILITY,
};

#[test]
fn task_0808_direct_label_generation_refuses_ready_without_matching_delivery_proof() {
    let installed_count = installed_service_count();
    let facts = installed_service_capability_facts();
    println!("TASK0808_INSTALLED_SERVICE_COUNT={installed_count}");
    println!("TASK0808_CAPABILITY_ROW_COUNT={}", facts.len());
    assert_eq!(facts.len(), installed_count);
    assert_ne!(installed_count, 0);

    let installed_ready_count = facts
        .iter()
        .filter(|fact| fact.real_two_person_protected_messaging)
        .count();
    println!("TASK0808_INSTALLED_READY_CAPABILITY_COUNT={installed_ready_count}");
    assert_eq!(
        installed_ready_count, 0,
        "no installed service may claim Ready until a real two-person protected-message proof exists"
    );

    let discord_without_proof = direct_service_ready_label("discord", None)
        .expect_err("Discord must refuse Ready without a delivery proof");
    println!("TASK0808_DIRECT_LABEL service=discord label=REFUSED refusal={discord_without_proof}");
    assert_eq!(
        discord_without_proof,
        READY_REQUIRES_REAL_TWO_PERSON_CAPABILITY
    );

    let ready_capable_facts = ServiceCapabilityFacts {
        service_id: ServiceKind::Discord,
        placing: true,
        reading: true,
        opening: true,
        real_two_person_protected_messaging: true,
    };
    let matching_proof = ProtectedDeliveryProof {
        service_id: ServiceKind::Discord,
        protected_message_id: "protected-msg-0808".to_owned(),
        sender_person_id: "person-alice".to_owned(),
        recipient_person_id: "person-bob".to_owned(),
        protected_message_received: true,
        received_by_real_other_person: true,
    };

    let no_proof = direct_service_ready_label_for_facts(ready_capable_facts, None)
        .expect_err("Ready must require a delivery proof");
    println!("TASK0808_READY_REFUSAL missing_delivery_proof={no_proof}");
    assert_eq!(no_proof, READY_REQUIRES_MATCHING_DELIVERY_PROOF);

    let mut wrong_service = matching_proof.clone();
    wrong_service.service_id = ServiceKind::Telegram;
    let wrong_service_refusal =
        direct_service_ready_label_for_facts(ready_capable_facts, Some(&wrong_service))
            .expect_err("Ready must require a proof for the same service");
    println!("TASK0808_READY_REFUSAL wrong_service={wrong_service_refusal}");
    assert_eq!(
        wrong_service_refusal,
        READY_REQUIRES_MATCHING_DELIVERY_PROOF
    );

    let mut same_person = matching_proof.clone();
    same_person.recipient_person_id = same_person.sender_person_id.clone();
    let same_person_refusal =
        direct_service_ready_label_for_facts(ready_capable_facts, Some(&same_person))
            .expect_err("Ready must require a real other recipient");
    println!("TASK0808_READY_REFUSAL same_person={same_person_refusal}");
    assert_eq!(same_person_refusal, READY_REQUIRES_MATCHING_DELIVERY_PROOF);

    let mut not_received = matching_proof.clone();
    not_received.protected_message_received = false;
    let not_received_refusal =
        direct_service_ready_label_for_facts(ready_capable_facts, Some(&not_received))
            .expect_err("Ready must require a received protected message");
    println!("TASK0808_READY_REFUSAL protected_message_not_received={not_received_refusal}");
    assert_eq!(not_received_refusal, READY_REQUIRES_MATCHING_DELIVERY_PROOF);

    let ready = direct_service_ready_label_for_facts(ready_capable_facts, Some(&matching_proof))
        .expect("matching proof may generate Ready");
    println!(
        "TASK0808_MATCHING_DELIVERY_PROOF service={:?} protected_message_received={} received_by_real_other_person={} sender_person_id={} recipient_person_id={} label={:?}",
        matching_proof.service_id,
        matching_proof.protected_message_received,
        matching_proof.received_by_real_other_person,
        matching_proof.sender_person_id,
        matching_proof.recipient_person_id,
        ready
    );
    assert_eq!(ready, ServiceReadyLabel::Ready);
}
