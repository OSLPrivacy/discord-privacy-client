//! T21-T25: Space channels have explicit, replicated metadata and position.

use ipc::space_roster::{
    SpaceChannel, SpaceChannelCategory, SpaceChannelCategoryId, SpaceChannelId, SpaceChannelKind,
    SpaceChannelLayout, SpaceChannelLayoutError, SpaceChannelMemberList, SpaceMemberId, SpacePerson,
    CHANNEL_CATEGORY_ONE_LEVEL_REASON,
};

fn channel_id(value: u8) -> SpaceChannelId {
    SpaceChannelId::from_bytes([value; SpaceChannelId::LENGTH])
}

fn category_id(value: u8) -> SpaceChannelCategoryId {
    SpaceChannelCategoryId::from_bytes([value; SpaceChannelCategoryId::LENGTH])
}

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
        channel_id(7),
        SpaceChannelKind::Text,
        4,
        "voice-lounge".to_owned(),
    );
    let voice_named_general = SpaceChannel::new(
        channel_id(8),
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
fn topic_category_membership_and_position_round_trip_exactly() {
    let category_a = category_id(1);
    let category_b = category_id(2);
    let mut fixture = SpaceChannelLayout::new();
    fixture
        .add_category(
            SpaceChannelCategory::new(category_a, 10, "Operations".to_owned()),
            None,
        )
        .unwrap();
    fixture
        .add_category(
            SpaceChannelCategory::new(category_b, 20, "Research".to_owned()),
            None,
        )
        .unwrap();

    let channels = [
        (
            channel_id(10),
            "general",
            "General coordination",
            category_a,
            100,
        ),
        (
            channel_id(11),
            "dispatch",
            "Dispatch queue",
            category_a,
            200,
        ),
        (channel_id(12), "triage", "Triage notes", category_a, 300),
        (channel_id(13), "signals", "Signal review", category_b, 100),
        (channel_id(14), "archive", "Archive index", category_b, 200),
    ];

    for (id, name, topic, category, position) in channels {
        fixture.add_channel(SpaceChannel::with_topic_and_category(
            id,
            SpaceChannelKind::Text,
            topic.to_owned(),
            Some(category),
            position,
            name.to_owned(),
        ));
    }

    let saved = serde_json::to_string_pretty(&fixture).unwrap();
    let read_back: SpaceChannelLayout = serde_json::from_str(&saved).unwrap();

    let expected_topics = [
        (channel_id(10), "General coordination"),
        (channel_id(11), "Dispatch queue"),
        (channel_id(12), "Triage notes"),
        (channel_id(13), "Signal review"),
        (channel_id(14), "Archive index"),
    ];
    let expected_categories = [
        (channel_id(10), Some(category_a)),
        (channel_id(11), Some(category_a)),
        (channel_id(12), Some(category_a)),
        (channel_id(13), Some(category_b)),
        (channel_id(14), Some(category_b)),
    ];
    let expected_positions = [
        (channel_id(10), 100),
        (channel_id(11), 200),
        (channel_id(12), 300),
        (channel_id(13), 100),
        (channel_id(14), 200),
    ];

    let topics_read_back = expected_topics
        .iter()
        .filter(|(id, topic)| {
            read_back
                .channels
                .iter()
                .any(|channel| channel.id == *id && channel.topic == *topic)
        })
        .count();
    let memberships_read_back = expected_categories
        .iter()
        .filter(|(id, category)| {
            read_back
                .channels
                .iter()
                .any(|channel| channel.id == *id && channel.category_id == *category)
        })
        .count();
    let positions_read_back = expected_positions
        .iter()
        .filter(|(id, position)| {
            read_back
                .channels
                .iter()
                .any(|channel| channel.id == *id && channel.position == *position)
        })
        .count();

    assert_eq!(read_back.categories.len(), 2);
    assert_eq!(read_back.channels.len(), 5);
    assert_eq!(topics_read_back, 5);
    assert_eq!(memberships_read_back, 5);
    assert_eq!(positions_read_back, 5);
    assert_eq!(
        read_back
            .ordered_categories()
            .into_iter()
            .map(|category| category.id)
            .collect::<Vec<_>>(),
        vec![category_a, category_b],
    );
    assert_eq!(
        read_back
            .ordered_channels_in_category(Some(category_a))
            .into_iter()
            .map(|channel| channel.id)
            .collect::<Vec<_>>(),
        vec![channel_id(10), channel_id(11), channel_id(12)],
    );

    let mut reordered = read_back.clone();
    let before = reordered.channel_positions();
    reordered.reorder_channel(channel_id(12), 50).unwrap();
    let after = reordered.channel_positions();
    let changed_position_values = before
        .iter()
        .filter(|(id, position)| after.get(id) != Some(position))
        .count();
    assert_eq!(changed_position_values, 1);
    assert_eq!(
        reordered
            .ordered_channels_in_category(Some(category_a))
            .into_iter()
            .map(|channel| channel.id)
            .collect::<Vec<_>>(),
        vec![channel_id(12), channel_id(10), channel_id(11)],
    );

    let nesting_error = reordered
        .add_category(
            SpaceChannelCategory::new(category_id(3), 30, "Nested".to_owned()),
            Some(category_a),
        )
        .unwrap_err();
    assert_eq!(
        nesting_error,
        SpaceChannelLayoutError::CategoryNestingRefused
    );
    assert_eq!(nesting_error.reason(), CHANNEL_CATEGORY_ONE_LEVEL_REASON);

    println!(
        "categories_saved_and_read_back={}",
        read_back.categories.len()
    );
    println!("channels_saved_and_read_back={}", read_back.channels.len());
    println!("topics_read_back={topics_read_back}");
    println!("category_memberships_read_back={memberships_read_back}");
    println!("positions_read_back={positions_read_back}");
    println!("changed_position_values_after_reorder={changed_position_values}");
    println!("category_nesting_refusal_reason={}", nesting_error.reason());
}

#[test]
fn task_1376_open_and_limited_channels_report_their_member_lists_and_refuse_by_name() {
    let ada = member(1, "Ada TASK1376");
    let ben = member(2, "Ben TASK1376");
    let cy = member(3, "Cy TASK1376");
    let server_members = vec![ada.clone(), ben.clone(), cy.clone()];

    let open_channel = SpaceChannel::new(channel_id(13), SpaceChannelKind::Text, 1, "task-1376-open".to_owned())
        .with_member_list(SpaceChannelMemberList::open_to_server());
    let open_members = open_channel.read_member_list_as(&ada, &server_members).unwrap();
    assert_eq!(names(&open_members), vec!["Ada TASK1376", "Ben TASK1376", "Cy TASK1376"]);

    let limited_channel = SpaceChannel::new(channel_id(14), SpaceChannelKind::Text, 2, "task-1376-limited".to_owned())
        .with_member_list(SpaceChannelMemberList::limited_to([ada.clone(), ben.clone()]));
    let limited_members = limited_channel.read_member_list_as(&ben, &server_members).unwrap();
    assert_eq!(names(&limited_members), vec!["Ada TASK1376", "Ben TASK1376"]);

    let refusal = limited_channel.read_member_list_as(&cy, &server_members).unwrap_err().to_string();
    assert_eq!(refusal, "OSL: channel read refused for Cy TASK1376");
    println!("TASK1376 open_channel_member_count={}", open_members.len());
    println!("TASK1376 limited_channel_member_count={}", limited_members.len());
    println!("TASK1376 refusal={refusal}");
}
