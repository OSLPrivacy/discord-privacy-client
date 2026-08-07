use std::collections::HashSet;

use osl_privacy_hub::models::ServiceKind;
use osl_privacy_hub::services::{
    installed_service_capability_facts, installed_service_count, service_capability_facts,
};

#[test]
fn task_0800_direct_reads_return_one_capability_record_per_installed_service() {
    let installed_count = installed_service_count();
    let facts = installed_service_capability_facts();
    let row_count = facts.len();

    println!("TASK0800_INSTALLED_SERVICE_COUNT={installed_count}");
    println!("TASK0800_CAPABILITY_ROW_COUNT={row_count}");

    assert_ne!(
        installed_count, 0,
        "installed-service count must be non-zero"
    );
    assert_eq!(
        row_count, installed_count,
        "capability row count must equal installed-service count"
    );

    let mut seen = HashSet::new();
    for fact in facts {
        assert!(
            seen.insert(fact.service_id),
            "duplicate capability row for {:?}",
            fact.service_id
        );
        println!(
            "TASK0800_CAPABILITY_FACT service={:?} placing={} reading={} opening={} real_two_person_messaging={}",
            fact.service_id,
            fact.placing,
            fact.reading,
            fact.opening,
            fact.real_two_person_protected_messaging
        );

        let direct = service_capability_facts(service_id_slug(fact.service_id))
            .expect("installed service must have a direct capability read");
        assert_eq!(direct, fact);
    }

    assert_eq!(
        seen.len(),
        installed_count,
        "every installed service must be represented exactly once"
    );

    let absent = service_capability_facts("not-installed-service");
    println!(
        "TASK0800_ABSENT_SERVICE_RECORD={}",
        if absent.is_some() { "some" } else { "none" }
    );
    assert!(
        absent.is_none(),
        "a service with no record must return nothing, not four true facts"
    );
}

fn service_id_slug(service_id: ServiceKind) -> &'static str {
    match service_id {
        ServiceKind::Discord => "discord",
        ServiceKind::Telegram => "telegram",
        ServiceKind::WhatsApp => "whatsapp",
        ServiceKind::Email => "email",
        ServiceKind::Signal => "signal",
    }
}
