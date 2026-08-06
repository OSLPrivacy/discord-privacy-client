use std::collections::BTreeSet;

use osl_privacy_hub::models::ServiceKind;
use osl_privacy_hub::services::{
    drawn_controls_without_capability_record, installed_service_capability_facts,
    installed_service_count, installed_service_screen_trees,
    service_screen_tree_from_capability_facts, ServiceCapabilityFacts, ServiceControlCapability,
};

#[test]
fn task_3764_controls_are_only_drawn_for_built_capabilities() {
    let installed_count = installed_service_count();
    let capability_records = installed_service_capability_facts();
    let screen_trees = installed_service_screen_trees();
    let drawn_control_count: usize = screen_trees.iter().map(|tree| tree.controls.len()).sum();
    let missing_record_count =
        drawn_controls_without_capability_record(&screen_trees, &capability_records);

    println!("TASK3764_INSTALLED_SERVICE_COUNT={installed_count}");
    println!(
        "TASK3764_CAPABILITY_RECORD_COUNT={}",
        capability_records.len()
    );
    println!("TASK3764_SCREEN_TREE_COUNT={}", screen_trees.len());
    println!("TASK3764_DRAWN_CONTROL_COUNT={drawn_control_count}");
    println!("TASK3764_DRAWN_CONTROLS_WITH_NO_CAPABILITY_RECORD={missing_record_count}");

    assert_ne!(
        installed_count, 0,
        "the proof must not pass with no services"
    );
    assert_eq!(
        capability_records.len(),
        installed_count,
        "every installed service must have a capability record"
    );
    assert_eq!(
        screen_trees.len(),
        installed_count,
        "every installed service must have one screen tree"
    );
    assert_eq!(
        missing_record_count, 0,
        "every drawn control must be backed by a built capability record"
    );

    let email = capability_records
        .iter()
        .copied()
        .find(|facts| facts.service_id == ServiceKind::Email)
        .expect("Email capability record must exist");
    assert!(
        !email.placing,
        "Email placing must be an explicit not-built record"
    );
    let email_tree = service_screen_tree_from_capability_facts(email);
    let email_control_ids = control_ids(&email_tree);
    let absent = !email_control_ids.contains(ServiceControlCapability::PlaceMessage.id());
    println!(
        "TASK3764_NOT_BUILT_CONTROL service=Email capability={} absent={absent}",
        ServiceControlCapability::PlaceMessage.id()
    );
    assert!(
        absent,
        "a control whose capability record says not built must be absent"
    );

    let baseline = ServiceCapabilityFacts {
        service_id: ServiceKind::Discord,
        placing: true,
        reading: true,
        opening: true,
        real_two_person_protected_messaging: false,
    };
    let mut switched_off = baseline;
    switched_off.reading = false;

    let baseline_ids = control_ids(&service_screen_tree_from_capability_facts(baseline));
    let switched_ids = control_ids(&service_screen_tree_from_capability_facts(switched_off));
    let removed = baseline_ids
        .difference(&switched_ids)
        .copied()
        .collect::<Vec<_>>();
    let added = switched_ids
        .difference(&baseline_ids)
        .copied()
        .collect::<Vec<_>>();
    let unchanged = baseline_ids
        .intersection(&switched_ids)
        .copied()
        .collect::<Vec<_>>();

    println!(
        "TASK3764_SWITCH_BASELINE_CONTROL_COUNT={}",
        baseline_ids.len()
    );
    println!("TASK3764_SWITCH_OFF_CONTROL_COUNT={}", switched_ids.len());
    println!("TASK3764_SWITCH_REMOVED_COUNT={}", removed.len());
    println!(
        "TASK3764_SWITCH_REMOVED_CONTROL={}",
        removed.first().copied().unwrap_or("none")
    );
    println!("TASK3764_SWITCH_ADDED_COUNT={}", added.len());
    println!("TASK3764_SWITCH_UNCHANGED_COUNT={}", unchanged.len());

    assert_eq!(removed, vec![ServiceControlCapability::ReadMessages.id()]);
    assert_eq!(
        added.len(),
        0,
        "switching one capability off must not add controls"
    );
    assert_eq!(
        unchanged.len(),
        baseline_ids.len() - 1,
        "switching one capability off must leave every other control unchanged"
    );
}

fn control_ids(tree: &osl_privacy_hub::services::ServiceScreenTree) -> BTreeSet<&'static str> {
    tree.controls
        .iter()
        .map(|control| control.capability.id())
        .collect()
}
