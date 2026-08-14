#[path = "../../src/proton_fake_page.rs"]
pub mod proton_fake_page;

#[cfg(test)]
mod tests {
    use super::proton_fake_page::{
        ProtonFakePageConnection, ProtonFakePageControlKind, TASK_1243_PROTON_MARKER,
    };

    #[test]
    fn task_1243_fake_proton_page_controls_place_read_send_and_refuse_send_removal() {
        let mut proton = ProtonFakePageConnection::new();
        let initial_sent_count = proton.sent_count();
        let initial_control_names = proton
            .controls()
            .into_iter()
            .map(|control| control.name)
            .collect::<Vec<_>>();

        let marked_words = "OSL-PROTON-1243 cover message";
        let placed = proton
            .place_marked_cover_message(marked_words)
            .expect("Place adds one marked Proton cover message");
        let placed_count_after_place = proton.placed_message_count();
        let read_words = proton
            .read_marked_words(TASK_1243_PROTON_MARKER)
            .expect("Read returns the marked Proton words");
        let sent_count_after_send = proton
            .send_placed_message()
            .expect("Send accepts the placed Proton message");
        let placed_count_before_remove = proton.placed_message_count();
        let sent_count_before_remove = proton.sent_count();
        let remove_send = proton.remove_control(ProtonFakePageControlKind::Send);
        let remove_send_refused = remove_send.is_err();
        let remove_send_error = remove_send.expect_err("removing Send must be refused");
        let placed_count_after_remove = proton.placed_message_count();
        let sent_count_after_remove = proton.sent_count();

        println!("TASK1243_INITIAL_SENT_COUNT={initial_sent_count}");
        println!("TASK1243_CONTROL_NAMES={}", initial_control_names.join(","));
        println!("TASK1243_PLACED_MARKER={}", placed.marker);
        println!("TASK1243_PLACED_WORDS={}", placed.words);
        println!("TASK1243_PLACED_COUNT_AFTER_PLACE={placed_count_after_place}");
        println!("TASK1243_READ_WORDS={read_words}");
        println!("TASK1243_SENT_COUNT_AFTER_SEND={sent_count_after_send}");
        println!("TASK1243_REMOVE_SEND_REFUSED={remove_send_refused}");
        println!("TASK1243_REMOVE_SEND_ERROR={remove_send_error}");
        println!("TASK1243_PLACED_COUNT_BEFORE_REMOVE={placed_count_before_remove}");
        println!("TASK1243_PLACED_COUNT_AFTER_REMOVE={placed_count_after_remove}");
        println!("TASK1243_SENT_COUNT_BEFORE_REMOVE={sent_count_before_remove}");
        println!("TASK1243_SENT_COUNT_AFTER_REMOVE={sent_count_after_remove}");

        assert_eq!(initial_sent_count, 0);
        assert_eq!(initial_control_names, vec!["Place", "Read", "Send"]);
        assert_eq!(placed.marker, TASK_1243_PROTON_MARKER);
        assert_eq!(placed.words, marked_words);
        assert_eq!(placed_count_after_place, 1);
        assert_eq!(read_words, marked_words);
        assert_eq!(sent_count_after_send, 1);
        assert_eq!(placed_count_before_remove, 1);
        assert!(remove_send_refused);
        assert_eq!(placed_count_after_remove, placed_count_before_remove);
        assert_eq!(sent_count_after_remove, sent_count_before_remove);
    }
}
