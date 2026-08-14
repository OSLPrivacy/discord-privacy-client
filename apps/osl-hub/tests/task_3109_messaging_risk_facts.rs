use osl_privacy_hub::services::{
    all_messaging_risk_facts, messaging_risk_facts, MESSAGING_RISK_FACTS,
};

#[test]
fn task_3109_direct_command_returns_risk_facts_for_typed_messaging_services() {
    let expected_services = [
        ("discord", "Discord"),
        ("telegram", "Telegram"),
        ("signal", "Signal"),
        ("whatsapp", "WhatsApp"),
        ("x", "X"),
        ("instagram", "Instagram"),
        ("messenger", "Messenger"),
        ("email", "email"),
    ];
    let all_facts = all_messaging_risk_facts();

    println!("TASK3109_DIRECT_COMMAND=messaging_risk_facts");
    println!("TASK3109_RISK_SERVICE_ROW_COUNT={}", all_facts.len());
    assert_eq!(
        all_facts.len(),
        expected_services.len(),
        "risk facts must cover exactly the required messaging services"
    );

    for (service_id, display_name) in expected_services {
        let facts = messaging_risk_facts(service_id)
            .expect("known messaging service must return risk facts");
        assert_eq!(facts.service_id, service_id);
        assert_eq!(facts.display_name, display_name);
        assert_eq!(facts.facts, MESSAGING_RISK_FACTS);

        println!("TASK3109_SERVICE={display_name}");
        for fact in facts.facts {
            println!("TASK3109_RISK_FACT service={display_name} fact={fact}");
        }
    }

    let refused = messaging_risk_facts("unknown-service").unwrap_err();
    println!("TASK3109_UNKNOWN_SERVICE_REFUSAL={refused}");
    assert_eq!(refused, "unknown messaging service");
}
