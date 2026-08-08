#![cfg(feature = "core")]

use osl_privacy_hub::adapters::{Bounds, NodeRef};
use osl_privacy_hub::row_who_wrote_it::SharedRowWhoWroteIt;
use osl_privacy_hub::web_surface_adapter::x::{
    accept_x_web_rows, XTranscriptPagePartKind, XTranscriptRow,
};

fn rect() -> Bounds {
    Bounds {
        x: 10,
        y: 20,
        width: 300,
        height: 44,
    }
}

fn x_web_row_types_missing_either_field() -> usize {
    let source = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/web_surface_adapter/x.rs"),
    )
    .expect("read X web adapter source");
    let start = source
        .find("pub struct XTranscriptRow {")
        .expect("XTranscriptRow source is present");
    let block = &source[start..];
    let end = block.find("\n}").expect("XTranscriptRow block closes");
    let row_block = &block[..end];
    usize::from(
        !row_block.contains("pub message_text:")
            || !row_block.contains("pub who_wrote_it: SharedRowWhoWroteIt"),
    )
}

fn x_web_row_types_offering_only_two_answers() -> usize {
    let source = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/web_surface_adapter/x.rs"),
    )
    .expect("read X web adapter source");
    usize::from(
        source.contains("pub struct XTranscriptRow {") && SharedRowWhoWroteIt::STATES.len() == 2,
    )
}

#[test]
fn task_4502_x_web_rows_carry_words_and_exactly_three_who_wrote_it_answers() {
    let states = SharedRowWhoWroteIt::STATES
        .iter()
        .map(|state| state.as_str())
        .collect::<Vec<_>>();

    let mine = XTranscriptRow::from_web_reading_parts(
        rect(),
        Some("osl carrier marker".to_owned()),
        "task 4502 exact X web row words",
        Some("yours"),
        Some(NodeRef::for_claimed_node(4502)),
    )
    .expect("valid X row is accepted");
    assert_eq!(mine.message_text, "task 4502 exact X web row words");
    assert_eq!(mine.who_wrote_it, SharedRowWhoWroteIt::Yours);
    assert_eq!(
        mine.who_wrote_it_part.kind,
        XTranscriptPagePartKind::WhoWroteIt
    );
    assert_eq!(
        mine.who_wrote_it_part.kind.as_str(),
        "whoWroteIt",
        "the row points at the who-wrote-it page part kind"
    );

    let peer = XTranscriptRow::from_web_reading_parts(
        rect(),
        Some("peer carrier marker".to_owned()),
        "their visible row words",
        Some("theirs"),
        Some(NodeRef::for_claimed_node(4503)),
    )
    .expect("peer X row is accepted");
    assert_eq!(peer.who_wrote_it, SharedRowWhoWroteIt::Theirs);

    let made_up_fourth = XTranscriptRow::from_web_reading_parts(
        rect(),
        None,
        "made-up answer row",
        Some("ghost_writer"),
        Some(NodeRef::for_claimed_node(4504)),
    )
    .expect_err("made-up fourth who-wrote-it answer is refused");

    let no_sender = XTranscriptRow::from_web_reading_parts(
        rect(),
        Some("senderless carrier marker".to_owned()),
        "senderless visible row words",
        None,
        None,
    )
    .expect("a real page row with no sender reaches not_published_by_app");
    assert_eq!(
        no_sender.who_wrote_it,
        SharedRowWhoWroteIt::NotPublishedByApp
    );
    let not_published_result = accept_x_web_rows(&[no_sender]);
    assert_eq!(not_published_result.accepted_rows, 0);
    assert_eq!(
        not_published_result.refusal.as_deref(),
        Some("OSL: row x-web-row-0 was not published by the app")
    );

    let published_result = accept_x_web_rows(&[mine, peer]);
    assert_eq!(published_result.accepted_rows, 2);
    assert_eq!(published_result.refusal, None);

    let missing_either_field = x_web_row_types_missing_either_field();
    let only_two_answers = x_web_row_types_offering_only_two_answers();

    println!("TASK4502_X_WEB_ROW_MESSAGE_TEXT=task 4502 exact X web row words");
    println!("TASK4502_WHO_WROTE_IT_STATE_COUNT={}", states.len());
    println!("TASK4502_WHO_WROTE_IT_STATES={}", states.join(","));
    println!(
        "TASK4502_MADE_UP_FOURTH_REFUSAL={}",
        made_up_fourth.reason()
    );
    println!("TASK4502_NO_SENDER_WHO_WROTE_IT=not_published_by_app");
    println!(
        "TASK4502_NO_SENDER_ACCEPTED_ROWS={}",
        not_published_result.accepted_rows
    );
    println!(
        "TASK4502_NO_SENDER_REFUSAL={}",
        not_published_result.refusal.as_deref().unwrap_or("<none>")
    );
    println!(
        "TASK4502_PUBLISHED_ACCEPTED_ROWS={}",
        published_result.accepted_rows
    );
    println!(
        "TASK4502_WHO_WROTE_IT_PART_KIND={}",
        XTranscriptPagePartKind::WhoWroteIt.as_str()
    );
    println!("TASK4502_WEB_ROW_TYPES_MISSING_EITHER_FIELD={missing_either_field}");
    println!("TASK4502_WEB_ROW_TYPES_OFFER_ONLY_TWO_ANSWERS={only_two_answers}");

    assert_eq!(states, vec!["yours", "theirs", "not_published_by_app"]);
    assert_eq!(
        made_up_fourth.reason(),
        "OSL: unknown who-wrote-it answer ghost_writer"
    );
    assert_eq!(missing_either_field, 0);
    assert_eq!(only_two_answers, 0);
}
