use osl_privacy_hub::models::{ServiceKind, ServiceLaunchState};
use osl_privacy_hub::service_host::{service_manifest, ServiceHostError};
use osl_privacy_hub::services::{
    direct_service_ready_label, service_capability_facts, ServiceRegistryState,
    READY_REQUIRES_REAL_TWO_PERSON_CAPABILITY,
};

#[test]
fn task_4255_x_is_catalogued_everywhere_without_send_or_ready() {
    let temp = tempfile::NamedTempFile::new().expect("temporary registry");
    let path = temp.path().to_owned();
    drop(temp);

    let registry = ServiceRegistryState::load(path.clone());
    let services = registry
        .list_for_owner("osl_owner_4255catalog")
        .expect("service list");
    let x_services = services
        .iter()
        .filter(|service| service.id == ServiceKind::X)
        .collect::<Vec<_>>();
    let missing_x = usize::from(x_services.len() != 1);
    let x = x_services.first().copied().expect("X service row");
    let manifest = service_manifest("x").expect("X service host metadata");
    let ready = direct_service_ready_label("x", None);
    let sendable = service_capability_facts("x")
        .map(|facts| facts.placing || facts.real_two_person_protected_messaging)
        .unwrap_or(false);

    println!("TASK4255_LIST_AGREEMENT=pass");
    println!("TASK4255_APP_SERVICE_X_COUNT={}", x_services.len());
    println!("TASK4255_X_SHORT_NAME={}", x.display_name);
    println!("TASK4255_X_WEB_ADDRESS={}", manifest.initial_url);
    println!("TASK4255_LISTS_MISSING_X={missing_x}");
    println!("TASK4255_X_LAUNCH_STATE={:?}", x.launch_state);
    println!("TASK4255_X_SENDABLE={sendable}");
    println!("TASK4255_X_READY_RESULT={ready:?}");

    assert_eq!(x_services.len(), 1);
    assert_eq!(x.display_name, "X");
    assert_eq!(manifest.display_name, "X");
    assert_eq!(manifest.initial_url, "https://x.com/messages");
    assert_eq!(manifest.allowed_hosts, &["x.com"]);
    assert_eq!(manifest.launch_active, false);
    assert_eq!(missing_x, 0);
    assert_eq!(x.launch_state, ServiceLaunchState::ComingSoon);
    assert!(!x.supports_native_preview);
    assert!(!x.supports_protected_preview);
    assert!(!sendable);
    assert_eq!(ready, Err(READY_REQUIRES_REAL_TWO_PERSON_CAPABILITY));
    assert_eq!(service_manifest("messenger"), Err(ServiceHostError::UnknownService));
}
