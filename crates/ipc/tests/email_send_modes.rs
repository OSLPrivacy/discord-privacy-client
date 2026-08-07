use ipc::commands::cmd_osl_list_email_send_modes;
use ipc::email_send_modes::{
    apply_email_composer_input, EmailComposerInput, EmailComposerInputEffect,
    EmailDraftReadbackState, EmailSendMode,
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
fn task_1212_changed_draft_readback_fails_before_send() {
    let mut draft = EmailDraftReadbackState::place("MAPLE-4172");

    println!(
        "TASK1212 matching_readback_count_before={}",
        draft.matching_readback_count()
    );
    assert_eq!(draft.matching_readback_count(), 0);

    let good = draft
        .readback("MAPLE-4172")
        .expect("matching readback should be accepted");
    println!("TASK1212 returned_readback={}", good.returned_readback);
    println!(
        "TASK1212 matching_readback_count_after_good={}",
        draft.matching_readback_count()
    );
    assert_eq!(good.returned_readback, "MAPLE-4172");
    assert_eq!(good.matching_readback_count, 1);
    assert_eq!(draft.matching_readback_count(), 1);

    let refusal = draft
        .send_after_readback("MAPLE-4173")
        .expect_err("changed readback must be refused before Send");
    println!("TASK1212 changed_readback=MAPLE-4173");
    println!("TASK1212 changed_readback_refusal={refusal}");
    assert_eq!(refusal, "readback mismatch");

    println!("TASK1212 body_still_reads={}", draft.body());
    println!(
        "TASK1212 matching_readback_count_after_bad={}",
        draft.matching_readback_count()
    );
    println!("TASK1212 send_count={}", draft.send_count());
    assert_eq!(draft.body(), "MAPLE-4172");
    assert_eq!(draft.matching_readback_count(), 1);
    assert_eq!(draft.send_count(), 0);
}
