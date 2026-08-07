#![cfg(feature = "core")]

use osl_privacy_hub::visible_osl_mark::{
    put_visible_osl_mark, take_visible_osl_mark_out, VisibleOslMarkAppView, VisibleOslMarkRequest,
    VisibleOslMarkState, VISIBLE_OSL_MARK,
};

#[test]
fn task_0284_visible_osl_mark_put_remove_and_silent_noop() {
    let app_view = VisibleOslMarkAppView::DiscordProfileName;
    let original = "Pine Channel";
    let marked = format!("{original}{VISIBLE_OSL_MARK}");

    let put = put_visible_osl_mark(&VisibleOslMarkRequest::new(
        app_view,
        original,
        VisibleOslMarkState::Visible,
    ))
    .expect("visible mark put command");
    println!("TASK0284_APP_VIEW={}", put.app_view.as_str());
    println!("TASK0284_MARK_STRING={}", put.mark);
    println!("TASK0284_PUT_BEFORE={}", put.before);
    println!("TASK0284_PUT_AFTER={}", put.after);
    println!("TASK0284_PUT_CHANGED={}", put.changed);

    assert_eq!(put.before, original);
    assert_eq!(put.after, marked);
    assert!(put.changed);

    let remove = take_visible_osl_mark_out(&VisibleOslMarkRequest::new(
        app_view,
        put.after.clone(),
        VisibleOslMarkState::Visible,
    ))
    .expect("visible mark remove command");
    println!("TASK0284_REMOVE_BEFORE={}", remove.before);
    println!("TASK0284_REMOVE_AFTER={}", remove.after);
    println!("TASK0284_REMOVE_CHANGED={}", remove.changed);

    assert_eq!(remove.before, marked);
    assert_eq!(remove.after, original);
    assert!(remove.changed);

    let silent = put_visible_osl_mark(&VisibleOslMarkRequest::new(
        app_view,
        original,
        VisibleOslMarkState::Silent,
    ))
    .expect("silent mark state command");
    println!("TASK0284_SILENT_STATE={}", silent.mark_state.as_str());
    println!("TASK0284_SILENT_BEFORE={}", silent.before);
    println!("TASK0284_SILENT_AFTER={}", silent.after);
    println!("TASK0284_SILENT_CHANGED={}", silent.changed);

    assert_eq!(silent.before, original);
    assert_eq!(silent.after, original);
    assert!(!silent.changed);
}
