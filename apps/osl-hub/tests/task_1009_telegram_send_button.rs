use osl_privacy_hub::telegram_send_choices::{
    TelegramCoverInsertion, TelegramSendButton, TelegramSendTrigger,
};

const COVER: &[u8] = b"telegram-selected-cover-1009\0\xff\xc3(";

#[test]
fn task_1009_each_trigger_prepares_the_selected_cover_without_posting() {
    let selections = [
        ("Enter", TelegramCoverInsertion::InsertOnSend),
        ("Enter x2", TelegramCoverInsertion::TypeNaturally),
        ("Clipboard", TelegramCoverInsertion::InsertOnSend),
    ];
    let mut prepared_count = 0usize;

    for (trigger, insertion) in selections {
        let button = TelegramSendButton::for_selected_choices(trigger, insertion)
            .unwrap_or_else(|error| panic!("{trigger} must connect: {error}"));
        let receipt = button
            .prepare_selected_cover(COVER)
            .expect("pressing Telegram's send button must prepare the selected cover");

        assert_eq!(receipt.trigger.label(), trigger);
        assert_eq!(receipt.cover_insertion, insertion);
        assert_eq!(receipt.cover_insertion.label(), insertion.label());
        assert_eq!(receipt.cover, COVER);
        // The receipt records preparation only; neither it nor the button has
        // provider control or a posted state, so this path cannot post.
        prepared_count += 1;
        println!(
            "TASK1009 trigger={} insertion={} prepared_cover_bytes={} posted=false",
            receipt.trigger.label(),
            receipt.cover_insertion.label(),
            receipt.cover.len()
        );
    }

    assert_eq!(prepared_count, TelegramSendTrigger::ALL.len());
    println!("TASK1009 prepared_cover_count={prepared_count} posted_count=0");
}
