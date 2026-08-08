#![cfg(feature = "core")]

use osl_privacy_hub::broker::{
    plan_private_message_cover_chunks, MAX_COVER_MESSAGES_PER_PRIVATE_MESSAGE,
    PRIVATE_MESSAGE_BYTES_PER_COVER,
};
use stego::{
    decode_layered_cover, encode_layered_cover, CoverLayerSettings, LayerStrength,
    LayeredCoverInput, SHRUNK_TOKEN_ID_BYTES,
};

fn handle(index: usize) -> [u8; SHRUNK_TOKEN_ID_BYTES] {
    let mut value = *b"0081msg0";
    value[7] = b'0' + u8::try_from(index).expect("test index fits one digit");
    value
}

#[test]
fn task_0081_private_message_cover_count_is_bounded_before_send() {
    let accepted = format!("{}🙂", "a".repeat(PRIVATE_MESSAGE_BYTES_PER_COVER * 2));
    let worked_out_cover_messages = accepted.len().div_ceil(PRIVATE_MESSAGE_BYTES_PER_COVER);
    let chunks = plan_private_message_cover_chunks(&accepted)
        .expect("inside-limit private message is planned");
    assert_eq!(chunks.len(), worked_out_cover_messages);

    let key = b"task-0081-cover-message-limit";
    let settings = CoverLayerSettings::new(
        LayerStrength::High,
        LayerStrength::High,
        LayerStrength::High,
        LayerStrength::High,
    );
    let covers: Vec<_> = chunks
        .iter()
        .enumerate()
        .map(|(index, _chunk)| {
            encode_layered_cover(
                key,
                LayeredCoverInput::SharedHandle(handle(index)),
                settings,
            )
            .expect("each private chunk sends as one cover message")
        })
        .collect();
    let recovered_cover_pointers = covers
        .iter()
        .enumerate()
        .filter(|(index, cover)| {
            decode_layered_cover(key, settings, cover)
                == Some(LayeredCoverInput::SharedHandle(handle(*index)))
        })
        .count();
    assert_eq!(worked_out_cover_messages, 3);
    assert_eq!(covers.len(), worked_out_cover_messages);
    assert_eq!(recovered_cover_pointers, covers.len());

    let characters_to_remove = 17;
    let over_limit = format!(
        "{}{}",
        "x".repeat(PRIVATE_MESSAGE_BYTES_PER_COVER * MAX_COVER_MESSAGES_PER_PRIVATE_MESSAGE),
        "🙂".repeat(characters_to_remove)
    );
    let mut refused_covers = Vec::<String>::new();
    let refusal = match plan_private_message_cover_chunks(&over_limit) {
        Ok(refused_chunks) => {
            refused_covers.extend(refused_chunks.into_iter().map(|_| "cover".to_owned()));
            panic!("an over-limit private message was not refused")
        }
        Err(error) => error,
    };

    println!(
        "TASK0081 accepted_cover_messages_worked_out={worked_out_cover_messages} sent_cover_messages={} recovered_cover_pointers={recovered_cover_pointers} max_cover_messages={} refused_message_name=private_message characters_to_remove={characters_to_remove} refusal=\"{refusal}\" silently_expanded_past_limit_count={}",
        covers.len(),
        MAX_COVER_MESSAGES_PER_PRIVATE_MESSAGE,
        refused_covers.len(),
    );
    assert_eq!(
        refusal,
        "This private message is too long. Remove 17 characters."
    );
    assert_eq!(refused_covers.len(), 0);
}
