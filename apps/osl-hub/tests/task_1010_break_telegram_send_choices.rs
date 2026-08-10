use osl_privacy_hub::telegram_send_choices::{TelegramCoverInsertion, TelegramSendButton};

const PRIVATE_TEXT: &[u8] = b"telegram-send-1010";
const ALLOWED_TRIGGERS: [&str; 3] = ["Enter", "Enter x2", "Clipboard"];
const RETIRED_TRIGGERS: [&str; 3] = ["Manual", "Instant", "Match typing"];

#[test]
fn task_1010_every_telegram_send_choice_requires_private_text() {
    let button =
        TelegramSendButton::for_selected_choices("Enter", TelegramCoverInsertion::InsertOnSend)
            .expect("Enter must remain an available Telegram send trigger");
    let first_prepared = [button
        .prepare_selected_cover(PRIVATE_TEXT)
        .expect("good private text must prepare a cover")];
    assert_eq!(first_prepared.len(), 1);
    assert_eq!(first_prepared[0].cover, PRIVATE_TEXT);
    println!(
        "TASK1010_GOOD trigger=Enter prepared_cover_count={} prepared_cover={}",
        first_prepared.len(),
        String::from_utf8_lossy(&first_prepared[0].cover)
    );

    for trigger in ALLOWED_TRIGGERS {
        let button =
            TelegramSendButton::for_selected_choices(trigger, TelegramCoverInsertion::InsertOnSend)
                .unwrap_or_else(|error| panic!("{trigger} must connect: {error}"));
        let error = button
            .prepare_selected_cover(&[])
            .expect_err("empty private text must be refused");
        assert_eq!(
            error,
            format!("OSL: Telegram send refused: empty private text for trigger '{trigger}'")
        );
        println!("TASK1010_EMPTY trigger={trigger} prepared_cover_count=0 error={error}");
    }

    for trigger in RETIRED_TRIGGERS {
        let error =
            TelegramSendButton::for_selected_choices(trigger, TelegramCoverInsertion::InsertOnSend)
                .expect_err("retired triggers must remain refused");
        assert_eq!(
            error,
            format!("OSL: Telegram send refused: unsupported trigger '{trigger}'")
        );
        println!("TASK1010_RETIRED trigger={trigger} prepared_cover_count=0 error={error}");
    }

    let afterward_prepared = [button
        .prepare_selected_cover(PRIVATE_TEXT)
        .expect("a prior refusal must not affect later good private text")];
    assert_eq!(afterward_prepared, first_prepared);
    println!(
        "TASK1010_AFTERWARD trigger=Enter prepared_cover_count={} prepared_cover={}",
        afterward_prepared.len(),
        String::from_utf8_lossy(&afterward_prepared[0].cover)
    );
}
