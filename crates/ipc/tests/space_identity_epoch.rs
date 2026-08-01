//! T21-C2 regression coverage for local Space identity and membership epochs.

use ipc::space_roster::{SpaceEpoch, SpaceId};

#[test]
fn separate_space_creations_are_unlinkable_and_membership_epochs_are_monotonic() {
    let first_space = SpaceId::generate();
    let second_space = SpaceId::generate();

    assert_ne!(first_space, second_space);
    assert_ne!(first_space.as_bytes(), &[0_u8; SpaceId::LENGTH]);
    assert_ne!(second_space.as_bytes(), &[0_u8; SpaceId::LENGTH]);

    let created = SpaceEpoch::INITIAL
        .advance()
        .expect("create event advances epoch");
    let changed = created.advance().expect("membership change advances epoch");
    assert_eq!(created.get(), 1);
    assert_eq!(changed.get(), 2);
    assert!(changed > created);
}
