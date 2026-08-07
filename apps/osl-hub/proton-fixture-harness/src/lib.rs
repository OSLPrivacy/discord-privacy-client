#[allow(dead_code)]
#[path = "../../src/proton_fake_page.rs"]
mod proton_fake_page;

#[cfg(test)]
mod tests {
    use super::proton_fake_page::{
        ProtonFakePageConnection, ProtonFakePageControlKind, TASK_1244_PROTON_WORDS,
    };

    #[test]
    fn task_1244_fake_page_proton_email_flow() {
        let mut proton = ProtonFakePageConnection::new();
        let initial_sent_count = proton.sent_count();
        let initial_control_names = proton
            .controls()
            .into_iter()
            .map(|control| control.name)
            .collect::<Vec<_>>();

        let composed_words = proton
            .compose_marked_cover_words()
            .expect("Compose returns the Proton 1244 words");
        let placed = proton
            .place_marked_cover_message(&composed_words)
            .expect("Place accepts the exact Proton 1244 words");
        let placed_count_after_place = proton.placed_message_count();
        let readback_words = proton
            .readback_marked_words()
            .expect("Readback returns the exact Proton 1244 words");
        let send_receipt = proton
            .send_placed_message_with_words()
            .expect("Send accepts the placed Proton message");
        let placed_count_before_remove = proton.placed_message_count();
        let sent_count_before_remove = proton.sent_count();
        let remove_send = proton.remove_control(ProtonFakePageControlKind::Send);
        let remove_send_refused = remove_send.is_err();
        let remove_send_error = remove_send.expect_err("removing Send must be refused");
        let placed_count_after_remove = proton.placed_message_count();
        let sent_count_after_remove = proton.sent_count();

        println!("TASK1244_INITIAL_SENT_COUNT={initial_sent_count}");
        println!("TASK1244_CONTROL_NAMES={}", initial_control_names.join(","));
        println!("TASK1244_COMPOSE_WORDS={composed_words}");
        println!("TASK1244_PLACE_WORDS={}", placed.words);
        println!("TASK1244_PLACED_COUNT_AFTER_PLACE={placed_count_after_place}");
        println!("TASK1244_READBACK_WORDS={readback_words}");
        println!("TASK1244_SEND_WORDS={}", send_receipt.words);
        println!("TASK1244_SENT_COUNT_AFTER_SEND={}", send_receipt.sent_count);
        println!("TASK1244_REMOVE_SEND_REFUSED={remove_send_refused}");
        println!("TASK1244_REMOVE_SEND_ERROR={remove_send_error}");
        println!("TASK1244_PLACED_COUNT_BEFORE_REMOVE={placed_count_before_remove}");
        println!("TASK1244_PLACED_COUNT_AFTER_REMOVE={placed_count_after_remove}");
        println!("TASK1244_SENT_COUNT_BEFORE_REMOVE={sent_count_before_remove}");
        println!("TASK1244_SENT_COUNT_AFTER_REMOVE={sent_count_after_remove}");

        assert_eq!(initial_sent_count, 0);
        assert_eq!(
            initial_control_names,
            vec!["Compose", "Place", "Readback", "Send"]
        );
        assert_eq!(composed_words, TASK_1244_PROTON_WORDS);
        assert_eq!(placed.words, TASK_1244_PROTON_WORDS);
        assert_eq!(placed_count_after_place, 1);
        assert_eq!(readback_words, TASK_1244_PROTON_WORDS);
        assert_eq!(send_receipt.words, TASK_1244_PROTON_WORDS);
        assert_eq!(send_receipt.sent_count, 1);
        assert!(remove_send_refused);
        assert_eq!(placed_count_after_remove, placed_count_before_remove);
        assert_eq!(sent_count_after_remove, sent_count_before_remove);
    }
}
