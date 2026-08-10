use ipc::commands::cmd_osl_list_email_send_modes;
use ipc::email_send_modes::{
    apply_email_composer_input, prepare_hidden_email_subject, EmailComposerInput,
    EmailComposerInputEffect, EmailSendMode, DEFAULT_PLAIN_EMAIL_SUBJECT,
};

#[test]
fn email_send_modes_command_lists_exactly_five_email_choices() {
    let modes = cmd_osl_list_email_send_modes();
    let names: Vec<&str> = modes.iter().map(|mode| mode.name.as_str()).collect();

    println!("email choices count: {}", names.len());
    println!("email choices: {}", names.join(", "));

    assert_eq!(
        names,
        vec![
            "Manual",
            "Double Enter",
            "Experimental Single Enter",
            "Instant",
            "Match typing",
        ]
    );
}

#[test]
fn task_1223_email_enter_adds_line_in_all_five_modes() {
    let mut line_insertions = 0usize;
    let mut send_review_rules = Vec::new();

    for mode in EmailSendMode::ALL {
        let entered = apply_email_composer_input(mode, "MAPLE-4172", EmailComposerInput::Enter);
        let reviewed =
            apply_email_composer_input(mode, &entered.body, EmailComposerInput::NamedSend);

        println!(
            "email enter check: mode={} effect={} body={:?}",
            mode.name(),
            entered.effect.name(),
            entered.body
        );
        println!(
            "email send review rule: mode={} rule={}",
            mode.name(),
            reviewed
                .send_review_rule
                .expect("named send opens send review")
                .name()
        );

        assert_eq!(entered.effect, EmailComposerInputEffect::InsertLine);
        assert_eq!(entered.body, "MAPLE-4172\n");
        assert!(entered.send_review_rule.is_none());
        assert_eq!(reviewed.effect, EmailComposerInputEffect::OpenSendReview);
        assert_eq!(reviewed.body, "MAPLE-4172\n");
        assert_eq!(
            reviewed
                .send_review_rule
                .expect("named send opens send review")
                .id(),
            mode.id()
        );

        line_insertions += entered.body.matches('\n').count();
        send_review_rules.push(format!(
            "{}={}",
            mode.name(),
            reviewed
                .send_review_rule
                .expect("named send opens send review")
                .name()
        ));
    }

    println!("email Enter adds a line mode count: {line_insertions}");
    println!(
        "email send review follows chosen rules: {}",
        send_review_rules.join(", ")
    );

    assert_eq!(line_insertions, 5);
}

#[test]
fn task_3139_empty_hidden_email_subject_defaults_and_typed_subject_stays_exact() {
    let empty = prepare_hidden_email_subject("");
    let typed_input = "CUSTOM-3139  Mixed Case / punctuation! ";
    let typed = prepare_hidden_email_subject(typed_input);

    let saved_choice = DEFAULT_PLAIN_EMAIL_SUBJECT;
    let default_read_back = empty.read_back();
    let typed_read_back = typed.read_back();
    let typed_change_count = usize::from(typed_read_back != typed_input);

    println!("TASK3139 empty_subject={default_read_back:?}");
    println!("TASK3139 typed_subject={typed_read_back:?}");
    println!("TASK3139 typed_subject_change_count={typed_change_count}");
    println!("TASK3139 saved_choice={saved_choice:?}");
    println!("TASK3139 choice_read_back={default_read_back:?}");

    assert_eq!(default_read_back, "Quick note");
    assert_eq!(default_read_back, saved_choice);
    assert_eq!(typed_read_back, typed_input);
    assert_eq!(typed_change_count, 0);

    // Sending consumes the prepared value and still returns the exact subject
    // that was read back, rather than resolving the default a second time.
    assert_eq!(empty.into_subject(), saved_choice);
    assert_eq!(typed.into_subject(), typed_input);
}
