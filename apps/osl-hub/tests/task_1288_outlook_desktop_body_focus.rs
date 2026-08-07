#[path = "../src/native_outlook_adapter.rs"]
mod native_outlook_adapter;

use native_outlook_adapter::{
    OutlookDesktopComposeField, OutlookDesktopEmailFixture, OutlookDesktopFixtureError,
    OUTLOOK_DESKTOP_TASK_1288_SUBJECT, OUTLOOK_DESKTOP_TASK_1288_WORDS,
};

#[test]
fn task_1288_outlook_desktop_place_refuses_subject_focus_after_body_place() {
    let mut fixture = OutlookDesktopEmailFixture::default();

    let before_count = fixture.placed_count();
    println!("TASK1288_DESKTOP_PLACEMENT_COUNT_BEFORE={before_count}");
    assert_eq!(before_count, 0);

    let composed = fixture
        .compose_with_body(OUTLOOK_DESKTOP_TASK_1288_WORDS)
        .expect("Outlook desktop compose starts with Body focused")
        .to_owned();
    println!("TASK1288_COMPOSE_BODY_WORDS={composed}");
    println!("TASK1288_INITIAL_FOCUS={}", fixture.focused_field_name());
    assert_eq!(composed, OUTLOOK_DESKTOP_TASK_1288_WORDS);
    assert_eq!(fixture.focused_field_name(), "Body");

    let body_place = fixture
        .place()
        .expect("Outlook desktop Body focus accepts placement")
        .to_owned();
    let after_body_count = fixture.placed_count();
    println!("TASK1288_BODY_PLACE_WORDS={body_place}");
    println!("TASK1288_DESKTOP_PLACEMENT_COUNT_AFTER_BODY={after_body_count}");
    println!("TASK1288_BODY_AFTER_BODY_PLACE={}", fixture.body_text());
    println!(
        "TASK1288_SUBJECT_AFTER_BODY_PLACE={}",
        fixture.subject_text()
    );
    assert_eq!(body_place, OUTLOOK_DESKTOP_TASK_1288_WORDS);
    assert_eq!(after_body_count, 1);
    assert_eq!(fixture.body_text(), OUTLOOK_DESKTOP_TASK_1288_WORDS);
    assert_eq!(fixture.subject_text(), OUTLOOK_DESKTOP_TASK_1288_SUBJECT);

    fixture.focus_field(OutlookDesktopComposeField::Subject);
    println!(
        "TASK1288_FOCUS_AFTER_CHANGE={}",
        fixture.focused_field_name()
    );
    assert_eq!(fixture.focused_field_name(), "Subject");

    let subject_place = fixture.place();
    println!("TASK1288_SUBJECT_PLACE_RESULT={subject_place:?}");
    println!("TASK1288_SUBJECT_FOCUS_REFUSAL=compose Body not focused");
    assert_eq!(
        subject_place,
        Err(OutlookDesktopFixtureError::ComposeBodyNotFocused(
            "Subject".to_owned()
        ))
    );

    let final_count = fixture.placed_count();
    println!("TASK1288_BODY_FINAL={}", fixture.body_text());
    println!("TASK1288_SUBJECT_FINAL={}", fixture.subject_text());
    println!("TASK1288_DESKTOP_PLACEMENT_COUNT_FINAL={final_count}");
    assert_eq!(fixture.body_text(), OUTLOOK_DESKTOP_TASK_1288_WORDS);
    assert_eq!(fixture.subject_text(), OUTLOOK_DESKTOP_TASK_1288_SUBJECT);
    assert_eq!(final_count, 1);
}
