use osl_privacy_hub::telegram_send_choices::{
    prepare_telegram_send, TelegramCoverInsertion, TelegramSendTrigger,
};

const COVER: &[u8] = b"telegram-cover-1008\0\xff\xc3(";

#[test]
fn task_1008_prepares_only_three_telegram_triggers_and_reports_insertion_separately() {
    let accepted = ["Enter", "Enter x2", "Clipboard"];
    let accepted_labels: Vec<&str> = TelegramSendTrigger::ALL
        .iter()
        .map(|trigger| trigger.label())
        .collect();
    assert_eq!(accepted_labels, accepted);

    for (index, trigger) in accepted.iter().enumerate() {
        let insertion = TelegramCoverInsertion::ALL[index % TelegramCoverInsertion::ALL.len()];
        let receipt = prepare_telegram_send(trigger, insertion, COVER)
            .unwrap_or_else(|error| panic!("{trigger} must prepare: {error}"));

        assert_eq!(receipt.trigger.label(), *trigger);
        assert_eq!(receipt.cover_insertion, insertion);
        assert_eq!(receipt.cover_insertion.label(), insertion.label());
        assert_eq!(receipt.cover, COVER);

        println!("TASK1008_PREPARED_TRIGGER={}", receipt.trigger.label());
        println!(
            "TASK1008_COVER_INSERTION_FOR_{}={}",
            receipt.trigger.label(),
            receipt.cover_insertion.label()
        );
        println!(
            "TASK1008_COVER_BYTES_FOR_{}={}",
            receipt.trigger.label(),
            receipt.cover.len()
        );
    }

    let insertion_labels: Vec<&str> = TelegramCoverInsertion::ALL
        .iter()
        .map(|choice| choice.label())
        .collect();
    assert_eq!(insertion_labels, ["Insert on send", "Type naturally"]);
    println!("TASK1008_ACCEPTED_TRIGGER_COUNT={}", accepted_labels.len());
    println!(
        "TASK1008_COVER_INSERTION_CHOICES={}",
        insertion_labels.join(",")
    );
    println!("TASK1008_COVER_BYTES_PRESERVED={}", COVER.len());

    let refused = [
        "Manual",
        "Instant",
        "Match typing",
        "Enter ",
        "enter",
        "Other",
        "",
    ];
    for trigger in refused {
        let error = prepare_telegram_send(trigger, TelegramCoverInsertion::InsertOnSend, COVER)
            .expect_err("every trigger outside the exact allowlist must be refused");
        assert_eq!(
            error,
            format!("OSL: Telegram send refused: unsupported trigger '{trigger}'")
        );
        println!("TASK1008_REFUSED_TRIGGER={trigger}");
    }
    println!("TASK1008_NAMED_REFUSAL_COUNT=3");
    println!("TASK1008_OTHER_REFUSAL_SAMPLES={}", refused.len() - 3);
}
