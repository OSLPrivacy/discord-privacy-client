#![cfg(feature = "core")]

use osl_privacy_hub::core_bridge::HubCoreState;
use osl_privacy_hub::security::{
    add_friend_code, export_friend_code, manual_peer_binding_for_email_address,
    set_friend_email_address, HubSecurityState,
};

const LIB_SOURCE: &str = include_str!("../src/lib.rs");

fn core_with_identity(user_id: &str) -> HubCoreState {
    let core = HubCoreState::default();
    *core.osl.identity.lock().expect("identity lock") =
        Some(keystore::generate_identity(user_id.to_owned()));
    core
}

fn add_friend(
    owner: &HubCoreState,
    security: &HubSecurityState,
    peer_user_id: &str,
    name: &str,
) -> String {
    let peer = core_with_identity(peer_user_id);
    let invite = export_friend_code(&peer).expect("peer exports focused invite");
    add_friend_code(owner, security, invite.friend_code, Some(name.to_owned()))
        .expect("owner adds focused friend")
        .person_id
}

#[test]
fn task_4506_allowed_list_resolves_one_email_address_per_friend() {
    let owner = core_with_identity("task-4506-owner");
    let security = HubSecurityState::default();
    let addressed_person_id = add_friend(&owner, &security, "task-4506-pine", "Pine 4506");
    let no_address_person_id = add_friend(&owner, &security, "task-4506-maple", "Maple 4506");
    let duplicate_candidate_id = add_friend(&owner, &security, "task-4506-elm", "Elm 4506");

    let addressed = set_friend_email_address(
        &security,
        addressed_person_id.clone(),
        "Pine4506@Example.Test".to_owned(),
    )
    .expect("one address is added to one friend");
    assert_eq!(
        addressed.email_address.as_deref(),
        Some("pine4506@example.test")
    );

    let found = manual_peer_binding_for_email_address(&owner, "pine4506@example.test")
        .expect("friend with an address is found by that address");
    let found_by_address = usize::from(found.person_id == addressed_person_id);

    let no_address_lookup = manual_peer_binding_for_email_address(&owner, "maple4506@example.test")
        .expect_err("friend with no address is not found by address");
    let no_address_not_found = usize::from(
        no_address_lookup == "OSL sender is not a friend" && !no_address_person_id.is_empty(),
    );

    let duplicate_refusal = set_friend_email_address(
        &security,
        duplicate_candidate_id,
        "PINE4506@example.test".to_owned(),
    )
    .expect_err("same address on two friends is refused");
    let duplicate_refused_by_name =
        usize::from(duplicate_refusal == "OSL friend email address already belongs to Pine 4506");

    let allowed_list_count = LIB_SOURCE.matches("static FOCUSED_ALLOWED_LIST").count();

    println!(
        "TASK4506 found_by_address={found_by_address} address=pine4506@example.test person_id={}",
        found.person_id
    );
    println!("TASK4506 no_address_not_found={no_address_not_found} refusal=\"{no_address_lookup}\" no_address_person_id={no_address_person_id}");
    println!("TASK4506 duplicate_refused_by_name={duplicate_refused_by_name} refusal=\"{duplicate_refusal}\"");
    println!("TASK4506 allowed_list_search_count={allowed_list_count} search=\"static FOCUSED_ALLOWED_LIST\"");

    assert_eq!(found_by_address, 1);
    assert_eq!(no_address_not_found, 1);
    assert_eq!(duplicate_refused_by_name, 1);
    assert_eq!(allowed_list_count, 1);
}
