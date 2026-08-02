//! T21-C3 regression coverage for client-authoritative Space roster state.

use ipc::space_roster::{SpaceEpoch, SpaceId, SpaceMemberId, SpaceRoster, SpaceRosterError};

fn member(byte: u8) -> SpaceMemberId {
    SpaceMemberId::from_identity_key_digest([byte; SpaceMemberId::LENGTH]).unwrap()
}

#[test]
fn roster_membership_is_local_to_each_space_and_rejects_replacement() {
    let first_space = SpaceId::generate();
    let second_space = SpaceId::generate();
    let first_epoch = SpaceEpoch::INITIAL.advance().unwrap();
    let mut roster = SpaceRoster::default();

    roster
        .insert(first_space, first_epoch, [member(1), member(2)])
        .unwrap();

    let first_members: Vec<_> = roster.get(first_space).unwrap().members().collect();
    assert_eq!(first_members, vec![member(1), member(2)]);
    assert_eq!(roster.get(first_space).unwrap().epoch(), first_epoch);
    assert!(roster.get(second_space).is_none());

    assert_eq!(
        roster.insert(first_space, first_epoch, [member(3)]),
        Err(SpaceRosterError::SpaceAlreadyExists),
        "a local roster snapshot cannot silently replace the authoritative membership"
    );
    assert_eq!(
        roster
            .get(first_space)
            .unwrap()
            .members()
            .collect::<Vec<_>>(),
        vec![member(1), member(2)],
    );

    let serialized = serde_json::to_vec(&roster).unwrap();
    let restored: SpaceRoster = serde_json::from_slice(&serialized).unwrap();
    assert_eq!(restored, roster, "the whole roster stays in client storage");
}
