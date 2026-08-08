#[path = "../src/web_surface_adapter/aol.rs"]
mod aol;

use aol::{AolFakePageConnection, AolFakePageError, AOL_1257_CONTROL_NAMES, AOL_1257_MARKED_WORDS};

#[test]
fn task_1257_aol_fake_page_email_flow_and_refuse_body_removal_before_placement() {
    let mut fixture = AolFakePageConnection::new_email_flow();

    let initial_sent = fixture.sent_email_count();
    let initial_controls = fixture.control_names();
    println!("TASK1257 initial_sent_email_count={initial_sent}");
    println!("TASK1257 named_controls={}", initial_controls.join(","));

    let compose_words = fixture
        .compose_marked_email()
        .expect("Compose produces the marked AOL email words")
        .to_owned();
    println!("TASK1257 compose_words={compose_words}");

    let body_words = fixture
        .body_marked_words()
        .expect("Body returns the marked AOL email words")
        .to_owned();
    println!("TASK1257 body_words={body_words}");

    let place_words = fixture
        .place_composed_message()
        .expect("Place places the marked AOL email words")
        .to_owned();
    let after_place_count = fixture.placed_message_count();
    let after_place_sent = fixture.sent_email_count();
    println!("TASK1257 place_words={place_words}");
    println!("TASK1257 after_place_marked_message_count={after_place_count}");
    println!("TASK1257 after_place_sent_email_count={after_place_sent}");

    let readback_words = fixture
        .readback_marked_words()
        .expect("Readback returns the placed AOL marked words")
        .to_owned();
    println!("TASK1257 readback_words={readback_words}");

    let send_words = fixture
        .send_readback_message()
        .expect("Send raises the AOL sent count and preserves the words")
        .to_owned();
    let after_send_count = fixture.sent_email_count();
    let after_send_placed_count = fixture.placed_message_count();
    println!("TASK1257 send_words={send_words}");
    println!("TASK1257 after_send_sent_email_count={after_send_count}");
    println!("TASK1257 after_send_placed_message_count={after_send_placed_count}");

    let mut removal_fixture = AolFakePageConnection::new_email_flow();
    let removal_compose_words = removal_fixture
        .compose_marked_email()
        .expect("second Compose produces the marked AOL email words")
        .to_owned();
    let before_remove_placed_count = removal_fixture.placed_message_count();
    let before_remove_sent_count = removal_fixture.sent_email_count();
    let remove_body = removal_fixture
        .remove_control("Body")
        .expect_err("removing Body is refused before placement");
    let after_remove_placed_count = removal_fixture.placed_message_count();
    let after_remove_sent_count = removal_fixture.sent_email_count();
    println!("TASK1257 second_compose_words={removal_compose_words}");
    println!("TASK1257 before_remove_body_placed_message_count={before_remove_placed_count}");
    println!("TASK1257 before_remove_body_sent_email_count={before_remove_sent_count}");
    println!("TASK1257 remove_body_refusal={remove_body:?}");
    println!("TASK1257 after_remove_body_placed_message_count={after_remove_placed_count}");
    println!("TASK1257 after_remove_body_sent_email_count={after_remove_sent_count}");

    let second_body_words = removal_fixture
        .body_marked_words()
        .expect("Body remains available after refused removal")
        .to_owned();
    let second_place_words = removal_fixture
        .place_composed_message()
        .expect("second Place places the marked AOL email words")
        .to_owned();
    let second_readback_words = removal_fixture
        .readback_marked_words()
        .expect("second Readback returns the placed AOL marked words")
        .to_owned();
    let second_send_words = removal_fixture
        .send_readback_message()
        .expect("second Send raises the AOL sent count")
        .to_owned();
    let second_after_send_count = removal_fixture.sent_email_count();
    let second_after_send_placed_count = removal_fixture.placed_message_count();
    println!("TASK1257 second_body_words={second_body_words}");
    println!("TASK1257 second_place_words={second_place_words}");
    println!("TASK1257 second_readback_words={second_readback_words}");
    println!("TASK1257 second_send_words={second_send_words}");
    println!("TASK1257 second_after_send_sent_email_count={second_after_send_count}");
    println!("TASK1257 second_after_send_placed_message_count={second_after_send_placed_count}");

    assert_eq!(initial_sent, 0);
    assert_eq!(initial_controls, AOL_1257_CONTROL_NAMES);
    assert_eq!(compose_words, AOL_1257_MARKED_WORDS);
    assert_eq!(body_words, AOL_1257_MARKED_WORDS);
    assert_eq!(place_words, AOL_1257_MARKED_WORDS);
    assert_eq!(after_place_count, 1);
    assert_eq!(after_place_sent, 0);
    assert_eq!(readback_words, AOL_1257_MARKED_WORDS);
    assert_eq!(send_words, AOL_1257_MARKED_WORDS);
    assert_eq!(after_send_count, 1);
    assert_eq!(after_send_placed_count, 1);
    assert_eq!(removal_compose_words, AOL_1257_MARKED_WORDS);
    assert_eq!(before_remove_placed_count, 0);
    assert_eq!(before_remove_sent_count, 0);
    assert_eq!(
        remove_body,
        AolFakePageError::ProtectedControlRemovalRefused("Body")
    );
    assert_eq!(after_remove_placed_count, before_remove_placed_count);
    assert_eq!(after_remove_sent_count, before_remove_sent_count);
    assert_eq!(second_body_words, AOL_1257_MARKED_WORDS);
    assert_eq!(second_place_words, AOL_1257_MARKED_WORDS);
    assert_eq!(second_readback_words, AOL_1257_MARKED_WORDS);
    assert_eq!(second_send_words, AOL_1257_MARKED_WORDS);
    assert_eq!(second_after_send_count, 1);
    assert_eq!(second_after_send_placed_count, 1);
}
