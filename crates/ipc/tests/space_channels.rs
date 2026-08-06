//! T21-T25: Space channels have an explicit, replicated kind, position, and
//! member list.

use ipc::space_roster::{
    SpaceChannel, SpaceChannelId, SpaceChannelKind, SpaceChannelMemberList, SpaceMemberId,
    SpacePerson,
};

fn member(byte: u8, name: &str) -> SpacePerson {
    SpacePerson::new(
        SpaceMemberId::from_identity_key_digest([byte; SpaceMemberId::LENGTH]).unwrap(),
        name.to_owned(),
    )
}

fn names(people: &[SpacePerson]) -> Vec<String> {
    people.iter().map(|person| person.name.clone()).collect()
}

#[test]
fn channel_kind_is_explicit_not_inferred_from_its_name() {
    let text_named_voice = SpaceChannel::new(
        SpaceChannelId::from_bytes([7; SpaceChannelId::LENGTH]),
        SpaceChannelKind::Text,
        4,
        "voice-lounge".to_owned(),
    );
    let voice_named_general = SpaceChannel::new(
        SpaceChannelId::from_bytes([8; SpaceChannelId::LENGTH]),
        SpaceChannelKind::Voice,
        9,
        "general".to_owned(),
    );

    assert_eq!(text_named_voice.kind, SpaceChannelKind::Text);
    assert_eq!(voice_named_general.kind, SpaceChannelKind::Voice);
    assert_eq!(text_named_voice.position, 4);
    assert_eq!(voice_named_general.position, 9);
}

#[test]
fn task_1376_open_and_limited_channels_report_their_member_lists_and_refuse_by_name() {
    let ada = member(1, "Ada TASK1376");
    let ben = member(2, "Ben TASK1376");
    let cy = member(3, "Cy TASK1376");
    let server_members = vec![ada.clone(), ben.clone(), cy.clone()];

    let open_channel = SpaceChannel::new(
        SpaceChannelId::from_bytes([13; SpaceChannelId::LENGTH]),
        SpaceChannelKind::Text,
        1,
        "task-1376-open".to_owned(),
    )
    .with_member_list(SpaceChannelMemberList::open_to_server());
    let open_members = open_channel
        .read_member_list_as(&ada, &server_members)
        .expect("open channel should list the whole server roster");
    let open_names = names(&open_members);
    assert_eq!(
        open_names,
        vec![
            "Ada TASK1376".to_owned(),
            "Ben TASK1376".to_owned(),
            "Cy TASK1376".to_owned()
        ]
    );

    let limited_channel = SpaceChannel::new(
        SpaceChannelId::from_bytes([14; SpaceChannelId::LENGTH]),
        SpaceChannelKind::Text,
        2,
        "task-1376-limited".to_owned(),
    )
    .with_member_list(SpaceChannelMemberList::limited_to([
        ada.clone(),
        ben.clone(),
    ]));
    let limited_members = limited_channel
        .read_member_list_as(&ben, &server_members)
        .expect("named member should read the limited list");
    let limited_names = names(&limited_members);
    assert_eq!(
        limited_names,
        vec!["Ada TASK1376".to_owned(), "Ben TASK1376".to_owned()]
    );

    let refused = limited_channel
        .read_member_list_as(&cy, &server_members)
        .expect_err("a person omitted from the limited channel must be refused");
    let refusal = refused.to_string();
    assert_eq!(refusal, "OSL: channel read refused for Cy TASK1376");

    println!("TASK1376 open_channel_member_count={}", open_members.len());
    println!("TASK1376 open_channel_members={}", open_names.join("|"));
    println!(
        "TASK1376 limited_channel_member_count={}",
        limited_members.len()
    );
    println!(
        "TASK1376 limited_channel_members={}",
        limited_names.join("|")
    );
    println!("TASK1376 refused_person_name=Cy TASK1376");
    println!("TASK1376 refusal={refusal}");
}
