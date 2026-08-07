use ipc::allowed_places::AllowedPlaceRecord;

#[test]
fn direct_record_check_prints_discord_direct_message_fields() {
    let record =
        AllowedPlaceRecord::discord_direct_message("900000000000000001", "900000000000000003");

    println!("TASK0100 allowed_place_record.app={}", record.app);
    println!("TASK0100 allowed_place_record.account={}", record.account);
    println!("TASK0100 allowed_place_record.kind={}", record.kind);
    println!(
        "TASK0100 allowed_place_record.stable_id={}",
        record.stable_id
    );

    assert_eq!(record.app, "discord");
    assert_eq!(record.account, "900000000000000001");
    assert_eq!(record.kind, "direct_message");
    assert_eq!(
        record.stable_id,
        "discord:900000000000000001:direct_message:900000000000000003"
    );
}
