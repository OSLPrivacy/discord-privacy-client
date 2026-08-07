#[path = "../src/native_outlook_adapter.rs"]
mod native_outlook_adapter;

use native_outlook_adapter::{
    OutlookDesktopEmailFixture, OutlookDesktopFixtureError, OUTLOOK_DESKTOP_TASK_1287_WORDS,
};

#[test]
fn task_1287_fake_page_outlook_desktop_email_flow_executes_controls_and_refuses_send_removal() {
    let mut fixture = OutlookDesktopEmailFixture::default();
    let start_sent_count = fixture.sent_count();
    let controls = fixture.named_control_names();

    println!("TASK1287_START_SENT_COUNT={start_sent_count}");
    println!("TASK1287_NAMED_CONTROLS={}", controls.join(","));

    assert_eq!(start_sent_count, 0);
    assert_eq!(controls, vec!["Compose", "Place", "Readback", "Send"]);

    let compose_words = fixture.compose().expect("Compose control executes");
    println!("TASK1287_COMPOSE_WORDS={compose_words}");
    assert_eq!(compose_words, OUTLOOK_DESKTOP_TASK_1287_WORDS);

    let place_words = fixture.place().expect("Place control executes");
    println!("TASK1287_PLACE_WORDS={place_words}");
    assert_eq!(place_words, OUTLOOK_DESKTOP_TASK_1287_WORDS);
    assert_eq!(fixture.placed_count(), 1);

    let readback_words = fixture.readback().expect("Readback control executes");
    println!("TASK1287_READBACK_WORDS={readback_words}");
    assert_eq!(readback_words, OUTLOOK_DESKTOP_TASK_1287_WORDS);

    let send_words = fixture.send().expect("Send control executes");
    println!("TASK1287_SEND_WORDS={send_words}");
    assert_eq!(send_words, OUTLOOK_DESKTOP_TASK_1287_WORDS);
    println!("TASK1287_SENT_COUNT_AFTER_SEND={}", fixture.sent_count());
    assert_eq!(fixture.sent_count(), 1);

    let placed_before_remove = fixture.placed_count();
    let sent_before_remove = fixture.sent_count();
    let remove_send = fixture.remove_control("Send");
    println!("TASK1287_REMOVE_SEND_RESULT={remove_send:?}");
    println!(
        "TASK1287_PLACED_COUNT_AFTER_REMOVE={}",
        fixture.placed_count()
    );
    println!("TASK1287_SENT_COUNT_AFTER_REMOVE={}", fixture.sent_count());

    assert_eq!(
        remove_send,
        Err(OutlookDesktopFixtureError::RequiredControlCannotBeRemoved(
            "Send".to_owned()
        ))
    );
    assert_eq!(fixture.placed_count(), placed_before_remove);
    assert_eq!(fixture.sent_count(), sent_before_remove);
    assert_eq!(fixture.named_control_names(), controls);
}
