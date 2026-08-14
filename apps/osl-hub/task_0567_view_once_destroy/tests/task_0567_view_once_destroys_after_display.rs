//! TASK 0567: a displayed view-once message is destroyed immediately.

use task_0567_view_once_destroy::view_once_open::NamedViewOnceCopies;

#[test]
fn task_0567_view_once_destroys_content_after_display() {
    let named_copy = "task-0567-named-copy";
    let item_id = format!("task-0567-item-{:016x}", rand::random::<u64>());
    let mark = format!("TASK0567-MARK-{:032x}", rand::random::<u128>());
    let mut copies = NamedViewOnceCopies::default();
    copies.insert_mark(named_copy, item_id.as_str(), mark.as_str());

    let exact_before = copies
        .read_mark(named_copy, item_id.as_str())
        .expect("the random marked view-once message is readable before opening")
        .to_owned();
    let count_before = copies.count(named_copy);
    println!(
        "TASK0567_BEFORE named_copy={named_copy} exact_mark={exact_before} count={count_before}"
    );
    assert_eq!(exact_before, mark);
    assert_eq!(count_before, 1);

    let first_open = copies.request_open(named_copy, item_id.as_str());
    let first_open_mark_count = usize::from(first_open.content == mark);
    println!(
        "TASK0567_FIRST_OPEN exit={} exact_mark={} mark_count={first_open_mark_count}",
        first_open.exit_code, first_open.content
    );
    assert_eq!(first_open.exit_code, 0);
    assert_eq!(first_open.content, mark);
    assert_eq!(first_open_mark_count, 1);

    let count_after = copies.count(named_copy);
    assert_eq!(count_after, 0);

    let second_open = copies.request_open(named_copy, item_id.as_str());
    println!(
        "TASK0567_AFTER count={count_after} second_open_exit={} second_open_content_len={}",
        second_open.exit_code,
        second_open.content.len()
    );
    assert_eq!(second_open.exit_code, 1);
    assert!(second_open.content.is_empty());
}
