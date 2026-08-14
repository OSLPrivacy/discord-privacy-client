use ipc::commands::{
    cmd_osl_add_channel_reaction, cmd_osl_count_channel_reactions,
    cmd_osl_read_channel_reaction_policy, cmd_osl_set_channel_reaction_policy,
    cmd_osl_set_open_channel_members, cmd_osl_write_server_member_list,
};
use ipc::server_membership::{ChannelReactionPolicyMode, CHANNEL_REACTION_SETTING_SENTENCE};
use ipc::state::AppState;

const SERVER_ID: &str = "server-5029";
const CHANNEL_ID: &str = "channel-5029";
const OWNER: &str = "Nia Owner";
const MESSAGE_CHOSEN: &str = "message-5029-chosen";
const MESSAGE_NONE: &str = "message-5029-none";
const MESSAGE_ALL: &str = "message-5029-all";
const THUMBS_UP: &str = "👍";
const HEART: &str = "❤️";
const PARTY: &str = "🎉";
const ANGRY: &str = "😡";
const LAUGH: &str = "😂";

fn seed_open_channel(state: &AppState) {
    cmd_osl_write_server_member_list(
        state,
        SERVER_ID.to_owned(),
        OWNER.to_owned(),
        "2026-08-07T09:00:00Z".to_owned(),
    )
    .expect("server owner row can be written");
    cmd_osl_set_open_channel_members(state, SERVER_ID.to_owned(), CHANNEL_ID.to_owned())
        .expect("channel can be created open");
}

fn add_reaction(
    state: &AppState,
    message_id: &str,
    actor_name: &str,
    emoji: &str,
) -> Result<ipc::server_membership::ChannelReactionResult, String> {
    cmd_osl_add_channel_reaction(
        state,
        SERVER_ID.to_owned(),
        CHANNEL_ID.to_owned(),
        message_id.to_owned(),
        actor_name.to_owned(),
        emoji.to_owned(),
    )
}

#[test]
fn task5029_channel_reaction_policy_enforces_chosen_none_and_all() {
    let state = AppState::new();
    seed_open_channel(&state);

    let chosen = cmd_osl_set_channel_reaction_policy(
        &state,
        SERVER_ID.to_owned(),
        CHANNEL_ID.to_owned(),
        ChannelReactionPolicyMode::ChosenSet,
        vec![THUMBS_UP.to_owned(), HEART.to_owned(), PARTY.to_owned()],
    )
    .expect("chosen reaction policy saves");
    let chosen_accepted = [THUMBS_UP, HEART, PARTY]
        .into_iter()
        .enumerate()
        .map(|(index, emoji)| {
            add_reaction(&state, MESSAGE_CHOSEN, &format!("actor-{index}"), emoji)
        })
        .collect::<Result<Vec<_>, _>>()
        .expect("chosen policy accepts only configured emoji");
    let refused = [ANGRY, LAUGH]
        .into_iter()
        .map(|emoji| {
            let error = add_reaction(&state, MESSAGE_CHOSEN, "refused-actor", emoji)
                .expect_err("chosen policy must refuse unconfigured emoji");
            assert!(error.contains(emoji), "refusal must name {emoji}: {error}");
            (emoji, error)
        })
        .collect::<Vec<_>>();
    let chosen_count = cmd_osl_count_channel_reactions(
        &state,
        SERVER_ID.to_owned(),
        CHANNEL_ID.to_owned(),
        MESSAGE_CHOSEN.to_owned(),
    )
    .expect("chosen reaction count reads");

    let none = cmd_osl_set_channel_reaction_policy(
        &state,
        SERVER_ID.to_owned(),
        CHANNEL_ID.to_owned(),
        ChannelReactionPolicyMode::None,
        Vec::new(),
    )
    .expect("none reaction policy saves");
    let none_landed = [THUMBS_UP, HEART, PARTY]
        .into_iter()
        .filter(|emoji| add_reaction(&state, MESSAGE_NONE, "none-actor", emoji).is_ok())
        .count();
    let none_count = cmd_osl_count_channel_reactions(
        &state,
        SERVER_ID.to_owned(),
        CHANNEL_ID.to_owned(),
        MESSAGE_NONE.to_owned(),
    )
    .expect("none reaction count reads");

    let all = cmd_osl_set_channel_reaction_policy(
        &state,
        SERVER_ID.to_owned(),
        CHANNEL_ID.to_owned(),
        ChannelReactionPolicyMode::All,
        Vec::new(),
    )
    .expect("all reaction policy saves");
    let all_accepted = [ANGRY, LAUGH]
        .into_iter()
        .enumerate()
        .map(|(index, emoji)| {
            add_reaction(&state, MESSAGE_ALL, &format!("all-actor-{index}"), emoji)
        })
        .collect::<Result<Vec<_>, _>>()
        .expect("all policy accepts previously refused emoji");
    let all_count = cmd_osl_count_channel_reactions(
        &state,
        SERVER_ID.to_owned(),
        CHANNEL_ID.to_owned(),
        MESSAGE_ALL.to_owned(),
    )
    .expect("all reaction count reads");
    let read_all =
        cmd_osl_read_channel_reaction_policy(&state, SERVER_ID.to_owned(), CHANNEL_ID.to_owned())
            .expect("all reaction policy reads");
    let settings_source = include_str!("../../../apps/osl-hub-ui/src/main.ts");

    println!(
        "TASK5029 chosen_policy_mode={} allowed_set={} accepted_count={} landed_count={}",
        chosen.mode.name(),
        chosen.allowed_emoji.join(","),
        chosen_accepted.len(),
        chosen_count
    );
    println!(
        "TASK5029 chosen_refused_count={} refused_names={} refusal_errors={}",
        refused.len(),
        refused
            .iter()
            .map(|(emoji, _)| *emoji)
            .collect::<Vec<_>>()
            .join(","),
        refused
            .iter()
            .map(|(_, error)| error.as_str())
            .collect::<Vec<_>>()
            .join(" | ")
    );
    println!(
        "TASK5029 none_policy_mode={} attempted=3 landed_count={} stored_count={}",
        none.mode.name(),
        none_landed,
        none_count
    );
    println!(
        "TASK5029 all_policy_mode={} accepted_previously_refused_count={} accepted_names={} landed_count={}",
        all.mode.name(),
        all_accepted.len(),
        all_accepted.iter().map(|result| result.emoji.as_str()).collect::<Vec<_>>().join(","),
        all_count
    );
    println!("TASK5029 setting_sentence={}", read_all.setting_sentence);

    assert_eq!(chosen.allowed_emoji, vec![HEART, PARTY, THUMBS_UP]);
    assert_eq!(chosen_accepted.len(), 3);
    assert_eq!(chosen_count, 3);
    assert_eq!(refused.len(), 2);
    assert_eq!(
        refused.iter().map(|(emoji, _)| *emoji).collect::<Vec<_>>(),
        vec![ANGRY, LAUGH]
    );
    assert_eq!(none.mode, ChannelReactionPolicyMode::None);
    assert_eq!(none_landed, 0);
    assert_eq!(none_count, 0);
    assert_eq!(all.mode, ChannelReactionPolicyMode::All);
    assert_eq!(all_accepted.len(), 2);
    assert_eq!(all_count, 2);
    assert_eq!(read_all.setting_sentence, CHANNEL_REACTION_SETTING_SENTENCE);
    assert!(settings_source.contains(CHANNEL_REACTION_SETTING_SENTENCE));
}
