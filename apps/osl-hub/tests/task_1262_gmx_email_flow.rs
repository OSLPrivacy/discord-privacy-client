#[path = "../src/web_surface_adapter/gmx.rs"]
mod gmx;

use gmx::{GmxFakePageConnection, GmxFakePageError, GMX_1262_CONTROL_NAMES, GMX_1262_MARKED_WORDS};

#[test]
fn task_1262_gmx_fake_page_compose_place_readback_send_and_refuse_send_removal() {
    let mut fixture = GmxFakePageConnection::new_email_flow();

    let initial_sent = fixture.sent_email_count();
    let initial_controls = fixture.control_names();
    println!("TASK1262 initial_sent_email_count={initial_sent}");
    println!("TASK1262 named_controls={}", initial_controls.join(","));

    let compose_words = fixture
        .compose_marked_email()
        .expect("Compose produces the marked GMX email words")
        .to_owned();
    println!("TASK1262 compose_words={compose_words}");

    let place_words = fixture
        .place_composed_message()
        .expect("Place places the marked GMX email words")
        .to_owned();
    let after_place_count = fixture.placed_message_count();
    let after_place_sent = fixture.sent_email_count();
    println!("TASK1262 place_words={place_words}");
    println!("TASK1262 after_place_marked_message_count={after_place_count}");
    println!("TASK1262 after_place_sent_email_count={after_place_sent}");

    let readback_words = fixture
        .readback_marked_words()
        .expect("Readback returns the placed GMX marked words")
        .to_owned();
    println!("TASK1262 readback_words={readback_words}");

    let send_words = fixture
        .send_readback_message()
        .expect("Send raises the GMX sent count and preserves the words")
        .to_owned();
    let after_send_count = fixture.sent_email_count();
    let after_send_placed_count = fixture.placed_message_count();
    println!("TASK1262 send_words={send_words}");
    println!("TASK1262 after_send_sent_email_count={after_send_count}");
    println!("TASK1262 after_send_placed_message_count={after_send_placed_count}");

    let remove_send = fixture
        .remove_control("Send")
        .expect_err("removing Send is refused");
    let after_remove_placed_count = fixture.placed_message_count();
    let after_remove_sent_count = fixture.sent_email_count();
    println!("TASK1262 remove_send_refusal={remove_send:?}");
    println!("TASK1262 after_remove_send_placed_message_count={after_remove_placed_count}");
    println!("TASK1262 after_remove_send_sent_email_count={after_remove_sent_count}");

    assert_eq!(initial_sent, 0);
    assert_eq!(initial_controls, GMX_1262_CONTROL_NAMES);
    assert_eq!(compose_words, GMX_1262_MARKED_WORDS);
    assert_eq!(place_words, GMX_1262_MARKED_WORDS);
    assert_eq!(after_place_count, 1);
    assert_eq!(after_place_sent, 0);
    assert_eq!(readback_words, GMX_1262_MARKED_WORDS);
    assert_eq!(send_words, GMX_1262_MARKED_WORDS);
    assert_eq!(after_send_count, 1);
    assert_eq!(after_send_placed_count, 1);
    assert_eq!(
        remove_send,
        GmxFakePageError::ProtectedControlRemovalRefused("Send")
    );
    assert_eq!(after_remove_placed_count, after_send_placed_count);
    assert_eq!(after_remove_sent_count, after_send_count);
}
