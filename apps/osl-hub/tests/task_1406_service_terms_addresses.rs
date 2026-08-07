use osl_privacy_hub::hub_command_surface::{
    service_terms_address, supported_service_terms_addresses,
};

#[test]
fn terms_command_returns_discord_address_and_rejects_unknown_service() {
    let supported = supported_service_terms_addresses();
    let discord = service_terms_address("discord").expect("discord is a supported service");
    let unknown = service_terms_address("myspace").expect_err("unknown service must be rejected");

    assert_eq!(supported.len(), 5);
    assert!(supported
        .iter()
        .all(|address| !address.terms_address.is_empty()));
    assert_eq!(discord.service_id, "discord");
    assert_eq!(discord.terms_address, "https://discord.com/terms");
    assert_eq!(unknown, "unknown service");

    println!(
        "TASK1406 direct_terms_command=get_service_terms_address supported_terms_addresses={} discord_service={} discord_terms_address={} unknown_service_rejected={} unknown_error={}",
        supported.len(),
        discord.service_id,
        discord.terms_address,
        unknown == "unknown service",
        unknown
    );
}
