use osl_privacy_hub::models::ServiceKind;
use osl_privacy_hub::services::{generated_tile_label, ServiceCapabilityFacts};

#[test]
fn task_0804_direct_label_generation_returns_all_five_tile_labels() {
    let cases = [
        ("ready", facts(true, true, true, true), "Ready"),
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
        let actual = generated_tile_label(input);
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
