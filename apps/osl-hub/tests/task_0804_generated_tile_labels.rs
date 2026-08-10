use osl_privacy_hub::models::ServiceKind;
use osl_privacy_hub::services::{
    generated_tile_label, ProtectedDeliveryProof, ServiceCapabilityFacts,
};

#[test]
fn task_0804_direct_label_generation_returns_all_six_tile_labels() {
    let ready_facts = facts(true, true, true, true);
    let proof = matching_proof();
    assert_eq!(generated_tile_label(ready_facts, Some(&proof)), "Ready");

    let cases = [
        (
            "placing_and_reading_without_proof",
            facts(true, true, true, true),
            "Placing and reading",
        ),
        (
            "placing_only",
            facts(true, false, true, false),
            "Placing only",
        ),
        (
            "reading_only",
            facts(false, true, true, false),
            "Reading only",
        ),
        (
            "opens_the_app",
            facts(false, false, true, false),
            "Opens the app",
        ),
        (
            "not_started",
            facts(false, false, false, false),
            "Not started",
        ),
    ];

    for (case, input, expected) in cases {
        let actual = generated_tile_label(input, None);
        println!(
            "TASK0804_GENERATED_TILE_LABEL case={case} placing={} reading={} opening={} real_two_person_protected_messaging={} label={actual}",
            input.placing,
            input.reading,
            input.opening,
            input.real_two_person_protected_messaging
        );
        assert_eq!(actual, expected, "{case} generated the wrong tile label");
    }
}

fn matching_proof() -> ProtectedDeliveryProof {
    ProtectedDeliveryProof {
        service_id: ServiceKind::Discord,
        protected_message_id: "protected-msg-0804".to_owned(),
        sender_person_id: "person-alice-0804".to_owned(),
        recipient_person_id: "person-bob-0804".to_owned(),
        protected_message_received: true,
        received_by_real_other_person: true,
    }
}

fn facts(
    placing: bool,
    reading: bool,
    opening: bool,
    real_two_person_protected_messaging: bool,
) -> ServiceCapabilityFacts {
    ServiceCapabilityFacts {
        service_id: ServiceKind::Discord,
        placing,
        reading,
        opening,
        real_two_person_protected_messaging,
    }
}
