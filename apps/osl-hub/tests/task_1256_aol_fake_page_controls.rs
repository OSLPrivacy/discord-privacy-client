use osl_privacy_hub::aol_fake_page::{AolFakePageFixture, AOL_TASK_1256_MARKED_WORDS};

#[test]
fn task_1256_aol_fake_page_place_read_send_and_refuse_send_removal() {
    let mut fixture = AolFakePageFixture::task_1256();
    let controls = fixture
        .mapped_controls()
        .iter()
        .map(|control| control.name)
        .collect::<Vec<_>>();

    println!(
        "TASK1256_AOL_INITIAL_SENT_COUNT={}",
        fixture.sent_email_count()
    );
    println!("TASK1256_AOL_CONTROLS={}", controls.join(","));

    assert_eq!(fixture.sent_email_count(), 0);
    assert_eq!(controls, ["Place", "Read", "Send"]);

    fixture
        .place()
        .expect("Place adds marked AOL cover message");
    println!(
        "TASK1256_AOL_AFTER_PLACE_PLACED_COUNT={}",
        fixture.placed_message_count()
    );
    println!(
        "TASK1256_AOL_AFTER_PLACE_MARKED_COUNT={}",
        fixture.marked_placed_message_count()
    );
    assert_eq!(fixture.placed_message_count(), 1);
    assert_eq!(fixture.marked_placed_message_count(), 1);

    let read = fixture.read().expect("Read returns placed cover message");
    println!("TASK1256_AOL_READ_WORDS={read}");
    assert_eq!(read, AOL_TASK_1256_MARKED_WORDS);

    fixture.send().expect("Send records one sent email");
    println!(
        "TASK1256_AOL_AFTER_SEND_SENT_COUNT={}",
        fixture.sent_email_count()
    );
    assert_eq!(fixture.sent_email_count(), 1);

    let placed_before_remove = fixture.placed_message_count();
    let sent_before_remove = fixture.sent_email_count();
    let remove = fixture
        .remove_control("Send")
        .expect_err("removing Send must be refused");
    println!("TASK1256_AOL_REMOVE_SEND_REFUSAL={remove}");
    println!(
        "TASK1256_AOL_AFTER_REMOVE_PLACED_COUNT={}",
        fixture.placed_message_count()
    );
    println!(
        "TASK1256_AOL_AFTER_REMOVE_SENT_COUNT={}",
        fixture.sent_email_count()
    );

    assert_eq!(remove, "AOL fake page Send control is required");
    assert_eq!(fixture.placed_message_count(), placed_before_remove);
    assert_eq!(fixture.sent_email_count(), sent_before_remove);
}
