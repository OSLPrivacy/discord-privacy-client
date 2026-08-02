//! T21-T25: Space channels have an explicit, replicated kind and position.

use ipc::space_roster::{SpaceChannel, SpaceChannelId, SpaceChannelKind};

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
