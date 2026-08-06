use osl_privacy_hub::app_own_names::{
    mark_name_published_rows, AppOwnNameState, NamePublishedRow, RowWhoWroteIt,
};
use osl_privacy_hub::models::ServiceKind;

const OWNER: &str = "osl_task4072_owner";
const ACCOUNT: &str = "acct-task-4072";

#[test]
fn saved_app_own_names_mark_only_listed_names_and_refuse_empty_list() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store_path = dir.path().join("app-own-names.json");
    let state = AppOwnNameState::load(store_path.clone());

    let shown = state
        .record_connected_account_names(
            OWNER,
            ServiceKind::Telegram,
            ACCOUNT,
            vec!["Morgan Lee".to_owned()],
        )
        .expect("connected account names are saved");
    println!(
        "TASK4072 shown_to_person={} needs_confirmation={} visible_names={}",
        shown.shown_to_person,
        shown.needs_confirmation,
        shown.names.join("|")
    );
    assert!(shown.shown_to_person);
    assert!(shown.needs_confirmation);
    assert_eq!(shown.names, vec!["Morgan Lee".to_owned()]);

    let corrected = state
        .correction_for_person(
            OWNER,
            ServiceKind::Telegram,
            ACCOUNT,
            vec!["Morgan Lee".to_owned(), "M Lee".to_owned()],
        )
        .expect("person can correct the own-name list");
    println!(
        "TASK4072 corrected_names={} needs_confirmation_after_correction={}",
        corrected.names.join("|"),
        corrected.needs_confirmation
    );
    assert_eq!(
        corrected.names,
        vec!["M Lee".to_owned(), "Morgan Lee".to_owned()]
    );
    assert!(!corrected.needs_confirmation);

    let reloaded = AppOwnNameState::load(store_path);
    let rows = ten_rows();
    let with_list = reloaded
        .read_name_published_rows(OWNER, ServiceKind::Telegram, ACCOUNT, &rows)
        .expect("saved list reads back");
    let marked_yours = with_list
        .marks
        .iter()
        .filter(|mark| mark.answer == RowWhoWroteIt::Yours)
        .count();
    let marked_theirs = with_list
        .marks
        .iter()
        .filter(|mark| mark.answer == RowWhoWroteIt::Theirs)
        .count();
    let unknown_name_marked_yours = with_list
        .marks
        .iter()
        .any(|mark| mark.row_id == "row-4072-10" && mark.answer == RowWhoWroteIt::Yours);
    println!(
        "TASK4072 with_list rows={} yours={} theirs={} refused={} unknown_name_marked_yours={}",
        with_list.marks.len(),
        marked_yours,
        marked_theirs,
        with_list.refused,
        unknown_name_marked_yours
    );
    assert_eq!(with_list.marks.len(), 10);
    assert_eq!(marked_yours, 4);
    assert_eq!(marked_theirs, 6);
    assert_eq!(with_list.refused, 0);
    assert!(!unknown_name_marked_yours);

    let emptied = state
        .correction_for_person(OWNER, ServiceKind::Telegram, ACCOUNT, Vec::new())
        .expect("person can empty the own-name list");
    assert!(emptied.names.is_empty());
    let empty_answer = state
        .read_name_published_rows(OWNER, ServiceKind::Telegram, ACCOUNT, &rows)
        .expect("empty list fails closed as an answer");
    let empty_yours = empty_answer
        .marks
        .iter()
        .filter(|mark| mark.answer == RowWhoWroteIt::Yours)
        .count();
    println!(
        "TASK4072 empty_list rows={} yours={} refused={} refusal={}",
        empty_answer.marks.len(),
        empty_yours,
        empty_answer.refused,
        empty_answer.refusal.as_deref().unwrap_or("<none>")
    );
    assert_eq!(empty_answer.marks.len(), 10);
    assert_eq!(empty_yours, 0);
    assert_eq!(empty_answer.refused, 10);
    assert_eq!(
        empty_answer.refusal.as_deref(),
        Some("OSL: own-name list is empty; refused 10 name-published rows")
    );
}

#[test]
fn pure_classifier_never_uses_a_name_not_on_the_list() {
    let answer = mark_name_published_rows(&["Morgan Lee".to_owned()], &ten_rows());
    let not_on_list = answer
        .marks
        .iter()
        .find(|mark| mark.row_id == "row-4072-10")
        .expect("fixture includes the unknown-name row");
    assert_eq!(not_on_list.answer, RowWhoWroteIt::Theirs);
}

fn ten_rows() -> Vec<NamePublishedRow> {
    [
        ("row-4072-01", "Morgan Lee"),
        ("row-4072-02", "Ari Quinn"),
        ("row-4072-03", "Morgan Lee"),
        ("row-4072-04", "Sam Patel"),
        ("row-4072-05", "Devon Ray"),
        ("row-4072-06", "M Lee"),
        ("row-4072-07", "Ari Quinn"),
        ("row-4072-08", "Morgan Lee"),
        ("row-4072-09", "Taylor Chen"),
        ("row-4072-10", "Morgan Le"),
    ]
    .into_iter()
    .map(|(row_id, name)| NamePublishedRow {
        row_id: row_id.to_owned(),
        published_name: Some(name.to_owned()),
    })
    .collect()
}
