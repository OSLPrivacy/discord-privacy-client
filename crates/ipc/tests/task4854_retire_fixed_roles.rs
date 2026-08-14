use ipc::permission_catalogue::REQUIRED_PERMISSION_ROWS;
use ipc::space_roster::{
    load_space_roster_records, migrate_fixed_space_roles_to_records, save_space_roster_records,
    SpaceEpoch, SpaceMemberId, SpaceRoleId, SpaceRoleLimits, SpaceRoleMentionRule, SpaceRoleRecord,
    SpaceRoster, SpaceRosterError, SPACE_ROSTER_FILE,
};
use tempfile::tempdir;

fn custom_role(index: u8, name: &str) -> SpaceRoleRecord {
    SpaceRoleRecord::new(
        SpaceRoleId::from_bytes([index; SpaceRoleId::LENGTH]),
        name.to_string(),
        format!("#{:02x}{:02x}{:02x}", 40 + index, 90 + index, 140 + index),
        format!("role-{index}"),
        index as u32,
        index % 2 == 0,
        if index % 2 == 0 {
            SpaceRoleMentionRule::MembersWithRole
        } else {
            SpaceRoleMentionRule::NotMentionable
        },
        [REQUIRED_PERMISSION_ROWS[index as usize % REQUIRED_PERMISSION_ROWS.len()].to_string()],
        SpaceRoleLimits {
            max_members: Some(100 + index as u32),
            max_channel_overrides: Some(10 + index as u32),
            max_mentions_per_hour: Some(index as u32),
        },
        1_700_000_000_000 + index as u64,
    )
}

#[test]
fn task4854_custom_roles_are_persisted_records_and_read_back_by_id() {
    let directory = tempdir().unwrap();
    let path = directory.path().join(SPACE_ROSTER_FILE);

    let space_id = ipc::space_roster::SpaceId::generate();
    let member = SpaceMemberId::from_identity_key_digest([0x7a; SpaceMemberId::LENGTH]).unwrap();
    let roles = vec![
        custom_role(1, "Reader"),
        custom_role(2, "Writer"),
        custom_role(3, "Planner"),
        custom_role(4, "Auditor"),
        custom_role(5, "Operator"),
        custom_role(6, "Designer"),
    ];
    let role_ids: Vec<SpaceRoleId> = roles.iter().map(|role| role.id).collect();

    let mut roster = SpaceRoster::default();
    roster
        .insert_with_roles(space_id, SpaceEpoch::INITIAL, [member], roles)
        .unwrap();

    let duplicate_id = custom_role(9, "Duplicate");
    let duplicate_refusal = roster.insert_with_roles(
        ipc::space_roster::SpaceId::generate(),
        SpaceEpoch::INITIAL,
        [member],
        [duplicate_id.clone(), duplicate_id],
    );
    assert_eq!(duplicate_refusal, Err(SpaceRosterError::DuplicateRoleId));

    ipc::main_password::set_file_storage_key(Some([0x54; 32]));
    let serialized = serde_json::to_vec(&roster).unwrap();
    save_space_roster_records(&path, &roster).unwrap();
    let loaded = load_space_roster_records(&path).unwrap();
    let second_loaded = load_space_roster_records(&path).unwrap();
    ipc::main_password::set_file_storage_key(None);
    let first_report = migrate_fixed_space_roles_to_records(&loaded);
    let second_report = migrate_fixed_space_roles_to_records(&second_loaded);
    assert_eq!(first_report, second_report);
    assert_eq!(first_report.persisted_role_records, 6);

    let roles_read_by_id = role_ids
        .iter()
        .filter(|role_id| loaded.role_by_id(space_id, **role_id).is_some())
        .count();
    let loaded_space = loaded.get(space_id).unwrap();
    let loaded_roles = loaded_space.roles().count();
    assert_eq!(loaded_roles, 6);
    assert_eq!(roles_read_by_id, 6);
    assert_eq!(
        loaded.role_by_id(space_id, role_ids[5]).unwrap().name,
        "Designer"
    );

    let persisted_json = String::from_utf8(serialized).unwrap();
    let fixed_role_name_hits = ["Member", "Moderator", "Admin"]
        .iter()
        .filter(|name| persisted_json.contains(**name))
        .count();
    assert_eq!(fixed_role_name_hits, 0);

    println!("TASK4854_CUSTOM_ROLES_SAVED={loaded_roles}");
    println!("TASK4854_CUSTOM_ROLES_READ_BY_ID={roles_read_by_id}");
    println!("TASK4854_FIXED_ROLE_NAME_HITS={fixed_role_name_hits}");
    println!(
        "TASK4854_MIGRATION_ROLE_RECORDS={}",
        first_report.persisted_role_records
    );
}
