use ipc::commands::{cmd_osl_list_email_send_modes, cmd_osl_open_email_send_review};
use ipc::email_send_modes::{
    apply_email_composer_input, open_email_send_review, visible_subject_protection_warning,
    EmailComposerInput, EmailComposerInputEffect, EmailSendMode, BORING_PROTECTED_SUBJECT,
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

/// TASK 1298 - the visible-subject warning has to reach the send review, not
/// only the send refusal. Fixture: one private subject, one "Hello", plus two
/// controls that keep the rule from collapsing into "long subjects warn".
#[test]
fn task_1298_send_review_warns_about_a_visible_private_subject_before_send() {
    let protected_body = "Meet me at the west loading door after payroll closes.";
    let private_subject = protected_body;
    let fixtures: [(&str, &str); 4] = [
        ("private subject", private_subject),
        ("Hello", "Hello"),
        (
            "unrelated long subject",
            "Quarterly logistics planning notes",
        ),
        ("boring replacement", BORING_PROTECTED_SUBJECT),
    ];

    let mut warned = 0usize;
    for (label, subject) in fixtures {
        let review = open_email_send_review(EmailSendMode::Manual, subject, protected_body);
        let rows = review.rows();

        println!(
            "TASK1298 fixture={label} subject={subject:?} warning={:?} rows={rows:?}",
            review.visible_subject_warning
        );

        assert_eq!(review.named_send_command, "Send");
        assert_eq!(rows[review.named_send_row_index()], "Send");

        if let Some(warning) = review.visible_subject_warning.as_deref() {
            let warning_index = review
                .visible_subject_warning_row_index()
                .expect("a warned review must place its warning in a row");
            println!(
                "TASK1298 fixture={label} warning_row={warning_index} send_row={}",
                review.named_send_row_index()
            );
            assert_eq!(warning, visible_subject_protection_warning());
            assert!(warning.contains(BORING_PROTECTED_SUBJECT));
            assert!(
                warning_index < review.named_send_row_index(),
                "the warning must be read before the named Send command"
            );
            warned += 1;
        } else {
            assert_eq!(rows.len(), 1);
            assert!(review.visible_subject_warning_row_index().is_none());
        }
    }

    // Exactly the private subject warns; "Hello" and both controls do not.
    let private = open_email_send_review(EmailSendMode::Manual, private_subject, protected_body);
    let hello = open_email_send_review(EmailSendMode::Manual, "Hello", protected_body);
    println!(
        "TASK1298 private_subject_warnings={} hello_warnings={} warned_fixtures={warned}",
        usize::from(private.warns_about_visible_subject()),
        usize::from(hello.warns_about_visible_subject())
    );
    assert!(private.warns_about_visible_subject());
    assert!(!hello.warns_about_visible_subject());
    assert_eq!(warned, 1);
}

/// The same review reaching the composer UI through the named command.
#[test]
fn task_1298_send_review_command_carries_the_warning_to_the_ui() {
    let protected_body = "Meet me at the west loading door after payroll closes.";

    let private = cmd_osl_open_email_send_review("manual", protected_body, protected_body)
        .expect("manual is a real send mode");
    let hello = cmd_osl_open_email_send_review("manual", "Hello", protected_body)
        .expect("manual is a real send mode");

    println!(
        "TASK1298 command private rows={:?} warning={:?}",
        private.rows, private.visible_subject_warning
    );
    println!(
        "TASK1298 command hello rows={:?} warning={:?}",
        hello.rows, hello.visible_subject_warning
    );

    assert_eq!(private.mode_name, "Manual");
    assert_eq!(
        private.visible_subject_warning.as_deref(),
        Some(visible_subject_protection_warning().as_str())
    );
    assert_eq!(
        private.rows,
        vec![visible_subject_protection_warning(), "Send".to_owned()]
    );
    assert_eq!(hello.visible_subject_warning, None);
    assert_eq!(hello.rows, vec!["Send".to_owned()]);
    assert!(cmd_osl_open_email_send_review("not_a_mode", "Hello", protected_body).is_err());
}
