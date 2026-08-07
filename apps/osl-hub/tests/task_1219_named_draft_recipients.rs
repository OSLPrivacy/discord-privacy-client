use osl_privacy_hub::osl_mail::{read_draft_recipients_from_named_controls, MailDraftControl};

fn control(name: &str, value: &str) -> MailDraftControl {
    MailDraftControl {
        name: name.to_owned(),
        value: value.to_owned(),
    }
}

#[test]
fn task_1219_fixture_returns_one_address_in_to_cc_and_bcc() {
    let fixture = vec![
        control(
            "Subject",
            "addresses here are ignored: subject@example.test",
        ),
        control("To", "alice.to@example.test"),
        control("CC", "bob.cc@example.test"),
        control("BCC", "carol.bcc@example.test"),
        control("Message", "body@example.test must not become a recipient"),
    ];

    let recipients = read_draft_recipients_from_named_controls(&fixture);

    println!("TASK1219_TO_COUNT={}", recipients.to.len());
    println!("TASK1219_TO_ADDRESS={}", recipients.to.join(","));
    println!("TASK1219_CC_COUNT={}", recipients.cc.len());
    println!("TASK1219_CC_ADDRESS={}", recipients.cc.join(","));
    println!("TASK1219_BCC_COUNT={}", recipients.bcc.len());
    println!("TASK1219_BCC_ADDRESS={}", recipients.bcc.join(","));

    assert_eq!(recipients.to, ["alice.to@example.test"]);
    assert_eq!(recipients.cc, ["bob.cc@example.test"]);
    assert_eq!(recipients.bcc, ["carol.bcc@example.test"]);
}
